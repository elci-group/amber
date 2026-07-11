//! amber_tracing_subscriber — Minimal, dependency-free subscriber shim.
//!
//! Mirrors the small slice of the `tracing-subscriber` API that most setup code
//! uses (`fmt()`, `with_max_level`, `with_env_filter`, `without_time`,
//! `try_init`, `EnvFilter`). It installs a simple stderr sink filtered by the
//! `RUST_LOG` directive. It does **not** implement the full `tracing::Subscriber`
//! trait (that would require the `tracing` crate as a dependency); pair it with
//! the `amber_tracing` macros for a fully dependency-free logging stack.

use std::env;
use std::sync::atomic::{AtomicU8, Ordering};

/// Log level, ordered from most to least severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl Level {
    #[must_use]
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "error" => Self::Error,
            "warn" | "warning" => Self::Warn,
            "info" => Self::Info,
            "debug" => Self::Debug,
            "trace" => Self::Trace,
            _ => Self::Info,
        }
    }

    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
        }
    }
}

static MAX_LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);

/// Set the global maximum log level.
pub fn set_max_level(level: Level) {
    MAX_LEVEL.store(level as u8, Ordering::Relaxed);
}

/// Return whether `level` is enabled under the current filter.
#[must_use]
pub fn enabled(level: Level) -> bool {
    (level as u8) <= MAX_LEVEL.load(Ordering::Relaxed)
}

/// Environment-driven filter.
#[derive(Debug, Clone)]
pub struct EnvFilter {
    directive: String,
}

impl EnvFilter {
    /// Build a filter from an explicit directive (e.g. `"info"` or `"debug"`).
    #[must_use]
    pub fn new(directive: &str) -> Self {
        Self {
            directive: directive.to_string(),
        }
    }

    /// Build a filter from the `RUST_LOG` environment variable.
    ///
    /// # Errors
    /// Returns `env::VarError` if `RUST_LOG` is unset or not valid Unicode.
    pub fn try_from_default_env() -> Result<Self, env::VarError> {
        env::var("RUST_LOG").map(|directive| Self { directive })
    }

    /// Resolve the maximum [`Level`] implied by this filter.
    #[must_use]
    pub fn max_level(&self) -> Level {
        // Directives may be `level` or `target=level,...`; take the first level
        // token found.
        let first = self.directive.split(',').next().unwrap_or("info");
        let level_token = first.rsplit('=').next().unwrap_or(first).trim();
        Level::from_str_lossy(level_token)
    }
}

/// Write a single log record to stderr if `level` is enabled.
pub fn log(level: Level, target: &str, message: &str) {
    if !enabled(level) {
        return;
    }
    if target.is_empty() {
        eprintln!("[{}] {}", level.as_str(), message);
    } else {
        eprintln!("[{}] {}: {}", level.as_str(), target, message);
    }
}

/// Builder-style subscriber, matching `tracing_subscriber::fmt()`.
#[derive(Debug, Clone)]
pub struct FmtSubscriber {
    max_level: Option<Level>,
    env_filter: Option<EnvFilter>,
    without_time: bool,
}

impl FmtSubscriber {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_level: None,
            env_filter: None,
            without_time: false,
        }
    }

    #[must_use]
    pub fn with_max_level<T>(mut self, level: T) -> Self
    where
        T: IntoLevel,
    {
        self.max_level = Some(level.into_level());
        self
    }

    #[must_use]
    pub fn with_env_filter(mut self, filter: EnvFilter) -> Self {
        self.env_filter = Some(filter);
        self
    }

    #[must_use]
    pub const fn without_time(mut self) -> Self {
        self.without_time = true;
        self
    }

    /// Install this subscriber as the global default.
    ///
    /// # Errors
    /// Currently infallible; returns `Result` for API compatibility.
    pub fn try_init(self) -> Result<(), Box<dyn std::error::Error>> {
        self.init();
        Ok(())
    }

    /// Install this subscriber as the global default.
    pub fn init(self) {
        let level = self
            .env_filter
            .as_ref()
            .map(EnvFilter::max_level)
            .or(self.max_level)
            .unwrap_or(Level::Info);
        let _ = self.without_time;
        set_max_level(level);
    }
}

impl Default for FmtSubscriber {
    fn default() -> Self {
        Self::new()
    }
}

/// Conversion trait so [`FmtSubscriber::with_max_level`] accepts common level
/// representations (our own [`Level`] or anything that maps to one).
pub trait IntoLevel {
    fn into_level(self) -> Level;
}

impl IntoLevel for Level {
    fn into_level(self) -> Level {
        self
    }
}

impl IntoLevel for &str {
    fn into_level(self) -> Level {
        Level::from_str_lossy(self)
    }
}

/// Convenience builder matching `tracing_subscriber::fmt()`.
#[must_use]
pub const fn fmt() -> FmtSubscriber {
    FmtSubscriber::new()
}

/// Placeholder top-level initializer matching `tracing_subscriber::init()`.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).without_time().init();
}
