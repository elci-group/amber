//! Internal recursive directory walker.
//!
//! Std-only replacement for the `walkdir` crate. Yields every file and
//! directory under a root path (depth-first). Symlinks are not followed.

use std::fs::{read_dir, symlink_metadata};
use std::path::{Path, PathBuf};

/// Iterator configuration for recursively walking a directory.
#[derive(Debug, Clone)]
pub struct WalkDir {
    root: PathBuf,
}

impl WalkDir {
    /// Create a new walker rooted at `path`.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            root: path.as_ref().to_path_buf(),
        }
    }
}

/// Owned iterator produced by [`WalkDir`].
pub struct IntoIter {
    stack: Vec<PathBuf>,
}

/// A single entry returned by the walker.
#[derive(Debug, Clone)]
pub struct DirEntry {
    path: PathBuf,
}

impl DirEntry {
    /// Path of this entry.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl IntoIterator for WalkDir {
    type Item = std::io::Result<DirEntry>;
    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            stack: vec![self.root],
        }
    }
}

impl Iterator for IntoIter {
    type Item = std::io::Result<DirEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let path = self.stack.pop()?;

            let metadata = match symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => return Some(Err(e)),
            };

            if metadata.is_dir() {
                match read_dir(&path) {
                    Ok(entries) => {
                        for entry in entries {
                            match entry {
                                Ok(e) => self.stack.push(e.path()),
                                Err(e) => return Some(Err(e)),
                            }
                        }
                    }
                    Err(e) => return Some(Err(e)),
                }
                continue;
            }

            return Some(Ok(DirEntry { path }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WalkDir;
    use std::collections::HashSet;

    #[test]
    fn walker_finds_rust_files() {
        let files: HashSet<_> = WalkDir::new("src/analysis")
            .into_iter()
            .filter_map(std::result::Result::ok)
            .map(|e| e.path().to_path_buf())
            .filter(|p| p.extension().is_some_and(|e| e == "rs"))
            .collect();

        assert!(files
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "walker.rs")));
        assert!(files
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "usage.rs")));
    }
}
