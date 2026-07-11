//! amber_once_cell — Replacement for the `once_cell` crate
//!
//! Use `std::sync::OnceLock` and `std::sync::LazyLock` instead.

pub use std::sync::LazyLock;
pub use std::sync::OnceLock;

/// Example migration:
///
/// Before (once_cell):
/// ```ignore
/// use once_cell::sync::Lazy;
/// static CONFIG: Lazy<String> = Lazy::new(|| std::fs::read_to_string("config.txt").unwrap());
/// ```
///
/// After (std):
/// ```ignore
/// static CONFIG: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
///     std::fs::read_to_string("config.txt").unwrap()
/// });
/// ```

