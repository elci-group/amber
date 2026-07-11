//! amber_tracing — Minimal replacement for the `tracing` crate
//!
//! Provides drop-in macros for `info!`, `debug!`, `warn!`, `error!`, and `trace!`
//! plus a no-op `Span` type. This covers the most common tracing usage without
//! the dependency graph of the full crate.

use std::fmt;

/// A no-op span handle. Most code only calls `.entered()` and drops it.
#[derive(Debug, Clone)]
pub struct Span;

impl Span {
    /// Create a new no-op span.
    #[must_use]
    pub const fn current() -> Self {
        Self
    }

    /// Enter the span. Returns a guard that does nothing on drop.
    #[must_use]
    pub fn entered(&self) -> SpanGuard {
        SpanGuard
    }
}

/// Guard returned by `Span::entered`.
#[derive(Debug)]
pub struct SpanGuard;

impl Drop for SpanGuard {
    fn drop(&mut self) {}
}

/// Dummy event builder. Not a full implementation, but satisfies the type.
pub struct Event;

impl Event {
    /// Construct a dummy event.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for Event {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal logging helper, made public so the exported macros can resolve it.
pub fn log(level: &str, args: fmt::Arguments) {
    eprintln!("[{level}] {args}");
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => { $crate::log("INFO", format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => { $crate::log("DEBUG", format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => { $crate::log("WARN", format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => { $crate::log("ERROR", format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => { $crate::log("TRACE", format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! span {
    ($($arg:tt)*) => { $crate::Span::current() };
}

#[macro_export]
macro_rules! event {
    ($($arg:tt)*) => { $crate::Event::new() };
}
