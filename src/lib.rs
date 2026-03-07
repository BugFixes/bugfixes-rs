mod config;
mod logger;

pub use config::{Config, DEFAULT_SERVER};
pub use logger::{
    BugfixesError, BugfixesLogger, Level, LogRecord, ReportError, global_logger, init_global,
    init_global_from_env, init_global_local,
};

#[doc(hidden)]
pub mod __private {
    pub use crate::global_logger;
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        $crate::__private::global_logger().debug(format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        $crate::__private::global_logger().log(format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        $crate::__private::global_logger().info(format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {{
        $crate::__private::global_logger().warn(format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {{
        $crate::__private::global_logger().error(format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! fatal {
    ($($arg:tt)*) => {{
        $crate::__private::global_logger().fatal(format!($($arg)*))
    }};
}
