use crate::logger::Level;
use std::env;
use std::time::Duration;

/// The Bugfixes API used when `BUGFIXES_SERVER` is not set.
pub const DEFAULT_SERVER: &str = "https://api.bugfix.es/v1";
/// How long a report request may take before it is abandoned.
pub const DEFAULT_TIMEOUT_SECS: u64 = 10;

/// Settings for a [`BugfixesLogger`](crate::BugfixesLogger).
///
/// [`Config::default`] gives the built-in defaults; [`Config::from_env`] reads
/// the `BUGFIXES_*` environment variables on top of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Base URL of the Bugfixes API (`BUGFIXES_SERVER`).
    pub server: String,
    /// Agent key sent as `X-API-KEY` (`BUGFIXES_AGENT_KEY`).
    pub agent_key: String,
    /// Agent secret sent as `X-API-SECRET` (`BUGFIXES_AGENT_SECRET`).
    pub agent_secret: String,
    /// Git commit the service was built from (`BUGFIXES_COMMIT_SHA`).
    pub commit_sha: String,
    /// Release name, such as `worker@1.2.3` (`BUGFIXES_RELEASE`).
    pub release: String,
    /// Deployment environment, such as `production` (`BUGFIXES_ENVIRONMENT`).
    pub environment: String,
    /// Service name (`BUGFIXES_SERVICE`).
    pub service: String,
    /// Lowest level sent to Bugfixes (`BUGFIXES_LOG_LEVEL`). `None` sends every level.
    pub log_level: Option<Level>,
    /// Print locally and never send anything (`BUGFIXES_LOCAL_ONLY`).
    pub local_only: bool,
    /// Timeout for each report request.
    pub timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: DEFAULT_SERVER.to_string(),
            agent_key: String::new(),
            agent_secret: String::new(),
            commit_sha: String::new(),
            release: String::new(),
            environment: String::new(),
            service: String::new(),
            log_level: None,
            local_only: false,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }
}

impl Config {
    /// Builds a config from the `BUGFIXES_*` environment variables, using the
    /// defaults for any that are unset or invalid.
    pub fn from_env() -> Self {
        let var = |name: &str| env::var(name).unwrap_or_default();
        let local_only: bool = env::var("BUGFIXES_LOCAL_ONLY")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(false);
        let log_level: Option<Level> = env::var("BUGFIXES_LOG_LEVEL")
            .ok()
            .and_then(|value| value.parse().ok());

        Self {
            server: env::var("BUGFIXES_SERVER").unwrap_or_else(|_| DEFAULT_SERVER.to_string()),
            agent_key: var("BUGFIXES_AGENT_KEY"),
            agent_secret: var("BUGFIXES_AGENT_SECRET"),
            commit_sha: var("BUGFIXES_COMMIT_SHA"),
            release: var("BUGFIXES_RELEASE"),
            environment: var("BUGFIXES_ENVIRONMENT"),
            service: var("BUGFIXES_SERVICE"),
            log_level,
            local_only,
            ..Self::default()
        }
    }

    /// The endpoint log records are posted to.
    pub fn log_endpoint(&self) -> String {
        format!("{}/log", self.server.trim_end_matches('/'))
    }

    /// The endpoint bug reports are posted to.
    pub fn bug_endpoint(&self) -> String {
        format!("{}/bug", self.server.trim_end_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, DEFAULT_SERVER};
    use crate::Level;

    #[test]
    fn config_uses_defaults() {
        temp_env::with_vars_unset(
            [
                "BUGFIXES_SERVER",
                "BUGFIXES_AGENT_KEY",
                "BUGFIXES_AGENT_SECRET",
                "BUGFIXES_LOG_LEVEL",
                "BUGFIXES_LOCAL_ONLY",
                "BUGFIXES_COMMIT_SHA",
                "BUGFIXES_RELEASE",
                "BUGFIXES_ENVIRONMENT",
                "BUGFIXES_SERVICE",
            ],
            || {
                let cfg = Config::from_env();
                assert_eq!(cfg.server, DEFAULT_SERVER);
                assert!(cfg.agent_key.is_empty());
                assert!(cfg.agent_secret.is_empty());
                assert!(cfg.commit_sha.is_empty());
                assert!(cfg.release.is_empty());
                assert!(cfg.environment.is_empty());
                assert!(cfg.service.is_empty());
                assert_eq!(cfg.log_level, None);
                assert!(!cfg.local_only);
            },
        );
    }

    #[test]
    fn default_ignores_env() {
        temp_env::with_var("BUGFIXES_AGENT_KEY", Some("abc"), || {
            assert!(Config::default().agent_key.is_empty());
        });
    }

    #[test]
    fn config_reads_env() {
        temp_env::with_vars(
            [
                ("BUGFIXES_SERVER", Some("https://example.test/v1")),
                ("BUGFIXES_AGENT_KEY", Some("abc")),
                ("BUGFIXES_AGENT_SECRET", Some("shh")),
                ("BUGFIXES_LOG_LEVEL", Some("warn")),
                ("BUGFIXES_LOCAL_ONLY", Some("true")),
                (
                    "BUGFIXES_COMMIT_SHA",
                    Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                ),
                ("BUGFIXES_RELEASE", Some("worker@1.2.3")),
                ("BUGFIXES_ENVIRONMENT", Some("development")),
                ("BUGFIXES_SERVICE", Some("worker")),
            ],
            || {
                let cfg = Config::from_env();
                assert_eq!(cfg.server, "https://example.test/v1");
                assert_eq!(cfg.agent_key, "abc");
                assert_eq!(cfg.agent_secret, "shh");
                assert_eq!(cfg.log_level, Some(Level::Warn));
                assert!(cfg.local_only);
                assert_eq!(cfg.commit_sha, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
                assert_eq!(cfg.release, "worker@1.2.3");
                assert_eq!(cfg.environment, "development");
                assert_eq!(cfg.service, "worker");
                assert_eq!(cfg.log_endpoint(), "https://example.test/v1/log");
                assert_eq!(cfg.bug_endpoint(), "https://example.test/v1/bug");
            },
        );
    }
}
