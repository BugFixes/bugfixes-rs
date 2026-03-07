use std::env;
use std::time::Duration;

pub const DEFAULT_SERVER: &str = "https://api.bugfix.es/v1";
pub const DEFAULT_TIMEOUT_SECS: u64 = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub server: String,
    pub agent_key: String,
    pub agent_secret: String,
    pub log_level: String,
    pub local_only: bool,
    pub timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Config {
    pub fn from_env() -> Self {
        let local_only = env::var("BUGFIXES_LOCAL_ONLY")
            .ok()
            .and_then(|value| value.parse::<bool>().ok())
            .unwrap_or(false);

        Self {
            server: env::var("BUGFIXES_SERVER").unwrap_or_else(|_| DEFAULT_SERVER.to_string()),
            agent_key: env::var("BUGFIXES_AGENT_KEY").unwrap_or_default(),
            agent_secret: env::var("BUGFIXES_AGENT_SECRET").unwrap_or_default(),
            log_level: env::var("BUGFIXES_LOG_LEVEL").unwrap_or_default(),
            local_only,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }

    pub fn log_endpoint(&self) -> String {
        format!("{}/log", self.server.trim_end_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, DEFAULT_SERVER};

    #[test]
    fn config_uses_defaults() {
        temp_env::with_vars_unset(
            [
                "BUGFIXES_SERVER",
                "BUGFIXES_AGENT_KEY",
                "BUGFIXES_AGENT_SECRET",
                "BUGFIXES_LOG_LEVEL",
                "BUGFIXES_LOCAL_ONLY",
            ],
            || {
                let cfg = Config::from_env();
                assert_eq!(cfg.server, DEFAULT_SERVER);
                assert!(cfg.agent_key.is_empty());
                assert!(cfg.agent_secret.is_empty());
                assert!(cfg.log_level.is_empty());
                assert!(!cfg.local_only);
            },
        );
    }

    #[test]
    fn config_reads_env() {
        temp_env::with_vars(
            [
                ("BUGFIXES_SERVER", Some("https://example.test/v1")),
                ("BUGFIXES_AGENT_KEY", Some("abc")),
                ("BUGFIXES_AGENT_SECRET", Some("def")),
                ("BUGFIXES_LOG_LEVEL", Some("warn")),
                ("BUGFIXES_LOCAL_ONLY", Some("true")),
            ],
            || {
                let cfg = Config::from_env();
                assert_eq!(cfg.server, "https://example.test/v1");
                assert_eq!(cfg.agent_key, "abc");
                assert_eq!(cfg.agent_secret, "def");
                assert_eq!(cfg.log_level, "warn");
                assert!(cfg.local_only);
                assert_eq!(cfg.log_endpoint(), "https://example.test/v1/log");
            },
        );
    }
}
