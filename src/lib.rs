//! Logging client for [Bugfixes](https://bugfix.es).
//!
//! Records print locally with their file and line. When an agent key and
//! secret are configured, they are also sent to Bugfixes, along with panics.
//!
//! ```no_run
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Reads BUGFIXES_AGENT_KEY, BUGFIXES_AGENT_SECRET and the other
//!     // BUGFIXES_* variables. Without credentials, logs only print.
//!     bugfixes::init_global_from_env()?;
//!     bugfixes::install_global_panic_hook();
//!
//!     bugfixes::info!("server starting on {}", ":3000")?;
//!     Ok(())
//! }
//! ```
//!
//! Use [`BugfixesLogger`] directly to pass a logger around instead of using
//! the global one, and the [`local`] macros for output that is never sent.

#![warn(missing_docs)]

mod config;
mod logger;

pub use config::{Config, DEFAULT_SERVER};
pub use logger::{
    BugReport, BugfixesError, BugfixesLogger, Level, LogRecord, ParseLevelError, ReportError,
    global_logger, init_global, init_global_from_env, init_global_local, install_global_panic_hook,
    local_logger,
};

#[doc(hidden)]
pub mod __private {
    pub use crate::global_logger;
    pub use crate::local_logger;
}

/// Formats a message and logs it at [`Level::Debug`] with the global logger.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        $crate::__private::global_logger().debug(format!($($arg)*))
    }};
}

/// Formats a message and logs it at [`Level::Log`] with the global logger.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        $crate::__private::global_logger().log(format!($($arg)*))
    }};
}

/// Formats a message and logs it at [`Level::Info`] with the global logger.
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        $crate::__private::global_logger().info(format!($($arg)*))
    }};
}

/// Formats a message and logs it at [`Level::Warn`] with the global logger.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {{
        $crate::__private::global_logger().warn(format!($($arg)*))
    }};
}

/// Formats a message and logs it at [`Level::Error`] with the global logger. Returns a [`BugfixesError`] to propagate.
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {{
        $crate::__private::global_logger().error(format!($($arg)*))
    }};
}

/// Formats a message and logs it at [`Level::Fatal`] with the global logger. Panics after sending.
#[macro_export]
macro_rules! fatal {
    ($($arg:tt)*) => {{
        $crate::__private::global_logger().fatal(format!($($arg)*))
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __bugfixes_local_debug {
    ($($arg:tt)*) => {{
        $crate::__private::local_logger().debug(format!($($arg)*))
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __bugfixes_local_log {
    ($($arg:tt)*) => {{
        $crate::__private::local_logger().log(format!($($arg)*))
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __bugfixes_local_info {
    ($($arg:tt)*) => {{
        $crate::__private::local_logger().info(format!($($arg)*))
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __bugfixes_local_warn {
    ($($arg:tt)*) => {{
        $crate::__private::local_logger().warn(format!($($arg)*))
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __bugfixes_local_error {
    ($($arg:tt)*) => {{
        $crate::__private::local_logger().error(format!($($arg)*))
    }};
}

/// Macros that log with [`local_logger`], which only prints and never sends.
pub mod local {
    pub use crate::__bugfixes_local_debug as debug;
    pub use crate::__bugfixes_local_error as error;
    pub use crate::__bugfixes_local_info as info;
    pub use crate::__bugfixes_local_log as log;
    pub use crate::__bugfixes_local_warn as warn;
}
