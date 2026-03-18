mod config;
mod logger;

pub use config::{Config, DEFAULT_SERVER};
pub use logger::{
    BugReport, BugfixesError, BugfixesLogger, Level, LogRecord, ReportError, global_logger,
    init_global, init_global_from_env, init_global_local, install_global_panic_hook, local_logger,
};

#[doc(hidden)]
pub mod __private {
    pub use crate::global_logger;
    pub use crate::local_logger;
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

#[macro_export]
macro_rules! __bugfixes_local_debug {
    ($($arg:tt)*) => {{
        $crate::__private::local_logger().debug(format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! __bugfixes_local_log {
    ($($arg:tt)*) => {{
        $crate::__private::local_logger().log(format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! __bugfixes_local_info {
    ($($arg:tt)*) => {{
        $crate::__private::local_logger().info(format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! __bugfixes_local_warn {
    ($($arg:tt)*) => {{
        $crate::__private::local_logger().warn(format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! __bugfixes_local_error {
    ($($arg:tt)*) => {{
        $crate::__private::local_logger().error(format!($($arg)*))
    }};
}

pub mod local {
    pub use crate::__bugfixes_local_debug as debug;
    pub use crate::__bugfixes_local_error as error;
    pub use crate::__bugfixes_local_info as info;
    pub use crate::__bugfixes_local_log as log;
    pub use crate::__bugfixes_local_warn as warn;
}
