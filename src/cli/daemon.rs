//! The `daemon` subcommand.

use crate::amber_anyhow::Result;
use crate::cli::Cli;
use crate::daemon::{Daemon, DaemonConfig};
use std::path::Path;
use std::time::Duration;

/// Parsed daemon options from the CLI layer.
#[derive(Clone, Copy)]
pub struct RunOptions<'a> {
    pub root: Option<&'a Path>,
    pub interval_secs: u64,
    pub once: bool,
    pub cache: Option<&'a Path>,
    pub max_projects: usize,
    pub analyze_source_changes: bool,
    pub max_cache_entries: usize,
    pub cache_max_age_days: u64,
}

/// Run the Amber daemon.
///
/// # Errors
///
/// Returns an error if the monitor root cannot be scanned, the cache cannot be
/// opened, or a marker file cannot be written.
pub fn run(cli: &Cli, options: RunOptions<'_>) -> Result<i32> {
    let config = DaemonConfig {
        root: options.root.map_or_else(default_root, Path::to_path_buf),
        cache_path: options
            .cache
            .map_or_else(default_cache_path, Path::to_path_buf),
        interval: Duration::from_secs(options.interval_secs.max(1)),
        once: options.once,
        max_projects: options.max_projects.max(1),
        include_transitive: cli.transitive,
        include_dev: !cli.no_dev,
        threshold: cli.threshold,
        analyze_source_changes: options.analyze_source_changes,
        max_cache_entries: options.max_cache_entries.max(1),
        cache_max_age: Duration::from_secs(options.cache_max_age_days.max(1) * 24 * 60 * 60),
    };

    Daemon::new(config)?.run()
}

fn default_root() -> std::path::PathBuf {
    std::env::var_os("HOME").map_or_else(|| std::path::PathBuf::from("."), Into::into)
}

fn default_cache_path() -> std::path::PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| std::path::PathBuf::from("."), Into::into);
    home.join(".amber").join("analysis-cache.pad")
}
