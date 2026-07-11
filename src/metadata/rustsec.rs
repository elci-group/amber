//! `RustSec` advisory database integration.
//!
//! Loads (or updates) the `RustSec` advisory database from a local cache
//! directory and counts matching advisories per crate. The database is
//! fetched with the `git` CLI when it is missing or stale.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use rustsec::database::Database;
use tracing::{debug, warn};

use super::{CrateMetadata, MetadataProvider};
use crate::analysis::types::Dependency;

const RUSTSEC_DB_URL: &str = "https://github.com/RustSec/advisory-db.git";
const FALLBACK_CACHE_DIR: &str = ".amber/rustsec-advisory-db";
const CACHE_SUBDIR: &str = "amber/rustsec-advisory-db";

type GitRunner = Box<dyn Fn(&Path, &[&str]) -> Result<(), RustSecError> + Send + Sync>;

/// Resolve the cache directory from environment variables.
///
/// Honours `XDG_CACHE_HOME` on all platforms, then falls back to
/// platform-specific conventions:
/// - Windows: `%LOCALAPPDATA%` or `%USERPROFILE%\AppData\Local`
/// - macOS: `~/Library/Caches`
/// - Other Unix: `~/.cache`
///
/// If no home directory can be determined, returns `None` so callers can fall
/// back to a local directory.
#[allow(clippy::too_many_arguments)]
fn resolve_cache_dir(
    xdg_cache_home: Option<&str>,
    home: Option<&str>,
    _local_app_data: Option<&str>,
    _user_profile: Option<&str>,
) -> Option<PathBuf> {
    if let Some(xdg) = xdg_cache_home {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join(CACHE_SUBDIR));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(local) = local_app_data {
            if !local.is_empty() {
                return Some(PathBuf::from(local).join(CACHE_SUBDIR));
            }
        }
        if let Some(profile) = user_profile {
            if !profile.is_empty() {
                return Some(
                    PathBuf::from(profile)
                        .join("AppData/Local")
                        .join(CACHE_SUBDIR),
                );
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(h) = home {
            if !h.is_empty() {
                return Some(PathBuf::from(h).join("Library/Caches").join(CACHE_SUBDIR));
            }
        }
        None
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(h) = home {
            if !h.is_empty() {
                return Some(PathBuf::from(h).join(".cache").join(CACHE_SUBDIR));
            }
        }
        None
    }
}

fn default_cache_dir() -> PathBuf {
    resolve_cache_dir(
        std::env::var("XDG_CACHE_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
        std::env::var("LOCALAPPDATA").ok().as_deref(),
        std::env::var("USERPROFILE").ok().as_deref(),
    )
    .unwrap_or_else(|| PathBuf::from(FALLBACK_CACHE_DIR))
}

/// Source of `RustSec` security advisories backed by a local git cache.
pub struct RustSecSource {
    cache_dir: PathBuf,
    db: OnceLock<Option<Database>>,
    git_runner: GitRunner,
}

impl RustSecSource {
    /// Create a new source using the default platform cache directory.
    ///
    /// The database is stored in the user's standard cache location
    /// (e.g. `~/.cache/amber/rustsec-advisory-db` on Linux,
    /// `~/Library/Caches/amber/rustsec-advisory-db` on macOS, or
    /// `%LOCALAPPDATA%\amber\rustsec-advisory-db` on Windows). If no standard
    /// cache location can be determined, it falls back to a local
    /// `.amber/rustsec-advisory-db` directory under the current working
    /// directory.
    #[must_use]
    pub fn new() -> Self {
        Self::with_cache_dir(default_cache_dir())
    }

    /// Create a new source using the provided cache directory.
    #[must_use]
    pub fn with_cache_dir(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            db: OnceLock::new(),
            git_runner: Box::new(Self::run_git),
        }
    }

    #[cfg(test)]
    fn with_git_runner(cache_dir: impl Into<PathBuf>, git_runner: GitRunner) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            db: OnceLock::new(),
            git_runner,
        }
    }

    /// Return the number of non-withdrawn advisories affecting `crate_name`.
    pub fn advisory_count(&self, crate_name: &str) -> usize {
        self.database()
            .map_or(0, |db| Self::count_advisories(db, crate_name))
    }

    fn database(&self) -> Option<&Database> {
        self.db
            .get_or_init(|| {
                if let Err(e) = self.ensure_database() {
                    warn!("RustSec advisory database unavailable: {e}");
                    return None;
                }
                match Database::open(&self.cache_dir) {
                    Ok(db) => Some(db),
                    Err(e) => {
                        warn!("Failed to open RustSec database: {e}");
                        None
                    }
                }
            })
            .as_ref()
    }

    fn count_advisories(db: &Database, crate_name: &str) -> usize {
        db.iter()
            .filter(|advisory| {
                !advisory.withdrawn() && advisory.metadata.package.as_str() == crate_name
            })
            .count()
    }

    fn ensure_database(&self) -> Result<(), RustSecError> {
        let crates_dir = self.cache_dir.join("crates");
        if crates_dir.is_dir() {
            debug!(
                "Updating RustSec advisory database in {}",
                self.cache_dir.display()
            );
            // Fetch the latest shallow commit and reset to it. This is more
            // reliable than `git pull --ff-only --depth 1` when the remote
            // default branch has moved non-linearly.
            (self.git_runner)(&self.cache_dir, &["fetch", "--depth", "1", "origin"])?;
            (self.git_runner)(&self.cache_dir, &["reset", "--hard", "FETCH_HEAD"])
        } else {
            debug!(
                "Cloning RustSec advisory database to {}",
                self.cache_dir.display()
            );
            std::fs::create_dir_all(&self.cache_dir)?;
            let parent = self.cache_dir.parent().unwrap_or(&self.cache_dir);
            (self.git_runner)(
                parent,
                &[
                    "clone",
                    "--depth",
                    "1",
                    RUSTSEC_DB_URL,
                    &self.cache_dir.to_string_lossy(),
                ],
            )
        }
    }

    fn run_git(cwd: &Path, args: &[&str]) -> Result<(), RustSecError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .map_err(|e| RustSecError::Git(format!("failed to run git: {e}")))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(RustSecError::Git(format!("git exited with {stderr}")))
        }
    }
}

impl Default for RustSecSource {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
enum RustSecError {
    Io(std::io::Error),
    Git(String),
}

impl std::fmt::Display for RustSecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Git(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for RustSecError {}

impl From<std::io::Error> for RustSecError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Wraps another metadata provider and enriches CVE counts from `RustSec`.
pub struct RustSecEnricher<P> {
    inner: P,
    source: RustSecSource,
}

impl<P> RustSecEnricher<P> {
    /// Create a new enricher around `inner`.
    #[must_use]
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            source: RustSecSource::new(),
        }
    }

    #[cfg(test)]
    const fn with_source(inner: P, source: RustSecSource) -> Self {
        Self { inner, source }
    }
}

impl<P: MetadataProvider> MetadataProvider for RustSecEnricher<P> {
    fn fetch(&self, dep: &Dependency) -> crate::amber_anyhow::Result<CrateMetadata> {
        let mut meta = self.inner.fetch(dep)?;
        meta.cve_count = self.source.advisory_count(&dep.name);
        Ok(meta)
    }

    fn fetch_batch(&self, deps: &[Dependency]) -> Vec<crate::amber_anyhow::Result<CrateMetadata>> {
        let mut results = self.inner.fetch_batch(deps);

        if let Some(db) = self.source.database() {
            let counts: HashMap<String, usize> = deps
                .iter()
                .map(|dep| {
                    (
                        dep.name.clone(),
                        RustSecSource::count_advisories(db, &dep.name),
                    )
                })
                .collect();

            for (result, dep) in results.iter_mut().zip(deps.iter()) {
                if let Ok(meta) = result {
                    meta.cve_count = *counts.get(&dep.name).unwrap_or(&0);
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::types::{Dependency, DependencyKind, DependencySource};

    #[test]
    fn default_source_matches_new() {
        let a = RustSecSource::default();
        let b = RustSecSource::new();
        assert_eq!(a.cache_dir, b.cache_dir);
    }

    #[test]
    fn resolve_cache_dir_prefers_xdg() {
        let dir = resolve_cache_dir(Some("/xdg/cache"), Some("/home/user"), None, None);
        assert_eq!(dir, Some(PathBuf::from("/xdg/cache").join(CACHE_SUBDIR)));
    }

    #[test]
    fn resolve_cache_dir_ignores_empty_xdg() {
        let dir = resolve_cache_dir(Some(""), Some("/home/user"), None, None);
        #[cfg(target_os = "macos")]
        assert_eq!(
            dir,
            Some(PathBuf::from("/home/user/Library/Caches").join(CACHE_SUBDIR))
        );
        #[cfg(target_os = "windows")]
        assert_eq!(dir, None);
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        assert_eq!(
            dir,
            Some(PathBuf::from("/home/user/.cache").join(CACHE_SUBDIR))
        );
    }

    #[test]
    fn resolve_cache_dir_returns_none_when_no_home() {
        let dir = resolve_cache_dir(None, None, None, None);
        #[cfg(target_os = "windows")]
        assert_eq!(dir, None);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(dir, None);
    }

    #[test]
    fn default_cache_dir_is_not_empty() {
        let dir = default_cache_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn rustsec_error_display_formats() {
        let io_err = RustSecError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
        assert!(io_err.to_string().contains("missing"));

        let git_err = RustSecError::Git("clone failed".to_string());
        assert_eq!(git_err.to_string(), "clone failed");
    }

    #[test]
    fn rustsec_error_from_io() {
        let io = std::io::Error::other("oops");
        let err: RustSecError = io.into();
        assert!(err.to_string().contains("oops"));
    }

    #[test]
    fn git_failure_returns_zero_advisories() {
        let temp = crate::temp::tempdir().unwrap();
        let source = RustSecSource::with_git_runner(
            temp.path().join("db"),
            Box::new(|_cwd, _args| Err(RustSecError::Git("no git".to_string()))),
        );
        assert_eq!(source.advisory_count("serde"), 0);
    }

    #[test]
    fn clone_success_but_invalid_db_returns_zero() {
        let temp = crate::temp::tempdir().unwrap();
        let db_dir = temp.path().join("db");
        let source = RustSecSource::with_git_runner(
            db_dir.clone(),
            Box::new(move |_cwd, _args| {
                // Create a crates dir so ensure_database takes the update path next time,
                // but leave the DB invalid so Database::open fails.
                std::fs::create_dir_all(db_dir.join("crates")).unwrap();
                Ok(())
            }),
        );
        assert_eq!(source.advisory_count("serde"), 0);
    }

    #[test]
    fn enricher_propagates_default_cve_count() {
        struct MockProvider;
        impl MetadataProvider for MockProvider {
            fn fetch(&self, _dep: &Dependency) -> crate::amber_anyhow::Result<CrateMetadata> {
                Ok(CrateMetadata::default())
            }
        }

        let temp = crate::temp::tempdir().unwrap();
        let source = RustSecSource::with_git_runner(
            temp.path().join("db"),
            Box::new(|_cwd, _args| Err(RustSecError::Git("no git".to_string()))),
        );
        let enricher = RustSecEnricher::with_source(MockProvider, source);
        let dep = Dependency {
            name: "serde".to_string(),
            version: "1.0.0".to_string(),
            source: DependencySource::CratesIo,
            kind: DependencyKind::Normal,
            features: Vec::new(),
            optional: false,
            uses_default_features: true,
            transitive_deps: Vec::new(),
            loc_approx: 0,
            public_api_count: 0,
            last_release: None,
            maintenance_score: 0,
            cve_count: 0,
            license: None,
            download_count: 0,
        };
        let meta = enricher.fetch(&dep).unwrap();
        assert_eq!(meta.cve_count, 0);
    }

    fn write_minimal_advisory(db_dir: &std::path::Path, crate_name: &str, id: &str) {
        let crate_dir = db_dir.join("crates").join(crate_name);
        std::fs::create_dir_all(&crate_dir).unwrap();
        std::fs::write(
            crate_dir.join(format!("{id}.md")),
            format!(
                r#"```toml
[advisory]
id = "{id}"
package = "{crate_name}"
date = "2021-01-01"
url = "https://example.com"
categories = ["code-execution"]

[versions]
patched = [">= 1.0.0"]
```

# Test advisory for {crate_name}

Description goes here.
"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn advisory_count_uses_open_database() {
        let temp = crate::temp::tempdir().unwrap();
        let db_dir = temp.path().join("db");
        write_minimal_advisory(&db_dir, "vulnerable", "RUSTSEC-2021-0001");

        // Verify the database loads before wrapping it in the source.
        let db = Database::open(&db_dir).expect("open test database");
        assert_eq!(db.iter().count(), 1, "expected one advisory in test db");

        // Pre-seed the DB so ensure_database takes the update path.
        let source = RustSecSource::with_git_runner(
            db_dir.clone(),
            Box::new(move |_cwd, _args| {
                assert!(db_dir.join("crates").is_dir());
                Ok(())
            }),
        );
        assert_eq!(source.advisory_count("vulnerable"), 1);
        assert_eq!(source.advisory_count("safe"), 0);
    }

    #[test]
    fn fetch_batch_enriches_cve_counts() {
        struct MockProvider;
        impl MetadataProvider for MockProvider {
            fn fetch(&self, _dep: &Dependency) -> crate::amber_anyhow::Result<CrateMetadata> {
                Ok(CrateMetadata::default())
            }

            fn fetch_batch(
                &self,
                deps: &[Dependency],
            ) -> Vec<crate::amber_anyhow::Result<CrateMetadata>> {
                deps.iter().map(|_| Ok(CrateMetadata::default())).collect()
            }
        }

        let temp = crate::temp::tempdir().unwrap();
        let db_dir = temp.path().join("db");
        write_minimal_advisory(&db_dir, "vulnerable", "RUSTSEC-2021-0001");

        let source = RustSecSource::with_git_runner(db_dir, Box::new(move |_cwd, _args| Ok(())));
        let enricher = RustSecEnricher::with_source(MockProvider, source);
        let deps = vec![
            Dependency {
                name: "vulnerable".to_string(),
                version: "1.0.0".to_string(),
                source: DependencySource::CratesIo,
                kind: DependencyKind::Normal,
                features: Vec::new(),
                optional: false,
                uses_default_features: true,
                transitive_deps: Vec::new(),
                loc_approx: 0,
                public_api_count: 0,
                last_release: None,
                maintenance_score: 0,
                cve_count: 0,
                license: None,
                download_count: 0,
            },
            Dependency {
                name: "safe".to_string(),
                version: "1.0.0".to_string(),
                source: DependencySource::CratesIo,
                kind: DependencyKind::Normal,
                features: Vec::new(),
                optional: false,
                uses_default_features: true,
                transitive_deps: Vec::new(),
                loc_approx: 0,
                public_api_count: 0,
                last_release: None,
                maintenance_score: 0,
                cve_count: 0,
                license: None,
                download_count: 0,
            },
        ];
        let results = enricher.fetch_batch(&deps);
        assert_eq!(results[0].as_ref().unwrap().cve_count, 1);
        assert_eq!(results[1].as_ref().unwrap().cve_count, 0);
    }
}
