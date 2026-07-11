//! amber_lazy_static — Replacement for the `lazy_static` crate
//! 
//! Usage: Use `std::sync::OnceLock` (Rust 1.70+) or `std::sync::LazyLock` (Rust 1.80+)

/// Example migration:
/// 
/// Before (lazy_static):
/// ```ignore
/// lazy_static::lazy_static! {
///     static ref MY_VEC: Vec<u32> = vec![1, 2, 3];
/// }
/// ```
/// 
/// After (std):
/// ```ignore
/// static MY_VEC: LazyLock<Vec<u32>> = LazyLock::new(|| vec![1, 2, 3]);
/// ```

pub use std::sync::LazyLock;
pub use std::sync::OnceLock;

/// Helper macro for simple lazy static declarations
#[macro_export]
macro_rules! lazy_static {
    ($(#[$attr:meta])* static ref $name:ident: $ty:ty = $init:expr; $($rest:tt)*) => {
        $(#[$attr])*
        static $name: std::sync::LazyLock<$ty> = std::sync::LazyLock::new(|| $init);
        $crate::lazy_static!($($rest)*);
    };
    ($(#[$attr:meta])* static ref $name:ident: $ty:ty = $init:expr;) => {
        $(#[$attr])*
        static $name: std::sync::LazyLock<$ty> = std::sync::LazyLock::new(|| $init);
    };
    () => {};
}

