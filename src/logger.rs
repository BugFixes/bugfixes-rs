use crate::config::Config;
use reqwest::Client;
use serde::Serialize;
use std::backtrace::Backtrace;
use std::fmt;
use std::future::Future;
use std::io::{self, IsTerminal, Write};
use std::panic::{Location, PanicHookInfo};
use std::str::FromStr;
use std::sync::OnceLock;
use std::thread::{self, JoinHandle};
use time::{OffsetDateTime, UtcOffset};
use tokio::runtime::Builder;

/// Severity of a log record, lowest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Level {
    /// Diagnostic detail. Captures a stack trace.
    Debug,
    /// General output.
    Log,
    /// Normal operational messages.
    Info,
    /// Something unexpected that the program recovered from. Captures a stack trace.
    Warn,
    /// A failure. Captures a stack trace.
    Error,
    /// A panic.
    Crash,
    /// An unrecoverable error; [`BugfixesLogger::fatal`] panics after sending it.
    Fatal,
    /// A level that is not recognised.
    Unknown,
}

impl Level {
    /// The lowercase name sent to Bugfixes, such as `"warn"`.
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

    /// The numeric severity Bugfixes uses: 1 (debug) to 6 (crash and fatal), 9 for unknown.
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

    /// Whether records at this level carry a stack trace when reported.
    pub fn captures_stack(self) -> bool {
        matches!(
            self,
            Self::Debug | Self::Warn | Self::Error | Self::Crash | Self::Fatal
        )
    }
}

/// Prints the capitalised name, such as `Warn`.
impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.as_str();
        f.write_str(&name[..1].to_ascii_uppercase())?;
        f.write_str(&name[1..])
    }
}

/// Parses a level name (case-insensitive; `panic` and `fatal` mean
/// [`Level::Crash`]) or its number, 1 to 6.
impl FromStr for Level {
    type Err = ParseLevelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "debug" | "1" => Ok(Self::Debug),
            "log" | "2" => Ok(Self::Log),
            "info" | "3" => Ok(Self::Info),
            "warn" | "4" => Ok(Self::Warn),
            "error" | "5" => Ok(Self::Error),
            "crash" | "panic" | "fatal" | "6" => Ok(Self::Crash),
            "unknown" => Ok(Self::Unknown),
            _ => Err(ParseLevelError),
        }
    }
}

/// The string given to [`Level::from_str`] is not a level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub struct ParseLevelError;

impl fmt::Display for ParseLevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unrecognised log level")
    }
}

impl std::error::Error for ParseLevelError {}

/// The error [`BugfixesLogger::error`] returns, so a logged error can be
/// propagated with `?`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub struct BugfixesError {
    message: String,
}

impl BugfixesError {
    /// Creates an error with the given message.
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

/// Why a record could not be sent to Bugfixes.
#[derive(Debug)]
#[must_use]
#[non_exhaustive]
pub enum ReportError {
    /// The agent key or secret is not configured.
    MissingCredentials,
    /// The request failed or the server rejected it.
    Http(reqwest::Error),
    /// The background runtime that sends reports could not start.
    RuntimeInit(io::Error),
    /// The background thread that sends reports panicked.
    ThreadJoin,
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCredentials => {
                f.write_str("missing BUGFIXES_AGENT_KEY or BUGFIXES_AGENT_SECRET")
            }
            Self::Http(err) => write!(f, "http error: {err}"),
            Self::RuntimeInit(err) => write!(f, "runtime init error: {err}"),
            Self::ThreadJoin => f.write_str("report thread panicked"),
        }
    }
}

impl std::error::Error for ReportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(err) => Some(err),
            Self::RuntimeInit(err) => Some(err),
            Self::MissingCredentials | Self::ThreadJoin => None,
        }
    }
}

/// A log line as printed and sent to Bugfixes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct LogRecord {
    /// The message.
    pub log: String,
    /// Its severity.
    pub level: Level,
    /// Source file of the call.
    pub file: String,
    /// Source line of the call, as text; the API expects a string here.
    pub line: String,
    /// Source line of the call.
    pub line_number: u32,
    /// The record in logfmt form.
    pub log_fmt: String,
    /// Stack trace, captured for levels that [capture one](Level::captures_stack)
    /// when remote reporting is on.
    pub stack: Option<String>,
}

/// A bug sent with [`BugfixesLogger::report_bug`]. Panics are reported this way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BugReport {
    /// Human-readable summary with a formatted stack trace.
    pub bug: String,
    /// The raw stack trace.
    pub raw: String,
    /// `file:line:column` where the bug happened.
    pub bug_line: String,
    /// Source file where the bug happened.
    pub file: String,
    /// Source line where the bug happened, as text; the API expects a string here.
    pub line: String,
    /// Source line where the bug happened.
    pub line_number: u32,
    /// Its severity.
    pub level: Level,
}

#[derive(Serialize)]
struct WirePayload<'a, T> {
    #[serde(flatten)]
    payload: &'a T,
    runtime: &'static str,
    #[serde(skip_serializing_if = "wire_value_is_empty")]
    commit_sha: &'a str,
    #[serde(skip_serializing_if = "wire_value_is_empty")]
    release: &'a str,
    #[serde(skip_serializing_if = "wire_value_is_empty")]
    environment: &'a str,
    #[serde(skip_serializing_if = "wire_value_is_empty")]
    service: &'a str,
}

fn wire_value_is_empty(value: &&str) -> bool {
    value.is_empty()
}

static GLOBAL_LOGGER: OnceLock<BugfixesLogger> = OnceLock::new();
static LOCAL_LOGGER: OnceLock<BugfixesLogger> = OnceLock::new();

/// Prints log records locally and sends them to Bugfixes.
///
/// Sending happens on a background thread and never blocks the caller, except
/// for [`fatal`](Self::fatal) and panics, which wait for the send to finish.
/// Without an agent key and secret, records are only printed.
#[derive(Clone)]
pub struct BugfixesLogger {
    config: Config,
    client: Client,
}

impl BugfixesLogger {
    /// Creates a logger. Fails only if the HTTP client cannot be built.
    pub fn new(config: Config) -> Result<Self, reqwest::Error> {
        let client = Client::builder().timeout(config.timeout).build()?;
        Ok(Self { config, client })
    }

    /// Creates a logger from [`Config::from_env`].
    pub fn from_env() -> Result<Self, reqwest::Error> {
        Self::new(Config::from_env())
    }

    /// Creates a logger from the environment that only prints and never sends.
    pub fn new_local() -> Result<Self, reqwest::Error> {
        let mut config = Config::from_env();
        config.local_only = true;
        Self::new(config)
    }

    /// Makes this the logger used by the crate's macros. If one is already set,
    /// it stays and this logger is handed back as the error.
    pub fn init_global(self) -> Result<&'static Self, Box<Self>> {
        GLOBAL_LOGGER.set(self).map_err(Box::new)?;
        Ok(global_logger())
    }

    /// The global logger, if one has been set or created.
    pub fn global() -> Option<&'static Self> {
        GLOBAL_LOGGER.get()
    }

    /// Logs at [`Level::Info`]. Returns the printed line, such as `"Info: message"`.
    #[track_caller]
    pub fn info(&self, message: impl Into<String>) -> Result<String, ReportError> {
        self.emit(Level::Info, message.into())
    }

    /// Logs at [`Level::Debug`]. Returns the printed line, such as `"Debug: message"`.
    #[track_caller]
    pub fn debug(&self, message: impl Into<String>) -> Result<String, ReportError> {
        self.emit(Level::Debug, message.into())
    }

    /// Logs at [`Level::Log`]. Returns the printed line, such as `"Log: message"`.
    #[track_caller]
    pub fn log(&self, message: impl Into<String>) -> Result<String, ReportError> {
        self.emit(Level::Log, message.into())
    }

    /// Logs at [`Level::Warn`]. Returns the printed line, such as `"Warn: message"`.
    #[track_caller]
    pub fn warn(&self, message: impl Into<String>) -> Result<String, ReportError> {
        self.emit(Level::Warn, message.into())
    }

    /// Logs at [`Level::Error`] and returns the message as a [`BugfixesError`].
    #[track_caller]
    pub fn error(&self, message: impl Into<String>) -> Result<BugfixesError, ReportError> {
        let message = message.into();
        self.emit(Level::Error, message.clone())?;
        Ok(BugfixesError::new(message))
    }

    /// Logs at [`Level::Fatal`], waits for it to be sent, then panics.
    ///
    /// # Panics
    ///
    /// Always, with `message`.
    #[track_caller]
    pub fn fatal(&self, message: impl Into<String>) -> ! {
        let message = message.into();
        let _ = self.emit(Level::Fatal, message.clone());
        panic!("{message}");
    }

    /// Sends a bug report in the background. Does nothing when local-only.
    pub fn report_bug(&self, bug: BugReport) -> Result<(), ReportError> {
        if self.config.local_only {
            return Ok(());
        }

        self.send_bug(&bug)
    }

    /// Prints and sends a panic caught with [`std::panic::catch_unwind`],
    /// waiting for the send to finish.
    pub fn report_panic_payload(
        &self,
        payload: &(dyn std::any::Any + Send),
    ) -> Result<(), ReportError> {
        let message = panic_payload_message(payload);
        let stack = capture_stack();
        print_panic(&message, &stack);

        if self.config.local_only {
            return Ok(());
        }

        let bug = build_bug_report(Level::Crash, &message, &stack);
        self.send_bug_blocking(&bug)
    }

    /// Installs a panic hook that prints and reports every panic, then runs the
    /// previous hook.
    pub fn install_panic_hook(&self) {
        let logger = self.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            logger.report_panic_hook(info);
            previous(info);
        }));
    }

    /// Prints a record at `level` without sending it, and returns it.
    #[track_caller]
    pub fn record(&self, level: Level, message: impl Into<String>) -> LogRecord {
        let location = Location::caller();
        let message = message.into();
        let stack = (!self.config.local_only && level.captures_stack()).then(capture_stack);
        let record = LogRecord {
            log_fmt: render_logfmt(level, location.file(), location.line(), &message),
            log: message,
            level,
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
            if level == Level::Fatal {
                self.send_blocking(&record)?;
            } else {
                self.send(&record)?;
            }
        }
        Ok(format!("{level}: {}", record.log))
    }

    fn report_panic_hook(&self, info: &PanicHookInfo<'_>) {
        let message = panic_hook_message(info);
        let stack = capture_stack();
        print_panic(&message, &stack);
        if self.config.local_only {
            return;
        }

        let bug = build_bug_report(Level::Crash, &message, &stack);
        let _ = self.send_bug_blocking(&bug);
    }

    fn should_report(&self, level: Level) -> bool {
        if self.config.local_only {
            return false;
        }

        match self.config.log_level {
            None | Some(Level::Unknown) => true,
            Some(minimum) => level.numeric() >= minimum.numeric(),
        }
    }

    fn send(&self, record: &LogRecord) -> Result<(), ReportError> {
        if !self.has_credentials() {
            return Ok(());
        }

        let logger = self.clone();
        let record = record.clone();
        let _ = spawn_report_thread(async move {
            if let Err(err) = logger.send_async(record).await {
                eprintln!("bugfixes sendLog: {err}");
            }
            Ok(())
        });

        Ok(())
    }

    fn send_blocking(&self, record: &LogRecord) -> Result<(), ReportError> {
        if !self.has_credentials() {
            return Ok(());
        }

        let logger = self.clone();
        let record = record.clone();
        join_report_thread(spawn_report_thread(async move {
            logger.send_async(record).await
        }))
    }

    fn send_bug(&self, bug: &BugReport) -> Result<(), ReportError> {
        if !self.has_credentials() {
            return Ok(());
        }

        let logger = self.clone();
        let bug = bug.clone();
        let _ = spawn_report_thread(async move {
            if let Err(err) = logger.send_bug_async(bug).await {
                eprintln!("bugfixes sendBug: {err}");
            }
            Ok(())
        });

        Ok(())
    }

    fn send_bug_blocking(&self, bug: &BugReport) -> Result<(), ReportError> {
        if !self.has_credentials() {
            return Ok(());
        }

        let logger = self.clone();
        let bug = bug.clone();
        join_report_thread(spawn_report_thread(async move {
            logger.send_bug_async(bug).await
        }))
    }

    async fn send_async(&self, record: LogRecord) -> Result<(), ReportError> {
        self.ensure_credentials()?;

        self.client
            .post(self.config.log_endpoint())
            .header("Content-Type", "application/json")
            .header("X-API-KEY", &self.config.agent_key)
            .header("X-API-SECRET", &self.config.agent_secret)
            .json(&WirePayload {
                payload: &record,
                runtime: "rust",
                commit_sha: &self.config.commit_sha,
                release: &self.config.release,
                environment: &self.config.environment,
                service: &self.config.service,
            })
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map(|_| ())
            .map_err(ReportError::Http)
    }

    async fn send_bug_async(&self, bug: BugReport) -> Result<(), ReportError> {
        self.ensure_credentials()?;

        self.client
            .post(self.config.bug_endpoint())
            .header("Content-Type", "application/json")
            .header("X-API-KEY", &self.config.agent_key)
            .header("X-API-SECRET", &self.config.agent_secret)
            .json(&WirePayload {
                payload: &bug,
                runtime: "rust",
                commit_sha: &self.config.commit_sha,
                release: &self.config.release,
                environment: &self.config.environment,
                service: &self.config.service,
            })
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map(|_| ())
            .map_err(ReportError::Http)
    }

    fn ensure_credentials(&self) -> Result<(), ReportError> {
        if !self.has_credentials() {
            return Err(ReportError::MissingCredentials);
        }

        Ok(())
    }

    fn has_credentials(&self) -> bool {
        !self.config.agent_key.is_empty() && !self.config.agent_secret.is_empty()
    }
}

/// Makes `logger` the global logger. See [`BugfixesLogger::init_global`].
pub fn init_global(logger: BugfixesLogger) -> Result<&'static BugfixesLogger, Box<BugfixesLogger>> {
    logger.init_global()
}

/// Sets the global logger from the environment, keeping one that is already set.
pub fn init_global_from_env() -> Result<&'static BugfixesLogger, reqwest::Error> {
    let logger = BugfixesLogger::from_env()?;
    Ok(init_global(logger).unwrap_or_else(|_| global_logger()))
}

/// Sets a local-only global logger, keeping one that is already set.
pub fn init_global_local() -> Result<&'static BugfixesLogger, reqwest::Error> {
    let logger = BugfixesLogger::new_local()?;
    Ok(init_global(logger).unwrap_or_else(|_| global_logger()))
}

/// The global logger, created from the environment on first use.
///
/// # Panics
///
/// If no logger is set and the HTTP client cannot be built.
pub fn global_logger() -> &'static BugfixesLogger {
    GLOBAL_LOGGER.get_or_init(|| {
        BugfixesLogger::from_env()
            .expect("failed to initialize global Bugfixes logger from environment")
    })
}

/// A shared local-only logger, created from the environment on first use.
///
/// # Panics
///
/// If the HTTP client cannot be built.
pub fn local_logger() -> &'static BugfixesLogger {
    LOCAL_LOGGER.get_or_init(|| {
        BugfixesLogger::new_local().expect("failed to initialize local Bugfixes logger")
    })
}

/// Installs [`BugfixesLogger::install_panic_hook`] for the global logger.
///
/// # Panics
///
/// See [`global_logger`].
pub fn install_global_panic_hook() {
    global_logger().install_panic_hook();
}

fn spawn_report_thread(
    task: impl Future<Output = Result<(), ReportError>> + Send + 'static,
) -> JoinHandle<Result<(), ReportError>> {
    thread::spawn(move || {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(ReportError::RuntimeInit)?;
        runtime.block_on(task)
    })
}

fn join_report_thread(handle: JoinHandle<Result<(), ReportError>>) -> Result<(), ReportError> {
    handle.join().map_err(|_| ReportError::ThreadJoin)?
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
    let use_color = io::stderr().is_terminal();
    let output = render_record(record, use_color);

    if level_uses_stdout(record.level) {
        let mut stdout = io::stdout();
        let _ = write_output(&mut stdout, &output);
    } else {
        let mut stderr = io::stderr();
        let _ = write_output(&mut stderr, &output);
    }
}

fn write_output(mut writer: impl Write, output: &str) -> io::Result<()> {
    writer.write_all(output.as_bytes())?;
    writer.flush()
}

fn print_panic(message: &str, stack: &str) {
    let use_color = io::stderr().is_terminal();
    eprintln!();
    eprintln!(
        "{} {}",
        colorize("panic:", ANSI_BRIGHT_CYAN, use_color),
        colorize(message, ANSI_BRIGHT_RED, use_color)
    );
    eprintln!();
    eprint!("{}", render_pretty_stack(stack, use_color));
}

fn render_record(record: &LogRecord, use_color: bool) -> String {
    render_record_with_timestamp(record, use_color, &current_timestamp())
}

fn render_record_with_timestamp(record: &LogRecord, use_color: bool, timestamp: &str) -> String {
    let mut output = format!(
        "{}: {} >> {}:{} >> {}\n",
        color_level_label(record.level, use_color),
        timestamp,
        record.file,
        record.line_number,
        record.log
    );

    if let Some(stack) = &record.stack {
        output.push_str(&colorize("Stack:", ANSI_BRIGHT_MAGENTA, use_color));
        output.push('\n');
        output.push_str(&render_pretty_stack(stack, use_color));
    }

    output
}

fn current_timestamp() -> String {
    let now = OffsetDateTime::now_utc();
    let now = UtcOffset::current_local_offset()
        .map(|offset| now.to_offset(offset))
        .unwrap_or(now);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

fn level_uses_stdout(level: Level) -> bool {
    matches!(level, Level::Debug | Level::Log | Level::Info)
}

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_BRIGHT_RED: &str = "\x1b[31;1m";
const ANSI_BRIGHT_GREEN: &str = "\x1b[32;1m";
const ANSI_BRIGHT_YELLOW: &str = "\x1b[33;1m";
const ANSI_BRIGHT_MAGENTA: &str = "\x1b[35;1m";
const ANSI_BRIGHT_CYAN: &str = "\x1b[36;1m";
const ANSI_BRIGHT_WHITE: &str = "\x1b[37;1m";

fn colorize(input: &str, ansi: &str, use_color: bool) -> String {
    if use_color {
        format!("{ansi}{input}{ANSI_RESET}")
    } else {
        input.to_string()
    }
}

fn color_level_label(level: Level, use_color: bool) -> String {
    let ansi = match level {
        Level::Warn => ANSI_BRIGHT_YELLOW,
        Level::Info => ANSI_BRIGHT_CYAN,
        Level::Log => ANSI_BRIGHT_GREEN,
        Level::Debug => ANSI_BRIGHT_MAGENTA,
        Level::Error | Level::Crash | Level::Fatal => ANSI_BRIGHT_RED,
        Level::Unknown => ANSI_BRIGHT_WHITE,
    };
    colorize(&level.to_string(), ansi, use_color)
}

fn render_pretty_stack(stack: &str, use_color: bool) -> String {
    let mut out = String::new();
    let mut line_index = 0usize;

    for raw_line in stack.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(function) = parse_backtrace_function_line(trimmed) {
            out.push_str(&decorate_func_call_line(function, use_color, line_index));
            line_index += 1;
            continue;
        }

        if let Some(source) = parse_backtrace_source_line(trimmed) {
            out.push_str(&decorate_source_line(source, use_color, line_index));
            line_index += 1;
            continue;
        }

        out.push_str("    ");
        out.push_str(trimmed);
        out.push('\n');
        line_index += 1;
    }

    out
}

fn build_bug_report(level: Level, message: &str, stack: &str) -> BugReport {
    let source = first_source_line(stack).unwrap_or_default();
    let (file, line, line_number) = parse_bug_line(&source);

    BugReport {
        bug: format!("panic: {message}\n\n{}", render_pretty_stack(stack, false)),
        raw: stack.to_string(),
        bug_line: source,
        file,
        line,
        line_number,
        level,
    }
}

fn first_source_line(stack: &str) -> Option<String> {
    stack.lines().map(str::trim).find_map(|line| {
        parse_backtrace_source_line(line).map(|source| {
            source
                .split_once(" + ")
                .map(|(path, _)| path)
                .unwrap_or(source)
                .to_string()
        })
    })
}

fn parse_bug_line(source: &str) -> (String, String, u32) {
    let mut parts = source.rsplitn(3, ':');
    let column = parts.next();
    let line = parts.next();
    let file = parts.next();

    match (file, line, column) {
        (Some(file), Some(line), Some(_column)) => {
            let line_number = line.parse::<u32>().unwrap_or_default();
            (file.to_string(), line.to_string(), line_number)
        }
        _ => (String::new(), String::new(), 0),
    }
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "Box<dyn Any>".to_string()
    }
}

fn panic_hook_message(info: &PanicHookInfo<'_>) -> String {
    if let Some(message) = info.payload().downcast_ref::<&'static str>() {
        (*message).to_string()
    } else if let Some(message) = info.payload().downcast_ref::<String>() {
        message.clone()
    } else if let Some(location) = info.location() {
        format!("panic at {}:{}", location.file(), location.line())
    } else {
        "panic".to_string()
    }
}

fn parse_backtrace_function_line(line: &str) -> Option<&str> {
    let (index, function) = line.split_once(':')?;
    if index.trim().parse::<usize>().is_ok() {
        Some(function.trim())
    } else {
        None
    }
}

fn parse_backtrace_source_line(line: &str) -> Option<&str> {
    line.strip_prefix("at ")
}

fn decorate_func_call_line(line: &str, use_color: bool, num: usize) -> String {
    let (pkg, method) = split_function_path(line);
    let mut out = String::new();

    if num == 0 {
        out.push_str(&colorize(" -> ", ANSI_BRIGHT_RED, use_color));
        out.push_str(&colorize(pkg, ANSI_BRIGHT_MAGENTA, use_color));
        out.push_str(&colorize(method, ANSI_BRIGHT_RED, use_color));
    } else {
        out.push_str("    ");
        out.push_str(&colorize(pkg, ANSI_YELLOW, use_color));
        out.push_str(&colorize(method, ANSI_BRIGHT_GREEN, use_color));
    }
    out.push('\n');
    out
}

fn decorate_source_line(line: &str, use_color: bool, num: usize) -> String {
    let mut out = String::new();
    let (dir, file, line_no) = split_source_path(line);

    if num == 1 {
        out.push_str(&colorize(" ->   ", ANSI_BRIGHT_RED, use_color));
        out.push_str(&colorize(&dir, ANSI_BRIGHT_WHITE, use_color));
        out.push_str(&colorize(&file, ANSI_BRIGHT_RED, use_color));
        out.push_str(&colorize(&line_no, ANSI_BRIGHT_MAGENTA, use_color));
    } else {
        out.push_str("      ");
        out.push_str(&colorize(&dir, ANSI_BRIGHT_WHITE, use_color));
        out.push_str(&colorize(&file, ANSI_BRIGHT_CYAN, use_color));
        out.push_str(&colorize(&line_no, ANSI_BRIGHT_GREEN, use_color));
    }
    out.push('\n');
    out
}

fn split_function_path(function: &str) -> (&str, &str) {
    if let Some(idx) = function.rfind("::") {
        (&function[..idx + 2], &function[idx + 2..])
    } else {
        ("", function)
    }
}

fn split_source_path(source: &str) -> (String, String, String) {
    let (path, suffix) = source
        .rsplit_once(':')
        .and_then(|(head, col)| col.parse::<u32>().ok().map(|_| (head, col)))
        .and_then(|(head, col)| head.rsplit_once(':').map(|(path, line)| (path, line, col)))
        .map(|(path, line, col)| (path, format!(":{line}:{col}")))
        .unwrap_or_else(|| (source, String::new()));

    if let Some(idx) = path.rfind('/') {
        (
            path[..idx + 1].to_string(),
            path[idx + 1..].to_string(),
            suffix,
        )
    } else {
        (String::new(), path.to_string(), suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ANSI_BRIGHT_CYAN, BugReport, BugfixesLogger, Level, LogRecord, ParseLevelError,
        ReportError, WirePayload, build_bug_report, color_level_label, colorize, current_timestamp,
        first_source_line, global_logger, init_global, level_uses_stdout, local_logger,
        panic_payload_message, parse_backtrace_function_line, parse_backtrace_source_line,
        parse_bug_line, quote_logfmt, render_logfmt, render_pretty_stack,
        render_record_with_timestamp, split_function_path, split_source_path, write_output,
    };
    use crate::Config;
    use std::cell::Cell;
    use std::error::Error;
    use std::io::{self, Write};

    #[test]
    fn level_parsing_matches_go_mapping() {
        let parse = |value: &str| value.parse::<Level>();
        assert_eq!(parse("debug"), Ok(Level::Debug));
        assert_eq!(parse("log"), Ok(Level::Log));
        assert_eq!(parse(" INFO "), Ok(Level::Info));
        assert_eq!(parse("warn"), Ok(Level::Warn));
        assert_eq!(parse("error"), Ok(Level::Error));
        assert_eq!(parse("crash"), Ok(Level::Crash));
        assert_eq!(parse("fatal"), Ok(Level::Crash));
        assert_eq!(parse("4"), Ok(Level::Warn));
        assert_eq!(parse("10"), Err(ParseLevelError));
        assert_eq!(parse("nonsense"), Err(ParseLevelError));
    }

    #[test]
    fn level_displays_capitalised_and_serializes_lowercase() {
        assert_eq!(Level::Warn.to_string(), "Warn");
        assert_eq!(
            serde_json::to_value(Level::Crash).expect("serialize"),
            "crash"
        );
    }

    #[test]
    fn log_level_filters_lower_levels() {
        let logger = |log_level| {
            BugfixesLogger::new(Config {
                log_level,
                ..Config::default()
            })
            .expect("logger")
        };
        assert!(logger(None).should_report(Level::Debug));
        assert!(logger(Some(Level::Unknown)).should_report(Level::Debug));
        assert!(!logger(Some(Level::Warn)).should_report(Level::Info));
        assert!(logger(Some(Level::Warn)).should_report(Level::Error));
    }

    #[test]
    fn runtime_init_error_keeps_its_source() {
        let err = ReportError::RuntimeInit(io::Error::other("no threads"));
        assert_eq!(err.source().expect("source").to_string(), "no threads");
    }

    #[test]
    fn wire_payload_carries_explicit_deployment_metadata() {
        let record = LogRecord {
            log: "boom".to_string(),
            level: Level::Error,
            file: "src/main.rs".to_string(),
            line: "42".to_string(),
            line_number: 42,
            log_fmt: String::new(),
            stack: None,
        };
        let value = serde_json::to_value(WirePayload {
            payload: &record,
            runtime: "rust",
            commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            release: "worker@1.2.3",
            environment: "development",
            service: "worker",
        })
        .expect("serialize wire payload");
        assert_eq!(
            value["commit_sha"],
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(value["release"], "worker@1.2.3");
        assert_eq!(value["environment"], "development");
        assert_eq!(value["service"], "worker");
        assert_eq!(value["runtime"], "rust");
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
            agent_key: String::new(),
            local_only: false,
            log_level: Some(Level::Info),
            ..Config::default()
        })
        .expect("logger");

        let response = logger.info("hello").expect("best effort");
        assert_eq!(response, "Info: hello");
    }

    #[test]
    fn local_record_skips_stack_capture() {
        let logger = BugfixesLogger::new(Config {
            local_only: true,
            ..Config::default()
        })
        .expect("logger");

        let record = logger.record(Level::Warn, "careful");
        assert_eq!(record.level, Level::Warn);
        assert_eq!(record.log, "careful");
        assert!(record.file.ends_with("src/logger.rs"));
        assert!(record.line_number > 0);
        assert!(record.stack.is_none());
    }

    #[test]
    fn remote_record_captures_stack() {
        let logger = BugfixesLogger::new(Config {
            local_only: false,
            ..Config::default()
        })
        .expect("logger");

        let record = logger.record(Level::Warn, "careful");
        assert!(record.stack.as_ref().is_some_and(|stack| !stack.is_empty()));
    }

    #[test]
    fn pretty_stack_renders_function_and_source_lines() {
        let stack = "\
   0: app::worker::run\n\
             at /workspace/src/worker.rs:42:7\n\
   1: std::rt::lang_start::{{closure}}\n\
             at /rustc/library/std/src/rt.rs:171:5\n";

        let rendered = render_pretty_stack(stack, false);
        assert!(rendered.contains(" -> app::worker::"));
        assert!(rendered.contains("run"));
        assert!(rendered.contains("/workspace/src/worker.rs"));
        assert!(rendered.contains(":42:7"));
    }

    #[test]
    fn pretty_stack_with_color_includes_ansi_sequences() {
        let stack = "   0: app::worker::run\n             at /workspace/src/worker.rs:42:7\n";
        let rendered = render_pretty_stack(stack, true);
        assert!(rendered.contains("\x1b["));
    }

    #[test]
    fn bug_report_extracts_first_source_location() {
        let stack = "\
   0: app::worker::run\n\
             at /workspace/src/worker.rs:42:7\n\
   1: std::rt::lang_start::{{closure}}\n";
        let bug = build_bug_report(Level::Crash, "boom", stack);
        assert_eq!(
            bug,
            BugReport {
                bug: "panic: boom\n\n -> app::worker::run\n ->   /workspace/src/worker.rs:42:7\n    std::rt::lang_start::{{closure}}\n"
                    .to_string(),
                raw: stack.to_string(),
                bug_line: "/workspace/src/worker.rs:42:7".to_string(),
                file: "/workspace/src/worker.rs".to_string(),
                line: "42".to_string(),
                line_number: 42,
                level: Level::Crash,
            }
        );
    }

    #[test]
    fn bug_location_helpers_handle_missing_source_lines() {
        assert_eq!(first_source_line("   0: app::main"), None);
        assert_eq!(parse_bug_line(""), (String::new(), String::new(), 0));
    }

    #[test]
    fn panic_payload_message_supports_strings() {
        let owned = String::from("owned panic");
        assert_eq!(panic_payload_message(&owned), "owned panic");
        assert_eq!(panic_payload_message(&"static panic"), "static panic");
    }

    #[test]
    fn backtrace_line_parsers_match_rust_format() {
        assert_eq!(
            parse_backtrace_function_line("   12: app::main"),
            Some("app::main")
        );
        assert_eq!(
            parse_backtrace_source_line("at /workspace/src/main.rs:10:5"),
            Some("/workspace/src/main.rs:10:5")
        );
        assert_eq!(parse_backtrace_function_line("app::main"), None);
    }

    #[test]
    fn path_splitters_keep_package_and_line_suffixes() {
        assert_eq!(
            split_function_path("app::worker::run"),
            ("app::worker::", "run")
        );
        assert_eq!(
            split_source_path("/workspace/src/main.rs:10:5"),
            (
                "/workspace/src/".to_string(),
                "main.rs".to_string(),
                ":10:5".to_string()
            )
        );
    }

    #[test]
    fn logfmt_rendering_quotes_when_needed() {
        assert_eq!(quote_logfmt("simple/path.rs"), "simple/path.rs");
        assert_eq!(quote_logfmt("hello world"), "\"hello world\"");
        assert!(
            render_logfmt(Level::Info, "src/main.rs", 42, "hello world")
                .contains("msg=\"hello world\"")
        );
    }

    #[test]
    fn color_helpers_fall_back_without_tty() {
        assert_eq!(colorize("Info", ANSI_BRIGHT_CYAN, false), "Info");
        assert_eq!(color_level_label(Level::Info, false), "Info");
    }

    #[test]
    fn info_levels_use_stdout() {
        assert!(level_uses_stdout(Level::Debug));
        assert!(level_uses_stdout(Level::Log));
        assert!(level_uses_stdout(Level::Info));
        assert!(!level_uses_stdout(Level::Warn));
        assert!(!level_uses_stdout(Level::Error));
    }

    #[test]
    fn write_output_flushes_after_write() {
        struct RecordingWriter {
            flushed: Cell<bool>,
            bytes: Vec<u8>,
        }

        impl Write for RecordingWriter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.bytes.extend_from_slice(buf);
                Ok(buf.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                self.flushed.set(true);
                Ok(())
            }
        }

        let mut writer = RecordingWriter {
            flushed: Cell::new(false),
            bytes: Vec::new(),
        };

        write_output(&mut writer, "hello world").expect("write");

        assert_eq!(writer.bytes, b"hello world");
        assert!(writer.flushed.get());
    }

    #[test]
    fn rendered_records_include_timestamp_before_source_location() {
        let rendered = render_record_with_timestamp(
            &LogRecord {
                log: "server started".to_string(),
                level: Level::Info,
                file: "src/main.rs".to_string(),
                line: "42".to_string(),
                line_number: 42,
                log_fmt: String::new(),
                stack: None,
            },
            false,
            "2026-03-23 15:04:05",
        );

        assert_eq!(
            rendered,
            "Info: 2026-03-23 15:04:05 >> src/main.rs:42 >> server started\n"
        );
    }

    #[test]
    fn timestamps_use_go_compatible_local_format() {
        let timestamp = current_timestamp();
        let parts: Vec<_> = timestamp.split(' ').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].len(), 10);
        assert_eq!(parts[1].len(), 8);
        assert_eq!(parts[0].chars().filter(|&ch| ch == '-').count(), 2);
        assert_eq!(parts[1].chars().filter(|&ch| ch == ':').count(), 2);
        assert!(
            timestamp
                .chars()
                .all(|ch| ch.is_ascii_digit() || ch == '-' || ch == ':' || ch == ' ')
        );
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

    #[test]
    fn local_logger_always_skips_remote_reporting() {
        let response = local_logger().info("local only").expect("info");
        assert_eq!(response, "Info: local only");
    }

    #[test]
    fn local_logger_is_safe_inside_tokio_runtime() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let logger = BugfixesLogger::new_local().expect("logger");
            let response = logger.info("tokio safe").expect("info");
            assert_eq!(response, "Info: tokio safe");
        });
    }
}
