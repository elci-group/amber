//! amber_tap — Replacement for the `tap` crate
//!
//! Provides tap methods for piping values through closures.

/// Tap trait for applying side-effect operations
pub trait Tap: Sized {
    /// Apply a closure for side effects, return self
    fn tap<F>(self, f: F) -> Self
    where
        F: FnOnce(&Self);

    /// Apply a mutable closure, return self
    fn tap_mut<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut Self);

    /// Apply a closure that may transform, return self on None
    fn tap_some<F>(self, f: F) -> Self
    where
        F: FnOnce(&Self);
}

impl<T: Sized> Tap for T {
    fn tap<F>(self, f: F) -> Self
    where
        F: FnOnce(&Self),
    {
        f(&self);
        self
    }

    fn tap_mut<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut Self),
    {
        f(&mut self);
        self
    }

    fn tap_some<F>(self, f: F) -> Self
    where
        F: FnOnce(&Self),
    {
        f(&self);
        self
    }
}

/// Pipe trait for functional composition
pub trait Pipe: Sized {
    /// Pipe self into a closure
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R;
}

impl<T: Sized> Pipe for T {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}

