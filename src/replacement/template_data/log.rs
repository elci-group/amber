//! amber_log — Minimal replacement for the `log` crate
//!
//! Provides basic logging macros without the crate dependency.

/// Log level enum
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

/// Static maximum log level (compile-time configurable)
pub static MAX_LEVEL: std::sync::atomic::AtomicU8 =
    std::sync::atomic::AtomicU8::new(Level::Debug as u8);

/// Set the maximum log level
pub fn set_max_level(level: Level) {
    MAX_LEVEL.store(level as u8, std::sync::atomic::Ordering::Relaxed);
}

/// Check if a level is enabled
#[inline]
pub fn enabled(level: Level) -> bool {
    level as u8 <= MAX_LEVEL.load(std::sync::atomic::Ordering::Relaxed)
}

/// Simple logger trait
pub trait Log {
    fn log(&self, level: Level, message: &str);
}

/// Default stderr logger
pub struct StderrLogger;

impl Log for StderrLogger {
    fn log(&self, level: Level, message: &str) {
        let level_str = match level {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        };
        eprintln!("[{}] {}", level_str, message);
    }
}

static LOGGER: std::sync::OnceLock<Box<dyn Log + Send + Sync>> = std::sync::OnceLock::new();

/// Set the global logger
pub fn set_logger<L: Log + Send + Sync + 'static>(logger: L) {
    let _ = LOGGER.set(Box::new(logger));
}

/// Internal log function
fn do_log(level: Level, args: std::fmt::Arguments) {
    if !enabled(level) {
        return;
    }
    let msg = format!("{}", args);
    if let Some(logger) = LOGGER.get() {
        logger.log(level, &msg);
    } else {
        StderrLogger.log(level, &msg);
    }
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => { $crate::do_log($crate::Level::Error, format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => { $crate::do_log($crate::Level::Warn, format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => { $crate::do_log($crate::Level::Info, format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => { $crate::do_log($crate::Level::Debug, format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => { $crate::do_log($crate::Level::Trace, format_args!($($arg)*)) };
}


