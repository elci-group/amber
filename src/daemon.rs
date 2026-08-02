//! Lightweight home-directory monitor for Amber-compatible Cargo projects.
//!
//! Enabled by the `daemon` feature. The daemon is intentionally conservative:
//! it polls, skips heavyweight/generated directories, performs a cheap
//! dependency fingerprint before full analysis, and stores JSON reports in a
//! Padagonia-backed cache.

use crate::amber_anyhow::{Context, Result};
use crate::analysis::repo::RepositoryAnalyzer;
use crate::analysis::usage::UsageAnalyzer;
use crate::config::Config;
use crate::metadata::offline::OfflineProvider;
use crate::metadata::rustsec::RustSecEnricher;
use crate::reporting::formatters::JsonReporter;
use crate::scoring::classifier::SafetyClassifier;
use padagonia::{KeyId, Node, Provenance, Scalar, Store, StringTableExt};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs::OpenOptions;
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

const CACHE_LABEL: &str = "AmberAnalysisCache";
const PROP_PROJECT: &str = "project_root";
const PROP_MANIFEST: &str = "manifest_path";
const PROP_DEP_FP: &str = "dependency_fingerprint";
const PROP_SOURCE_FP: &str = "source_fingerprint";
const PROP_REPORT: &str = "report_json";
const PROP_CREATED: &str = "created_at";

/// Runtime configuration for the daemon.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct DaemonConfig {
    /// Directory tree to monitor.
    pub root: PathBuf,
    /// Padagonia file used for cached analysis reports.
    pub cache_path: PathBuf,
    /// Poll interval for continuous mode.
    pub interval: Duration,
    /// Run one scan and exit.
    pub once: bool,
    /// Maximum Cargo projects processed per scan.
    pub max_projects: usize,
    /// Include transitive dependencies in full analysis.
    pub include_transitive: bool,
    /// Include dev-dependencies in full analysis.
    pub include_dev: bool,
    /// Score threshold recorded in project marker metadata.
    pub threshold: u8,
    /// Re-run full analysis for source-only changes.
    pub analyze_source_changes: bool,
    /// Maximum analysis snapshots retained after cache compaction.
    pub max_cache_entries: usize,
    /// Maximum age retained after cache compaction.
    pub cache_max_age: Duration,
}

/// Polling monitor that discovers Cargo projects and refreshes cache entries.
#[derive(Debug)]
pub struct Daemon {
    config: DaemonConfig,
    cache: AnalysisCacheStore,
}

impl Daemon {
    /// Create a daemon and open its cache.
    ///
    /// # Errors
    ///
    /// Returns an error if the Padagonia cache cannot be opened.
    pub fn new(config: DaemonConfig) -> Result<Self> {
        let cache = AnalysisCacheStore::open(&config.cache_path).with_context(|| {
            format!(
                "failed to open analysis cache at {}",
                config.cache_path.display()
            )
        })?;
        Ok(Self { config, cache })
    }

    /// Run the daemon until stopped, or once when configured with `once`.
    ///
    /// # Errors
    ///
    /// Returns an error if scanning or cache persistence fails.
    pub fn run(&mut self) -> Result<i32> {
        loop {
            let summary = self.scan_once()?;
            println!(
                "amber daemon: scanned {} project(s), analyzed {}, cached {}, skipped {}",
                summary.discovered, summary.analyzed, summary.cached, summary.skipped
            );
            if self.config.once {
                return Ok(0);
            }
            std::thread::sleep(self.config.interval);
        }
    }

    /// Execute a single scan.
    ///
    /// # Errors
    ///
    /// Returns an error if project marker files or cache writes fail.
    pub fn scan_once(&mut self) -> Result<ScanSummary> {
        let manifests = discover_manifests(&self.config.root, self.config.max_projects)?;
        let mut summary = ScanSummary {
            discovered: manifests.len(),
            ..ScanSummary::default()
        };

        for manifest in manifests {
            match self.process_manifest(&manifest) {
                Ok(ScanDecision::Analyzed) => summary.analyzed += 1,
                Ok(ScanDecision::Cached) => summary.cached += 1,
                Ok(ScanDecision::Skipped) => summary.skipped += 1,
                Err(e) => {
                    summary.skipped += 1;
                    warn!(manifest = %manifest.display(), error = %e, "daemon project scan failed");
                }
            }
        }

        Ok(summary)
    }

    fn process_manifest(&mut self, manifest: &Path) -> Result<ScanDecision> {
        let project_root = manifest
            .parent()
            .map_or_else(|| Path::new(".").to_path_buf(), Path::to_path_buf);
        let dep_fingerprint = dependency_fingerprint(manifest)?;
        let source_fingerprint = source_fingerprint(&project_root)?;
        ensure_project_marker(&project_root, &self.config, &dep_fingerprint)?;

        let cached = self.cache.find_project(&project_root);
        if let Some(entry) = cached {
            if entry.dependency_fingerprint == dep_fingerprint
                && entry.source_fingerprint == source_fingerprint
            {
                debug!(project = %project_root.display(), "cache hit");
                return Ok(ScanDecision::Cached);
            }
            if entry.dependency_fingerprint == dep_fingerprint
                && !self.config.analyze_source_changes
            {
                info!(
                    project = %project_root.display(),
                    "source-only change detected; dependency-triggered analysis skipped"
                );
                return Ok(ScanDecision::Skipped);
            }
        }

        let report_json = run_full_analysis(
            manifest,
            self.config.include_transitive,
            self.config.include_dev,
        )?;
        self.cache.insert(
            &AnalysisCacheEntry {
                project_root: project_root.to_string_lossy().into_owned(),
                manifest_path: manifest.to_string_lossy().into_owned(),
                dependency_fingerprint: dep_fingerprint,
                source_fingerprint,
                report_json,
                created_at: now_unix(),
            },
            self.config.max_cache_entries,
            self.config.cache_max_age,
        )?;
        Ok(ScanDecision::Analyzed)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScanSummary {
    pub discovered: usize,
    pub analyzed: usize,
    pub cached: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanDecision {
    Analyzed,
    Cached,
    Skipped,
}

fn run_full_analysis(
    manifest: &Path,
    include_transitive: bool,
    include_dev: bool,
) -> Result<String> {
    let analyzer = RepositoryAnalyzer::with_provider(
        manifest,
        Box::new(RustSecEnricher::new(OfflineProvider::new())),
    )?;
    let deps = analyzer.list_dependencies(include_transitive, include_dev)?;
    let usage_analyzer = UsageAnalyzer::new(manifest)?;
    let usage_stats = usage_analyzer.analyze_all_usage(&deps)?;
    let classifier = SafetyClassifier::with_weights(
        Config::default()
            .weights
            .unwrap_or_else(crate::config::Weights::default),
    );
    let scores = deps
        .iter()
        .map(|dep| {
            let usage = usage_stats.get(&dep.name).cloned().unwrap_or_default();
            classifier.score_dependency(dep, &usage)
        })
        .collect::<Vec<_>>();
    JsonReporter::new().generate_json(&deps, &usage_stats, &scores)
}

fn discover_manifests(root: &Path, max_projects: usize) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut queue = VecDeque::from([root.to_path_buf()]);

    while let Some(dir) = queue.pop_front() {
        if found.len() >= max_projects {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut child_dirs = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_file()
                && entry.file_name() == "Cargo.toml"
                && is_amber_compatible(&path)
            {
                found.push(path);
                if found.len() >= max_projects {
                    break;
                }
            } else if file_type.is_dir() && should_descend(&path) {
                child_dirs.push(path);
            }
        }
        child_dirs.sort();
        queue.extend(child_dirs);
    }

    found.sort();
    Ok(found)
}

fn is_amber_compatible(manifest: &Path) -> bool {
    std::fs::read_to_string(manifest)
        .ok()
        .and_then(|content| content.parse::<toml::Value>().ok())
        .and_then(|value| value.as_table().cloned())
        .is_some_and(|table| table.contains_key("package") || table.contains_key("workspace"))
}

fn should_descend(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    !matches!(
        name,
        ".git"
            | ".hg"
            | ".svn"
            | "target"
            | "node_modules"
            | ".cache"
            | ".cargo"
            | ".rustup"
            | ".npm"
            | ".pnpm-store"
            | "dist"
            | "build"
    )
}

fn dependency_fingerprint(manifest: &Path) -> Result<String> {
    let content = std::fs::read_to_string(manifest)
        .with_context(|| format!("failed to read {}", manifest.display()))?;
    let parsed = content
        .parse::<toml::Value>()
        .with_context(|| format!("failed to parse {}", manifest.display()))?;
    let mut deps = BTreeMap::new();
    collect_dependency_tables("", &parsed, &mut deps);
    Ok(hash_value(&deps))
}

fn collect_dependency_tables(
    prefix: &str,
    value: &toml::Value,
    deps: &mut BTreeMap<String, String>,
) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (key, child) in table {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        if matches!(
            key.as_str(),
            "dependencies" | "dev-dependencies" | "build-dependencies"
        ) {
            deps.insert(path, child.to_string());
        } else if key == "target" || prefix.starts_with("target.") {
            collect_dependency_tables(&path, child, deps);
        }
    }
}

fn source_fingerprint(project_root: &Path) -> Result<String> {
    let src = project_root.join("src");
    if !src.exists() {
        return Ok(hash_value(&""));
    }

    let mut files = Vec::new();
    let mut queue = VecDeque::from([src]);
    while let Some(dir) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() && should_descend(&path) {
                queue.push_back(path);
            } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                let meta = entry.metadata()?;
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |duration| duration.as_secs());
                files.push(format!(
                    "{}:{}:{}",
                    path.strip_prefix(project_root)
                        .unwrap_or(&path)
                        .to_string_lossy(),
                    meta.len(),
                    modified
                ));
            }
        }
    }
    files.sort();
    Ok(hash_value(&files))
}

fn ensure_project_marker(
    project_root: &Path,
    config: &DaemonConfig,
    dep_fingerprint: &str,
) -> Result<()> {
    let marker_dir = project_root.join(".amber");
    std::fs::create_dir_all(&marker_dir)?;
    let marker = MarkerFile {
        version: env!("CARGO_PKG_VERSION"),
        cache_path: &config.cache_path.to_string_lossy(),
        threshold: config.threshold,
        dependency_fingerprint: dep_fingerprint,
        updated_at: now_unix(),
    };
    let content = toml::to_string_pretty(&marker)?;
    std::fs::write(marker_dir.join("daemon.toml"), content)?;
    Ok(())
}

fn hash_value<T: Hash>(value: &T) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[derive(Serialize)]
struct MarkerFile<'a> {
    version: &'a str,
    cache_path: &'a str,
    threshold: u8,
    dependency_fingerprint: &'a str,
    updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AnalysisCacheEntry {
    project_root: String,
    manifest_path: String,
    dependency_fingerprint: String,
    source_fingerprint: String,
    report_json: String,
    created_at: u64,
}

#[derive(Debug)]
struct AnalysisCacheStore {
    store: Store,
    path: PathBuf,
    _lock: CacheLock,
}

impl AnalysisCacheStore {
    fn open(path: &Path) -> std::io::Result<Self> {
        let lock = CacheLock::acquire(path)?;
        let store = if path.exists() {
            Store::load(path).map_err(io_err)?
        } else {
            Store::new()
        };
        Ok(Self {
            store,
            path: path.to_path_buf(),
            _lock: lock,
        })
    }

    fn find_project(&self, project_root: &Path) -> Option<AnalysisCacheEntry> {
        let project = project_root.to_string_lossy();
        let label_id = self.store.string_table.label_id(CACHE_LABEL)?;
        let ids = self.store.node_label_index.get(&label_id)?;
        ids.iter()
            .filter_map(|id| self.store.nodes.get(id))
            .filter_map(|node| self.entry_from_node(node))
            .filter(|entry| entry.project_root == project)
            .max_by_key(|entry| entry.created_at)
    }

    fn insert(
        &mut self,
        entry: &AnalysisCacheEntry,
        max_entries: usize,
        max_age: Duration,
    ) -> std::io::Result<()> {
        let props = vec![
            (PROP_PROJECT, Scalar::String(entry.project_root.clone())),
            (PROP_MANIFEST, Scalar::String(entry.manifest_path.clone())),
            (
                PROP_DEP_FP,
                Scalar::String(entry.dependency_fingerprint.clone()),
            ),
            (
                PROP_SOURCE_FP,
                Scalar::String(entry.source_fingerprint.clone()),
            ),
            (PROP_REPORT, Scalar::String(entry.report_json.clone())),
            (PROP_CREATED, Scalar::Timestamp(entry.created_at)),
        ];
        let prov = Provenance::new(
            "amber-daemon",
            env!("CARGO_PKG_VERSION"),
            1.0,
            0.0,
            entry.created_at,
            Vec::new(),
        );
        self.store.add_node(CACHE_LABEL, props, None, prov);
        self.compact(max_entries, max_age);
        self.save()
    }

    fn compact(&mut self, max_entries: usize, max_age: Duration) {
        let now = now_unix();
        let min_created_at = now.saturating_sub(max_age.as_secs());
        let mut entries = self.entries_with_ids();
        let mut remove = HashSet::new();

        for (id, entry) in &entries {
            if entry.created_at < min_created_at || !Path::new(&entry.manifest_path).exists() {
                remove.insert(*id);
            }
        }

        let mut latest_by_project = HashMap::<String, (padagonia::NodeId, u64)>::new();
        for (id, entry) in &entries {
            if remove.contains(id) {
                continue;
            }
            let replace = latest_by_project
                .get(&entry.project_root)
                .is_none_or(|(_, created_at)| entry.created_at > *created_at);
            if replace {
                if let Some((old_id, _)) =
                    latest_by_project.insert(entry.project_root.clone(), (*id, entry.created_at))
                {
                    remove.insert(old_id);
                }
            } else {
                remove.insert(*id);
            }
        }

        entries.retain(|(id, _)| !remove.contains(id));
        entries.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.created_at));
        for (id, _) in entries.into_iter().skip(max_entries.max(1)) {
            remove.insert(id);
        }

        for id in remove {
            self.store.nodes.remove(&id);
        }
    }

    fn entries_with_ids(&self) -> Vec<(padagonia::NodeId, AnalysisCacheEntry)> {
        let Some(label_id) = self.store.string_table.label_id(CACHE_LABEL) else {
            return Vec::new();
        };
        let Some(ids) = self.store.node_label_index.get(&label_id) else {
            return Vec::new();
        };
        ids.iter()
            .filter_map(|id| {
                self.store
                    .nodes
                    .get(id)
                    .and_then(|node| self.entry_from_node(node))
                    .map(|entry| (*id, entry))
            })
            .collect()
    }

    fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.store.save(&self.path).map_err(io_err)
    }

    fn entry_from_node(&self, node: &Node) -> Option<AnalysisCacheEntry> {
        Some(AnalysisCacheEntry {
            project_root: self.prop_string(&node.properties, PROP_PROJECT)?,
            manifest_path: self.prop_string(&node.properties, PROP_MANIFEST)?,
            dependency_fingerprint: self.prop_string(&node.properties, PROP_DEP_FP)?,
            source_fingerprint: self.prop_string(&node.properties, PROP_SOURCE_FP)?,
            report_json: self.prop_string(&node.properties, PROP_REPORT)?,
            created_at: self
                .prop_timestamp(&node.properties, PROP_CREATED)
                .unwrap_or(0),
        })
    }

    fn prop_string(&self, props: &[(KeyId, Scalar)], key: &str) -> Option<String> {
        let key_id = self.store.string_table.key_id(key)?;
        props.iter().find(|(k, _)| *k == key_id).and_then(|(_, v)| {
            if let Scalar::String(value) = v {
                Some(value.clone())
            } else {
                None
            }
        })
    }

    fn prop_timestamp(&self, props: &[(KeyId, Scalar)], key: &str) -> Option<u64> {
        let key_id = self.store.string_table.key_id(key)?;
        props.iter().find(|(k, _)| *k == key_id).and_then(|(_, v)| {
            if let Scalar::Timestamp(value) = v {
                Some(*value)
            } else {
                None
            }
        })
    }
}

fn io_err(e: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

#[derive(Debug)]
struct CacheLock {
    path: PathBuf,
}

impl CacheLock {
    fn acquire(cache_path: &Path) -> std::io::Result<Self> {
        let lock_path = lock_path(cache_path);
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => Ok(Self { path: lock_path }),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => Err(std::io::Error::new(
                ErrorKind::WouldBlock,
                format!(
                    "analysis cache is already locked at {}; another amber daemon may be running",
                    lock_path.display()
                ),
            )),
            Err(e) => Err(e),
        }
    }
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        let _: std::io::Result<()> = std::fs::remove_file(&self.path);
    }
}

fn lock_path(cache_path: &Path) -> PathBuf {
    let mut lock_path = cache_path.to_path_buf();
    let lock_extension = cache_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map_or_else(|| "lock".to_string(), |ext| format!("{ext}.lock"));
    lock_path.set_extension(lock_extension);
    lock_path
}

#[cfg(test)]
mod tests {
    use super::{
        dependency_fingerprint, discover_manifests, lock_path, should_descend, AnalysisCacheEntry,
        AnalysisCacheStore,
    };
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn dependency_fingerprint_changes_only_for_dependency_tables() {
        let root = temp_dir("dependency_fingerprint_changes_only_for_dependency_tables");
        let manifest = root.join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        let original = dependency_fingerprint(&manifest).unwrap();
        fs::write(
            &manifest,
            "[package]\nname = \"renamed\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        assert_eq!(original, dependency_fingerprint(&manifest).unwrap());
        fs::write(
            &manifest,
            "[package]\nname = \"renamed\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\ntoml = \"0.8\"\n",
        )
        .unwrap();
        assert_ne!(original, dependency_fingerprint(&manifest).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovery_skips_heavy_directories() {
        assert!(!should_descend(Path::new("target")));
        assert!(!should_descend(Path::new(".git")));
        assert!(should_descend(Path::new("workspace")));
    }

    #[test]
    fn discovery_finds_cargo_projects() {
        let root = temp_dir("discovery_finds_cargo_projects");
        fs::create_dir_all(root.join("app/src")).unwrap();
        fs::write(
            root.join("app/Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(root.join("app/src/lib.rs"), "").unwrap();
        let manifests = discover_manifests(&root, 8).unwrap();
        assert_eq!(manifests, vec![root.join("app/Cargo.toml")]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cache_lock_rejects_second_writer() {
        let root = temp_dir("cache_lock_rejects_second_writer");
        let cache = root.join("analysis.pad");
        let first = AnalysisCacheStore::open(&cache).unwrap();
        let second = AnalysisCacheStore::open(&cache);
        assert!(second.is_err());
        drop(first);
        assert!(AnalysisCacheStore::open(&cache).is_ok());
        assert!(!lock_path(&cache).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cache_compaction_keeps_latest_entry_per_project() {
        let root = temp_dir("cache_compaction_keeps_latest_entry_per_project");
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        let manifest = project.join("Cargo.toml");
        fs::write(&manifest, "[package]\nname = \"p\"\nversion = \"0.1.0\"\n").unwrap();
        let created_at = super::now_unix();

        let cache = root.join("analysis.pad");
        {
            let mut store = AnalysisCacheStore::open(&cache).unwrap();
            store
                .insert(
                    &entry(&project, &manifest, "old", created_at),
                    8,
                    std::time::Duration::from_secs(60),
                )
                .unwrap();
            store
                .insert(
                    &entry(&project, &manifest, "new", created_at + 1),
                    8,
                    std::time::Duration::from_secs(60),
                )
                .unwrap();
        }

        let store = AnalysisCacheStore::open(&cache).unwrap();
        let found = store.find_project(&project).unwrap();
        assert_eq!(found.report_json, "new");
        assert_eq!(store.entries_with_ids().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    fn entry(
        project_root: &Path,
        manifest_path: &Path,
        report_json: &str,
        created_at: u64,
    ) -> AnalysisCacheEntry {
        AnalysisCacheEntry {
            project_root: project_root.to_string_lossy().into_owned(),
            manifest_path: manifest_path.to_string_lossy().into_owned(),
            dependency_fingerprint: "deps".to_string(),
            source_fingerprint: "src".to_string(),
            report_json: report_json.to_string(),
            created_at,
        }
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("amber-daemon-{name}-{unique}"));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
