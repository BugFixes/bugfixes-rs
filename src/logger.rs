use crate::config::Config;
use reqwest::blocking::Client;
use serde::Serialize;
use std::backtrace::Backtrace;
use std::fmt;
use std::panic::Location;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Debug,
    Log,
    Info,
    Warn,
    Error,
    Crash,
    Fatal,
    Unknown,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Log => "log",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Crash => "crash",
            Self::Fatal => "fatal",
            Self::Unknown => "unknown",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Debug => "Debug",
            Self::Log => "Log",
            Self::Info => "Info",
            Self::Warn => "Warn",
            Self::Error => "Error",
            Self::Crash => "Crash",
            Self::Fatal => "Fatal",
            Self::Unknown => "Unknown",
        }
    }

    pub fn numeric(self) -> u8 {
        match self {
            Self::Debug => 1,
            Self::Log => 2,
            Self::Info => 3,
            Self::Warn => 4,
            Self::Error => 5,
            Self::Crash | Self::Fatal => 6,
            Self::Unknown => 9,
        }
    }

    pub fn captures_stack(self) -> bool {
        matches!(self, Self::Debug | Self::Warn | Self::Error | Self::Crash | Self::Fatal)
    }
}

impl From<&str> for Level {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "debug" => Self::Debug,
            "log" => Self::Log,
            "info" => Self::Info,
            "warn" => Self::Warn,
            "error" => Self::Error,
            "crash" | "panic" | "fatal" => Self::Crash,
            "unknown" => Self::Unknown,
            other => match other.parse::<u8>() {
                Ok(level) if level < 9 => match level {
                    1 => Self::Debug,
                    2 => Self::Log,
                    3 => Self::Info,
                    4 => Self::Warn,
                    5 => Self::Error,
                    6 => Self::Crash,
                    _ => Self::Unknown,
                },
                _ => Self::Unknown,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BugfixesError {
    message: String,
}

impl BugfixesError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for BugfixesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BugfixesError {}

#[derive(Debug)]
pub enum ReportError {
    MissingCredentials,
    Http(reqwest::Error),
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCredentials => f.write_str("missing BUGFIXES_AGENT_KEY or BUGFIXES_AGENT_SECRET"),
            Self::Http(err) => write!(f, "http error: {err}"),
        }
    }
}

impl std::error::Error for ReportError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LogRecord {
    pub log: String,
    pub level: String,
    pub file: String,
    pub line: String,
    pub line_number: u32,
    pub log_fmt: String,
    pub stack: Option<String>,
}

static GLOBAL_LOGGER: OnceLock<BugfixesLogger> = OnceLock::new();

#[derive(Clone)]
pub struct BugfixesLogger {
    config: Config,
    client: Client,
}

impl BugfixesLogger {
    pub fn new(config: Config) -> Result<Self, reqwest::Error> {
        let client = Client::builder().timeout(config.timeout).build()?;
        Ok(Self { config, client })
    }

    pub fn from_env() -> Result<Self, reqwest::Error> {
        Self::new(Config::from_env())
    }

    pub fn local() -> Result<Self, reqwest::Error> {
        let mut config = Config::from_env();
        config.local_only = true;
        Self::new(config)
    }

    pub fn init_global(self) -> Result<&'static Self, Self> {
        GLOBAL_LOGGER.set(self)?;
        Ok(global_logger())
    }

    pub fn global() -> Option<&'static Self> {
        GLOBAL_LOGGER.get()
    }

    #[track_caller]
    pub fn info(&self, message: impl Into<String>) -> Result<String, ReportError> {
        self.emit(Level::Info, message.into())
    }

    #[track_caller]
    pub fn debug(&self, message: impl Into<String>) -> Result<String, ReportError> {
        self.emit(Level::Debug, message.into())
    }

    #[track_caller]
    pub fn log(&self, message: impl Into<String>) -> Result<String, ReportError> {
        self.emit(Level::Log, message.into())
    }

    #[track_caller]
    pub fn warn(&self, message: impl Into<String>) -> Result<String, ReportError> {
        self.emit(Level::Warn, message.into())
    }

    #[track_caller]
    pub fn error(&self, message: impl Into<String>) -> Result<BugfixesError, ReportError> {
        let message = message.into();
        self.emit(Level::Error, message.clone())?;
        Ok(BugfixesError::new(message))
    }

    #[track_caller]
    pub fn fatal(&self, message: impl Into<String>) -> ! {
        let message = message.into();
        let _ = self.emit(Level::Fatal, message.clone());
        panic!("{message}");
    }

    #[track_caller]
    pub fn record(&self, level: Level, message: impl Into<String>) -> LogRecord {
        let location = Location::caller();
        let message = message.into();
        let stack = level.captures_stack().then(capture_stack);
        let record = LogRecord {
            log_fmt: render_logfmt(level, location.file(), location.line(), &message),
            log: message,
            level: level.as_str().to_string(),
            file: location.file().to_string(),
            line: location.line().to_string(),
            line_number: location.line(),
            stack,
        };

        print_record(&record);
        record
    }

    #[track_caller]
    fn emit(&self, level: Level, message: String) -> Result<String, ReportError> {
        let record = self.record(level, message);
        if self.should_report(level) {
            self.send(&record)?;
        }
        Ok(format!("{}: {}", level.display_name(), record.log))
    }

    fn should_report(&self, level: Level) -> bool {
        if self.config.local_only {
            return false;
        }

        let configured = Level::from(self.config.log_level.as_str());
        configured == Level::Unknown || level.numeric() >= configured.numeric()
    }

    fn send(&self, record: &LogRecord) -> Result<(), ReportError> {
        if self.config.agent_key.is_empty() || self.config.agent_secret.is_empty() {
            return Err(ReportError::MissingCredentials);
        }

        self.client
            .post(self.config.log_endpoint())
            .header("Content-Type", "application/json")
            .header("X-API-KEY", &self.config.agent_key)
            .header("X-API-SECRET", &self.config.agent_secret)
            .json(record)
            .send()
            .map(|_| ())
            .map_err(ReportError::Http)
    }
}

pub fn init_global(logger: BugfixesLogger) -> Result<&'static BugfixesLogger, BugfixesLogger> {
    logger.init_global()
}

pub fn init_global_from_env() -> Result<&'static BugfixesLogger, reqwest::Error> {
    let logger = BugfixesLogger::from_env()?;
    Ok(init_global(logger).unwrap_or_else(|_| global_logger()))
}

pub fn init_global_local() -> Result<&'static BugfixesLogger, reqwest::Error> {
    let logger = BugfixesLogger::local()?;
    Ok(init_global(logger).unwrap_or_else(|_| global_logger()))
}

pub fn global_logger() -> &'static BugfixesLogger {
    GLOBAL_LOGGER.get_or_init(|| {
        BugfixesLogger::from_env().expect("failed to initialize global Bugfixes logger from environment")
    })
}

fn render_logfmt(level: Level, file: &str, line: u32, message: &str) -> String {
    format!(
        "path={} level={} msg={} line={}",
        quote_logfmt(file),
        level.as_str(),
        quote_logfmt(message),
        line
    )
}

fn quote_logfmt(input: &str) -> String {
    if input
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':'))
    {
        input.to_string()
    } else {
        format!("\"{}\"", input.replace('"', "\\\""))
    }
}

fn capture_stack() -> String {
    Backtrace::force_capture().to_string()
}

fn print_record(record: &LogRecord) {
    eprintln!(
        "{}: {} >> {}:{}",
        capitalize(&record.level),
        record.log,
        record.file,
        record.line_number
    );
    if record.stack.is_some() {
        eprintln!("Stack captured");
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BugfixesLogger, Level, ReportError, capitalize, global_logger, init_global, quote_logfmt,
        render_logfmt,
    };
    use crate::Config;

    #[test]
    fn level_parsing_matches_go_mapping() {
        assert_eq!(Level::from("debug"), Level::Debug);
        assert_eq!(Level::from("log"), Level::Log);
        assert_eq!(Level::from("info"), Level::Info);
        assert_eq!(Level::from("warn"), Level::Warn);
        assert_eq!(Level::from("error"), Level::Error);
        assert_eq!(Level::from("crash"), Level::Crash);
        assert_eq!(Level::from("fatal"), Level::Crash);
        assert_eq!(Level::from("10"), Level::Unknown);
        assert_eq!(Level::from("nonsense"), Level::Unknown);
    }

    #[test]
    fn logger_local_info_returns_string() {
        let logger = BugfixesLogger::new(Config {
            local_only: true,
            ..Config::default()
        })
        .expect("logger");

        let response = logger.info("server started").expect("info");
        assert_eq!(response, "Info: server started");
    }

    #[test]
    fn logger_local_error_returns_custom_error() {
        let logger = BugfixesLogger::new(Config {
            local_only: true,
            ..Config::default()
        })
        .expect("logger");

        let response = logger.error("boom").expect("error");
        assert_eq!(response.to_string(), "boom");
    }

    #[test]
    fn logger_remote_requires_credentials() {
        let logger = BugfixesLogger::new(Config {
            local_only: false,
            log_level: "info".into(),
            ..Config::default()
        })
        .expect("logger");

        let err = logger.info("hello").expect_err("missing creds");
        assert!(matches!(err, ReportError::MissingCredentials));
    }

    #[test]
    fn record_captures_callsite_and_stack() {
        let logger = BugfixesLogger::new(Config {
            local_only: true,
            ..Config::default()
        })
        .expect("logger");

        let record = logger.record(Level::Warn, "careful");
        assert_eq!(record.level, "warn");
        assert_eq!(record.log, "careful");
        assert!(record.file.ends_with("src/logger.rs"));
        assert!(record.line_number > 0);
        assert!(record.stack.as_ref().is_some_and(|stack| !stack.is_empty()));
    }

    #[test]
    fn logfmt_rendering_quotes_when_needed() {
        assert_eq!(quote_logfmt("simple/path.rs"), "simple/path.rs");
        assert_eq!(quote_logfmt("hello world"), "\"hello world\"");
        assert!(render_logfmt(Level::Info, "src/main.rs", 42, "hello world").contains("msg=\"hello world\""));
    }

    #[test]
    fn capitalize_handles_empty_strings() {
        assert_eq!(capitalize("warn"), "Warn");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn global_init_is_available() {
        let logger = BugfixesLogger::new(Config {
            local_only: true,
            ..Config::default()
        })
        .expect("logger");

        let global = init_global(logger).unwrap_or_else(|_| global_logger());
        let response = global.info("global logger").expect("info");
        assert_eq!(response, "Info: global logger");
    }
}
