//! amber_env_logger — Minimal replacement for `env_logger`
//!
//! Initializes logging based on the `RUST_LOG` environment variable.

use std::env;
use std::sync::atomic::{AtomicU8, Ordering};

/// Log level enum
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

static MAX_LEVEL: AtomicU8 = AtomicU8::new(Level::Debug as u8);

/// Set the maximum log level.
pub fn set_max_level(level: Level) {
    MAX_LEVEL.store(level as u8, Ordering::Relaxed);
}

/// Initialize the logger from the environment.
pub fn init() {
    let rust_log = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let level = parse_level(&rust_log);
    set_max_level(level);
}

/// Builder pattern for configuration.
pub struct Builder {
    level: Level,
}

impl Builder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            level: Level::Info,
        }
    }

    #[must_use]
    pub fn parse_filters(mut self, filters: &str) -> Self {
        self.level = parse_level(filters);
        self
    }

    pub fn init(self) {
        set_max_level(self.level);
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_level(s: &str) -> Level {
    match s.to_lowercase().as_str() {
        "error" => Level::Error,
        "warn" => Level::Warn,
        "info" => Level::Info,
        "debug" => Level::Debug,
        "trace" => Level::Trace,
        _ => Level::Info,
    }
}
