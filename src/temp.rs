//! Internal temporary-directory helpers.
//!
//! Std-only replacement for the `tempfile` crate. Creates uniquely-named
//! directories under `std::env::temp_dir()` and removes them on drop.

use std::fs::{create_dir_all, remove_dir_all};
use std::path::{Path, PathBuf};
use std::process::id;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// RAII temporary directory.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a new unique temporary directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    pub fn new() -> std::io::Result<Self> {
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("amber-{}-{count}", id()));
        create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// Path to the temporary directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _: std::io::Result<()> = remove_dir_all(&self.path);
    }
}

/// Convenience function creating a new [`TempDir`].
///
/// # Errors
///
/// Returns an error if the directory cannot be created.
pub fn tempdir() -> std::io::Result<TempDir> {
    TempDir::new()
}

#[cfg(test)]
mod tests {
    use super::TempDir;
    use std::path::Path;

    #[test]
    fn temp_dir_exists_and_cleans_up() {
        let path: std::path::PathBuf;
        {
            let temp = TempDir::new().unwrap();
            path = temp.path().to_path_buf();
            assert!(path.exists());
        }
        assert!(!path.exists());
    }

    #[test]
    fn temp_dir_implements_as_ref_path() {
        let temp = TempDir::new().unwrap();
        let _: &Path = temp.as_ref();
    }
}
