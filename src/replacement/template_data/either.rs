//! amber_either — Replacement for the `either` crate
//!
//! Provides Either<L, R> for dual-type contexts.

/// A value that is either Left(L) or Right(R)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Either<L, R> {
    /// Returns true if Left
    pub fn is_left(&self) -> bool {
        matches!(self, Either::Left(_))
    }

    /// Returns true if Right
    pub fn is_right(&self) -> bool {
        matches!(self, Either::Right(_))
    }

    /// Unwrap Left or panic
    pub fn unwrap_left(self) -> L {
        match self {
            Either::Left(l) => l,
            Either::Right(_) => panic!("called `Either::unwrap_left()` on a `Right` value"),
        }
    }

    /// Unwrap Right or panic
    pub fn unwrap_right(self) -> R {
        match self {
            Either::Right(r) => r,
            Either::Left(_) => panic!("called `Either::unwrap_right()` on a `Left` value"),
        }
    }

    /// Map over Left
    pub fn map_left<F, T>(self, f: F) -> Either<T, R>
    where
        F: FnOnce(L) -> T,
    {
        match self {
            Either::Left(l) => Either::Left(f(l)),
            Either::Right(r) => Either::Right(r),
        }
    }

    /// Map over Right
    pub fn map_right<F, T>(self, f: F) -> Either<L, T>
    where
        F: FnOnce(R) -> T,
    {
        match self {
            Either::Left(l) => Either::Left(l),
            Either::Right(r) => Either::Right(f(r)),
        }
    }

    /// Factor an Iterator of Eithers into one of two collections
    pub fn factor_iter<I>(iter: I) -> (Vec<L>, Vec<R>)
    where
        I: Iterator<Item = Either<L, R>>,
    {
        let mut lefts = Vec::new();
        let mut rights = Vec::new();
        for item in iter {
            match item {
                Either::Left(l) => lefts.push(l),
                Either::Right(r) => rights.push(r),
            }
        }
        (lefts, rights)
    }
}

impl<L, R, T> Either<L, R>
where
    L: Into<T>,
    R: Into<T>,
{
    /// Convert either variant into a common type
    pub fn into_inner(self) -> T {
        match self {
            Either::Left(l) => l.into(),
            Either::Right(r) => r.into(),
        }
    }
}

