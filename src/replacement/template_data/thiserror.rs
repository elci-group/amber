//! amber_thiserror — Lightweight replacement for `thiserror`
//!
/// Derive macro replacement - manually implement Error for enums/structs
/// 
/// Example migration:
/// 
/// Before:
/// ```ignore
/// #[derive(thiserror::Error, Debug)]
/// enum MyError {
///     #[error("io failed: {0}")]
///     Io(#[from] std::io::Error),
///     #[error("invalid input: {0}")]
///     Invalid(String),
/// }
/// ```
///
/// After:
/// ```ignore
/// #[derive(Debug)]
/// enum MyError {
///     Io(std::io::Error),
///     Invalid(String),
/// }
///
/// impl std::fmt::Display for MyError {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///         match self {
///             MyError::Io(e) => write!(f, "io failed: {}", e),
///             MyError::Invalid(s) => write!(f, "invalid input: {}", s),
///         }
///     }
/// }
///
/// impl std::error::Error for MyError {
///     fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
///         match self {
///             MyError::Io(e) => Some(e),
///             _ => None,
///         }
///     }
/// }
///
/// impl From<std::io::Error> for MyError {
///     fn from(e: std::io::Error) -> Self { MyError::Io(e) }
/// }
/// ```

/// Helper to reduce boilerplate for simple error types
#[macro_export]
macro_rules! simple_error {
    ($name:ident { $($variant:ident($ty:ty) => $msg:expr),* $(,)? }) => {
        #[derive(Debug)]
        pub enum $name {
            $($variant($ty),)*
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(Self::$variant(e) => write!(f, $msg, e),)*
                }
            }
        }

        impl std::error::Error for $name {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                match self {
                    $(Self::$variant(e) => Some(e),)*
                }
            }
        }
    };
}

