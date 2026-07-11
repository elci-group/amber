//! `amber_anyhow` — Minimal replacement for `anyhow`
//!
//! Provides basic error chaining without the proc-macro overhead.

use std::error::Error;
use std::fmt;

/// A generic error type with context
#[derive(Debug)]
pub struct AmberError {
    message: String,
    #[allow(dead_code)]
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl fmt::Display for AmberError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl<E: Error + Send + Sync + 'static> From<E> for AmberError {
    fn from(err: E) -> Self {
        Self {
            message: err.to_string(),
            source: Some(Box::new(err)),
        }
    }
}

/// Create an error from a string
pub fn anyhow(msg: impl Into<String>) -> AmberError {
    AmberError {
        message: msg.into(),
        source: None,
    }
}

// Re-export commonly used types
pub type Result<T> = std::result::Result<T, AmberError>;

/// Add context to a result.
pub trait Context<T> {
    /// Wrap the error value with a new contextual message.
    ///
    /// # Errors
    ///
    /// Returns `Err` with the wrapped error if the input was `Err`.
    fn context(self, msg: impl Into<String>) -> std::result::Result<T, AmberError>;

    /// Wrap the error value with a lazily evaluated contextual message.
    ///
    /// # Errors
    ///
    /// Returns `Err` with the wrapped error if the input was `Err`.
    fn with_context<F>(self, f: F) -> std::result::Result<T, AmberError>
    where
        F: FnOnce() -> String;
}

impl<T, E: Error + Send + Sync + 'static> Context<T> for std::result::Result<T, E> {
    fn context(self, msg: impl Into<String>) -> std::result::Result<T, AmberError> {
        self.map_err(|e| AmberError {
            message: msg.into(),
            source: Some(Box::new(e)),
        })
    }

    fn with_context<F>(self, f: F) -> std::result::Result<T, AmberError>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| AmberError {
            message: f(),
            source: Some(Box::new(e)),
        })
    }
}

impl<T> Context<T> for Option<T> {
    fn context(self, msg: impl Into<String>) -> std::result::Result<T, AmberError> {
        self.ok_or_else(|| AmberError {
            message: msg.into(),
            source: None,
        })
    }

    fn with_context<F>(self, f: F) -> std::result::Result<T, AmberError>
    where
        F: FnOnce() -> String,
    {
        self.ok_or_else(|| AmberError {
            message: f(),
            source: None,
        })
    }
}

/// Create an error using a format string.
#[macro_export]
macro_rules! anyhow {
    ($($arg:tt)*) => { $crate::amber_anyhow::anyhow(format!($($arg)*)) };
}

/// Macro for early returns with ?
#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => { return Err($crate::anyhow!($($arg)*)) };
}

/// Ensure a condition is true
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $($arg:tt)*) => {
        if !$cond { $crate::bail!($($arg)*) }
    };
}
