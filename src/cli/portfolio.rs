// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! The `--portfolio` mode.
//!
//! Discovers every Amber-compatible Cargo project under a root directory and
//! runs the selected command against each one in turn, reusing the same
//! `Cli` flags for every project. A single project failing does not abort
//! the rest of the run.

use crate::amber_anyhow::Result;
use crate::cli::{run, Cli};
use crate::reporting::style::Colorize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Directory names Amber never descends into while discovering projects.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "target",
    "node_modules",
    ".cache",
    ".cargo",
    ".rustup",
    ".npm",
    ".pnpm-store",
    "dist",
    "build",
];

/// Resolve the root a portfolio run should scan.
///
/// When `path` is left at its default (`.`), the portfolio defaults to the
/// user's home directory; an explicitly supplied path is used as-is.
#[must_use]
pub fn resolve_root(path: &Path) -> PathBuf {
    if path == Path::new(".") {
        std::env::var_os("HOME").map_or_else(|| path.to_path_buf(), PathBuf::from)
    } else {
        path.to_path_buf()
    }
}

/// Run `cli`'s command against every Amber-compatible Cargo project found
/// under `root`, up to `max_projects`.
///
/// # Errors
///
/// Returns an error if `root` does not exist. Individual project failures
/// are printed and counted, not propagated, so one broken project does not
/// abort the rest of the portfolio.
pub fn run_portfolio(cli: &Cli, root: &Path, max_projects: usize) -> Result<i32> {
    if !root.exists() {
        crate::bail!("portfolio root {} does not exist", root.display());
    }

    let manifests = discover_manifests(root, max_projects);
    println!(
        "{} portfolio: {} project(s) found under {}",
        "\u{25c8}".bright_yellow(),
        manifests.len().to_string().bold(),
        root.display().to_string().cyan()
    );

    let mut worst_exit = 0i32;
    let mut failed = 0usize;

    for manifest in &manifests {
        let project = manifest.parent().unwrap_or(manifest);
        println!(
            "\n{} {}",
            "\u{25b6}".cyan().bold(),
            project.display().to_string().bold()
        );
        match run(cli, manifest) {
            Ok(code) => worst_exit = worst_exit.max(code),
            Err(e) => {
                failed += 1;
                eprintln!("  {} {e}", "error:".red().bold());
                warn!(project = %project.display(), error = %e, "portfolio: project failed");
            }
        }
    }

    println!(
        "\n{} portfolio complete: {} project(s), {} failed",
        "\u{25c8}".bright_yellow(),
        manifests.len().to_string().bold(),
        failed.to_string().bold()
    );

    Ok(if failed > 0 {
        worst_exit.max(2)
    } else {
        worst_exit
    })
}

fn discover_manifests(root: &Path, max_projects: usize) -> Vec<PathBuf> {
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
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
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
    found
}

fn is_amber_compatible(manifest: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(manifest) else {
        return false;
    };
    let Ok(value) = content.parse::<toml::Value>() else {
        return false;
    };
    value
        .as_table()
        .is_some_and(|table| table.contains_key("package") || table.contains_key("workspace"))
}

fn should_descend(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| !SKIP_DIRS.contains(&name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolve_root_defaults_to_home() {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        assert_eq!(resolve_root(Path::new(".")), home.unwrap_or_default());
    }

    #[test]
    fn resolve_root_keeps_explicit_path() {
        let explicit = PathBuf::from("/some/explicit/path");
        assert_eq!(resolve_root(&explicit), explicit);
    }

    #[test]
    fn discovery_skips_heavy_directories() {
        assert!(!should_descend(Path::new("target")));
        assert!(!should_descend(Path::new("node_modules")));
        assert!(should_descend(Path::new("workspace")));
    }

    #[test]
    fn discovery_finds_cargo_projects() {
        let temp = crate::temp::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("app/src")).unwrap();
        fs::write(
            root.join("app/Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(root.join("app/src/lib.rs"), "").unwrap();

        let manifests = discover_manifests(root, 8);
        assert_eq!(manifests, vec![root.join("app/Cargo.toml")]);
    }

    #[test]
    fn discovery_ignores_non_cargo_manifests_and_heavy_dirs() {
        let temp = crate::temp::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("app")).unwrap();
        fs::write(root.join("app/Cargo.toml"), "not valid toml").unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::write(
            root.join("node_modules/pkg/Cargo.toml"),
            "[package]\nname = \"pkg\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let manifests = discover_manifests(root, 8);
        assert!(manifests.is_empty());
    }

    #[test]
    fn discovery_respects_max_projects() {
        let temp = crate::temp::tempdir().unwrap();
        let root = temp.path();
        for name in ["a", "b", "c"] {
            fs::create_dir_all(root.join(name)).unwrap();
            fs::write(
                root.join(name).join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
            )
            .unwrap();
        }

        let manifests = discover_manifests(root, 2);
        assert_eq!(manifests.len(), 2);
    }
}
