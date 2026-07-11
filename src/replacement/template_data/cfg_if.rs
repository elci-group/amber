//! amber_cfg_if — Replacement for the `cfg-if` crate
//!
/// Simplifies conditional compilation. Use native `cfg!` or `#[cfg]` instead.
///
/// Example migration:
///
/// Before:
/// ```ignore
/// cfg_if::cfg_if! {
///     if #[cfg(unix)] {
///         fn platform() -> &'static str { "unix" }
///     } else if #[cfg(windows)] {
///         fn platform() -> &'static str { "windows" }
///     } else {
///         fn platform() -> &'static str { "unknown" }
///     }
/// }
/// ```
///
/// After (native Rust):
/// ```ignore
/// #[cfg(unix)]
/// fn platform() -> &'static str { "unix" }
/// #[cfg(windows)]
/// fn platform() -> &'static str { "windows" }
/// #[cfg(not(any(unix, windows)))]
/// fn platform() -> &'static str { "unknown" }
/// ```

/// Helper macro for cfg-if like behavior
#[macro_export]
macro_rules! cfg_if {
    (
        if #[cfg($($meta:meta),*)] { $($it:item)* }
        else if #[cfg($($emeta:meta),*)] { $($eit:item)* }
        else { $($dit:item)* }
    ) => {
        $(#[cfg($($meta),*)] $it)*
        $(#[cfg(all(not($($meta),*), $($emeta),*))] $eit)*
        $(#[cfg(not(any($($meta),*, $($emeta),*)))] $dit)*
    };
    (
        if #[cfg($($meta:meta),*)] { $($it:item)* }
        else { $($dit:item)* }
    ) => {
        $(#[cfg($($meta),*)] $it)*
        $(#[cfg(not($($meta),*))] $dit)*
    };
}

