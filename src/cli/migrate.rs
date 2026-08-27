// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! The `migrate` subcommand (requires the `migrate` Cargo feature).
//!
//! Applies a validated replacement module to the target project in one shot:
//!
//! 1. Verifies the crate is a direct dependency of the target `Cargo.toml`.
//! 2. Copies the replacement module into the target's `src/` directory and
//!    declares it with `mod <name>;` in `src/lib.rs` (preferred) or
//!    `src/main.rs`.
//! 3. Rewrites `use <crate>::…` imports and fully-qualified `<crate>::…`
//!    paths to the new module name in every `.rs` file under `src/`. Hyphens
//!    in the crate name map to underscores in Rust paths (e.g.
//!    `tracing-subscriber` → `tracing_subscriber`), mirroring the mapping in
//!    usage analysis. Paths are rewritten to `crate::<module>::…` (not a bare
//!    `<module>::…`): under Rust 2018+ path-clarity rules a bare module path
//!    only resolves in the crate root file, so any submodule would fail to
//!    compile. Edition-2015 targets need a manual fix-up; the `cargo check`
//!    step catches that and rolls back.
//! 4. Removes the dependency key from `[dependencies]` and
//!    `[dev-dependencies]` in `Cargo.toml` using `toml_edit`, which preserves
//!    the rest of the document (comments, ordering, formatting).
//! 5. Runs `cargo check` in the target project; on failure every change is
//!    rolled back from in-memory snapshots (original `Cargo.toml`,
//!    `Cargo.lock`, and source file bytes are restored, the copied module is
//!    deleted) and an error is returned.
//!
//! ## Rewrite limitations
//!
//! The source rewrite is deliberately line-based, not AST-based:
//!
//! - Lines whose first non-whitespace characters are `//` are skipped, so
//!   comment lines are preserved. Occurrences of `<crate>::` inside string
//!   literals on code lines *are* rewritten; there is no trivial line-based
//!   way to distinguish them.
//! - Path-qualified macro invocations such as `anyhow::bail!(…)` are
//!   rewritten textually to `crate::<module>::bail!(…)`, but
//!   `#[macro_export]` macros live at the crate root and are *not* resolvable
//!   through a module path. Such call sites need a manual fix-up; because the
//!   project then fails `cargo check`, the migration is rolled back rather
//!   than left half-applied.
//! - `extern crate <name>;` lines and renamed dependencies
//!   (`alias = { package = "<crate>" }`) are not handled.
//! - Only the top-level `[dependencies]` and `[dev-dependencies]` tables of
//!   the target manifest are edited; workspace-inherited dependencies are
//!   out of scope.

use crate::amber_anyhow::{Context, Result};
use crate::analysis::walker::WalkDir;
use crate::replacement::validator::{summarize_stderr, Validator};
use crate::reporting::style::Colorize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use toml_edit::DocumentMut;
use tracing::{info, warn};

/// Maximum time to wait for the post-migration `cargo check`.
const CARGO_CHECK_TIMEOUT: Duration = Duration::from_secs(300);

/// A fully planned migration, ready to preview or apply.
struct MigrationPlan {
    manifest_path: PathBuf,
    project_dir: PathBuf,
    crate_name: String,
    module_name: String,
    /// Manifest content with the dependency removed.
    edited_toml: String,
    /// Dependency tables the crate was removed from.
    sections: Vec<&'static str>,
    replace_with: PathBuf,
    module_dest: PathBuf,
    entry_point: PathBuf,
    /// New entry-point content (imports rewritten and `mod` declaration
    /// appended), or `None` if the entry point needs no change.
    entry_new_content: Option<String>,
    /// Whether the `mod <name>;` declaration was added to the entry point.
    mod_decl_added: bool,
    /// Import lines rewritten in the entry point itself.
    entry_changed_lines: usize,
    /// Rewrites for source files other than the entry point.
    rewrites: Vec<FileRewrite>,
}

/// A planned line-based rewrite of a single source file.
struct FileRewrite {
    path: PathBuf,
    new_content: String,
    changed_lines: usize,
}

/// In-memory snapshot of a file, used to roll back a failed migration.
struct FileSnapshot {
    path: PathBuf,
    /// Original bytes, or `None` if the file did not exist.
    original: Option<Vec<u8>>,
}

/// Run the `migrate` command for `crate_name` against `manifest_path`.
///
/// # Errors
///
/// Returns an error if the crate is not a direct dependency, the replacement
/// file is missing or has an invalid module name, the target has no `src/`
/// entry point, any file cannot be read or written, or `cargo check` fails
/// after applying the migration (in which case all changes are rolled back
/// first).
pub fn run(
    manifest_path: &Path,
    crate_name: &str,
    replace_with: &Path,
    dry_run: bool,
) -> Result<i32> {
    info!(crate = %crate_name, manifest = %manifest_path.display(), "migrating dependency to replacement module");
    let plan = plan_migration(manifest_path, crate_name, replace_with)?;

    if dry_run {
        print_dry_run(&plan);
        return Ok(0);
    }

    // Snapshot every file that will change *before* writing anything, so a
    // failure at any point can be rolled back completely.
    let mut snapshots = Vec::with_capacity(plan.rewrites.len() + 4);
    snapshots.push(snapshot_file(&plan.manifest_path)?);
    snapshots.push(snapshot_file(&plan.project_dir.join("Cargo.lock"))?);
    snapshots.push(snapshot_file(&plan.entry_point)?);
    for rewrite in &plan.rewrites {
        snapshots.push(snapshot_file(&rewrite.path)?);
    }
    snapshots.push(FileSnapshot {
        path: plan.module_dest.clone(),
        original: None,
    });

    if let Err(error) = apply_migration(&plan) {
        warn!(crate = %crate_name, %error, "migration write failed; restoring snapshots");
        restore_snapshots(&snapshots);
        return Err(error);
    }

    match run_cargo_check(&plan.project_dir) {
        Ok((true, _)) => {}
        Ok((false, stderr)) => {
            restore_snapshots(&snapshots);
            crate::bail!("cargo check failed; migration was rolled back\n{stderr}");
        }
        Err(error) => {
            warn!(crate = %crate_name, %error, "cargo check could not run; restoring snapshots");
            restore_snapshots(&snapshots);
            crate::bail!("failed to run cargo check; migration was rolled back: {error}");
        }
    }

    print_summary(&plan);
    Ok(0)
}

/// Validate the inputs and compute every change without writing anything.
fn plan_migration(
    manifest_path: &Path,
    crate_name: &str,
    replace_with: &Path,
) -> Result<MigrationPlan> {
    let manifest_path = resolve_manifest(manifest_path)?;
    let project_dir = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let toml_content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;

    // 1. The crate must be a direct dependency; compute the edited manifest.
    let edit = remove_dependency(&toml_content, crate_name)?.ok_or_else(|| {
        crate::anyhow!(
            "crate '{crate_name}' is not a direct dependency of {}",
            manifest_path.display()
        )
    })?;

    // 2. The replacement module must exist; its file stem is the module name.
    if !replace_with.is_file() {
        crate::bail!(
            "replacement module {} does not exist or is not a file",
            replace_with.display()
        );
    }
    let module_name = replace_with
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .context("replacement file has no valid UTF-8 file stem")?
        .to_string();
    Validator::validate_module_name(&module_name)?;

    let src_dir = project_dir.join("src");
    if !src_dir.is_dir() {
        crate::bail!(
            "target project has no src/ directory at {}",
            src_dir.display()
        );
    }
    let module_dest = src_dir.join(format!("{module_name}.rs"));
    if module_dest.exists() {
        crate::bail!(
            "module file {} already exists; refusing to overwrite",
            module_dest.display()
        );
    }

    // 3. Entry point: lib.rs preferred, main.rs as fallback. Its imports are
    //    rewritten together with the `mod` declaration so neither change
    //    clobbers the other when the file is written.
    let crate_path = crate_name.replace('-', "_");
    let entry_point = find_entry_point(&src_dir)?;
    let entry_content = fs::read_to_string(&entry_point)
        .with_context(|| format!("failed to read {}", entry_point.display()))?;
    let (rewritten_entry, entry_changed_lines) =
        rewrite_source(&entry_content, &crate_path, &module_name);
    let mod_decl_added = !has_mod_decl(&rewritten_entry, &module_name);
    let entry_new_content = ensure_mod_decl(&rewritten_entry, &module_name)
        .filter(|content| content != &entry_content)
        .or_else(|| (rewritten_entry != entry_content).then_some(rewritten_entry));

    // 4. Plan rewrites for the remaining sources now, before the module is
    //    copied into src/, so the new module file itself is never scanned.
    let rewrites = plan_rewrites(&src_dir, &entry_point, &crate_path, &module_name)?;

    Ok(MigrationPlan {
        manifest_path,
        project_dir,
        crate_name: crate_name.to_string(),
        module_name,
        edited_toml: edit.content,
        sections: edit.sections,
        replace_with: replace_with.to_path_buf(),
        module_dest,
        entry_point,
        entry_new_content,
        mod_decl_added,
        entry_changed_lines,
        rewrites,
    })
}

/// Resolve `manifest_path` to an existing `Cargo.toml` file.
fn resolve_manifest(manifest_path: &Path) -> Result<PathBuf> {
    let manifest = if manifest_path.is_dir() {
        manifest_path.join("Cargo.toml")
    } else {
        manifest_path.to_path_buf()
    };
    if !manifest.is_file() {
        crate::bail!("no Cargo.toml found at {}", manifest.display());
    }
    Ok(manifest)
}

/// Result of editing a manifest: the new content and the tables that changed.
struct ManifestEdit {
    content: String,
    sections: Vec<&'static str>,
}

/// Remove `crate_name` from `[dependencies]` and `[dev-dependencies]`,
/// preserving the rest of the document. Returns `None` if the crate is not a
/// direct dependency in either table.
fn remove_dependency(toml_content: &str, crate_name: &str) -> Result<Option<ManifestEdit>> {
    let mut document = toml_content
        .parse::<DocumentMut>()
        .context("failed to parse Cargo.toml")?;
    let mut sections = Vec::new();
    for section in ["dependencies", "dev-dependencies"] {
        let removed = document
            .get_mut(section)
            .and_then(toml_edit::Item::as_table_like_mut)
            .is_some_and(|table| table.remove(crate_name).is_some());
        if removed {
            sections.push(section);
        }
    }
    if sections.is_empty() {
        return Ok(None);
    }
    Ok(Some(ManifestEdit {
        content: document.to_string(),
        sections,
    }))
}

/// Pick the crate root source file: `src/lib.rs` preferred over `src/main.rs`.
fn find_entry_point(src_dir: &Path) -> Result<PathBuf> {
    let lib = src_dir.join("lib.rs");
    if lib.is_file() {
        return Ok(lib);
    }
    let main = src_dir.join("main.rs");
    if main.is_file() {
        return Ok(main);
    }
    crate::bail!(
        "target project has neither src/lib.rs nor src/main.rs under {}",
        src_dir.display()
    );
}

/// Plan rewrites for every `.rs` file under `src_dir` that mentions the
/// crate, skipping the entry point (its rewrite is planned separately,
/// together with the `mod` declaration).
fn plan_rewrites(
    src_dir: &Path,
    entry_point: &Path,
    crate_path: &str,
    module_name: &str,
) -> Result<Vec<FileRewrite>> {
    let mut rewrites = Vec::new();
    for entry in WalkDir::new(src_dir)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if path == entry_point || path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let (new_content, changed_lines) = rewrite_source(&content, crate_path, module_name);
        if changed_lines > 0 {
            rewrites.push(FileRewrite {
                path: path.to_path_buf(),
                new_content,
                changed_lines,
            });
        }
    }
    Ok(rewrites)
}

/// Rewrite every non-comment line of `content`, returning the new content and
/// the number of changed lines. Line endings (including a missing trailing
/// newline) are preserved.
fn rewrite_source(content: &str, crate_path: &str, module_name: &str) -> (String, usize) {
    let mut changed = 0;
    let mut output = String::with_capacity(content.len());
    for line in content.lines() {
        let rewritten = rewrite_line(line, crate_path, module_name);
        if rewritten != line {
            changed += 1;
        }
        output.push_str(&rewritten);
        output.push('\n');
    }
    if !content.ends_with('\n') {
        output.pop();
    }
    (output, changed)
}

/// Rewrite a single line, mapping `<crate_path>::` path prefixes and bare
/// `use <crate_path>;` imports to the replacement module via a `crate::`
/// prefix (required for the path to resolve from submodules under Rust 2018+
/// path-clarity rules). Comment lines (first non-whitespace characters are
/// `//`) are left untouched.
fn rewrite_line(line: &str, crate_path: &str, module_name: &str) -> String {
    if line.trim_start().starts_with("//") {
        return line.to_string();
    }
    line.replace(
        &format!("{crate_path}::"),
        &format!("crate::{module_name}::"),
    )
    .replace(
        &format!("use {crate_path};"),
        &format!("use crate::{module_name};"),
    )
    .replace(
        &format!("use {crate_path} as "),
        &format!("use crate::{module_name} as "),
    )
}

/// Return `true` if `content` already declares `mod <module_name>;` (plain,
/// `pub`, or otherwise qualified) on a non-comment line.
fn has_mod_decl(content: &str, module_name: &str) -> bool {
    let needle = format!("mod {module_name};");
    content.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with("//")
            && (trimmed == needle || trimmed.ends_with(&format!(" {needle}")))
    })
}

/// Append `mod <module_name>;` to `content` unless it is already declared.
/// Returns `None` when no change is needed.
fn ensure_mod_decl(content: &str, module_name: &str) -> Option<String> {
    if has_mod_decl(content, module_name) {
        return None;
    }
    let mut output = content.to_string();
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str("mod ");
    output.push_str(module_name);
    output.push_str(";\n");
    Some(output)
}

/// Read `path` into memory; a missing file snapshots as `None` so rollback
/// deletes it.
fn snapshot_file(path: &Path) -> Result<FileSnapshot> {
    let original = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            warn!(path = %path.display(), %error, "failed to snapshot migration file");
            return Err(error).with_context(|| format!("failed to snapshot {}", path.display()));
        }
    };
    Ok(FileSnapshot {
        path: path.to_path_buf(),
        original,
    })
}

/// Restore every snapshot, best-effort. Failures are logged, not propagated:
/// rollback already runs in an error path.
fn restore_snapshots(snapshots: &[FileSnapshot]) {
    for snapshot in snapshots {
        let result = snapshot.original.as_ref().map_or_else(
            || fs::remove_file(&snapshot.path),
            |bytes| fs::write(&snapshot.path, bytes),
        );
        if let Err(error) = result {
            warn!(
                path = %snapshot.path.display(),
                %error,
                "rollback failed to restore snapshot"
            );
        }
    }
}

/// Write all planned changes to disk.
fn apply_migration(plan: &MigrationPlan) -> Result<()> {
    fs::copy(&plan.replace_with, &plan.module_dest).with_context(|| {
        format!(
            "failed to copy replacement module to {}",
            plan.module_dest.display()
        )
    })?;
    if let Some(content) = &plan.entry_new_content {
        fs::write(&plan.entry_point, content)
            .with_context(|| format!("failed to update {}", plan.entry_point.display()))?;
    }
    for rewrite in &plan.rewrites {
        fs::write(&rewrite.path, &rewrite.new_content)
            .with_context(|| format!("failed to rewrite {}", rewrite.path.display()))?;
    }
    fs::write(&plan.manifest_path, &plan.edited_toml)
        .with_context(|| format!("failed to update {}", plan.manifest_path.display()))?;
    Ok(())
}

/// Run `cargo check` in `project_dir`, mirroring the spawn/poll/timeout
/// pattern of [`crate::replacement::validator::Validator`]. Returns whether
/// the check passed and a summary of stderr.
fn run_cargo_check(project_dir: &Path) -> Result<(bool, String)> {
    let start = Instant::now();
    let mut child = Command::new("cargo")
        .arg("check")
        .current_dir(project_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn cargo check")?;

    let mut timed_out = false;
    while child
        .try_wait()
        .context("failed to wait for cargo check")?
        .is_none()
    {
        if start.elapsed() >= CARGO_CHECK_TIMEOUT {
            if let Err(error) = child.kill() {
                warn!(project = %project_dir.display(), %error, "failed to kill timed-out cargo check");
            }
            timed_out = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    if timed_out {
        if let Err(error) = child.wait() {
            warn!(project = %project_dir.display(), %error, "failed to reap timed-out cargo check");
        }
        return Ok((
            false,
            format!("cargo check timed out after {CARGO_CHECK_TIMEOUT:?}"),
        ));
    }

    let output = child
        .wait_with_output()
        .context("failed to read cargo check output")?;
    let stderr = summarize_stderr(&String::from_utf8_lossy(&output.stderr));
    Ok((output.status.success(), stderr))
}

/// Print the planned changes without writing anything.
fn print_dry_run(plan: &MigrationPlan) {
    println!(
        "{} migrating '{}' (no files written)",
        "Dry run:".yellow().bold(),
        plan.crate_name
    );
    println!(
        "  would remove '{}' from {} in {}",
        plan.crate_name,
        plan.sections.join(", "),
        display_path(&plan.manifest_path, &plan.project_dir)
    );
    println!(
        "  would copy {} to {}",
        plan.replace_with.display(),
        display_path(&plan.module_dest, &plan.project_dir)
    );
    if plan.mod_decl_added {
        println!(
            "  would declare 'mod {};' in {}",
            plan.module_name,
            display_path(&plan.entry_point, &plan.project_dir)
        );
    } else {
        println!(
            "  module declaration already present in {}",
            display_path(&plan.entry_point, &plan.project_dir)
        );
    }
    if plan.entry_changed_lines > 0 {
        println!(
            "  would rewrite {} line(s) in {}",
            plan.entry_changed_lines,
            display_path(&plan.entry_point, &plan.project_dir)
        );
    }
    for rewrite in &plan.rewrites {
        println!(
            "  would rewrite {} line(s) in {}",
            rewrite.changed_lines,
            display_path(&rewrite.path, &plan.project_dir)
        );
    }
    println!("  would run cargo check");
    println!("Run without --dry-run to apply these changes.");
}

/// Print a concise summary of an applied migration.
fn print_summary(plan: &MigrationPlan) {
    let changed_lines: usize =
        plan.entry_changed_lines + plan.rewrites.iter().map(|r| r.changed_lines).sum::<usize>();
    let changed_files = usize::from(plan.entry_changed_lines > 0) + plan.rewrites.len();
    println!(
        "{} migrated '{}' to module '{}'",
        "✓".green().bold(),
        plan.crate_name,
        plan.module_name
    );
    println!(
        "  removed '{}' from {} in Cargo.toml",
        plan.crate_name,
        plan.sections.join(", ")
    );
    println!(
        "  copied {} to {}",
        plan.replace_with.display(),
        display_path(&plan.module_dest, &plan.project_dir)
    );
    if plan.mod_decl_added {
        println!(
            "  declared 'mod {};' in {}",
            plan.module_name,
            display_path(&plan.entry_point, &plan.project_dir)
        );
    }
    println!("  rewrote {changed_lines} line(s) across {changed_files} file(s) under src/");
    println!("  cargo check passed");
}

/// Display `path` relative to `project_dir` when possible.
fn display_path<'a>(path: &'a Path, project_dir: &Path) -> std::path::Display<'a> {
    path.strip_prefix(project_dir).unwrap_or(path).display()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_line_rewrites_use_path() {
        assert_eq!(
            rewrite_line("use anyhow::{Context, Result};", "anyhow", "amber_anyhow"),
            "use crate::amber_anyhow::{Context, Result};"
        );
    }

    #[test]
    fn rewrite_line_rewrites_qualified_path() {
        assert_eq!(
            rewrite_line(
                "let error = anyhow::anyhow(\"boom\");",
                "anyhow",
                "amber_anyhow"
            ),
            "let error = crate::amber_anyhow::anyhow(\"boom\");"
        );
    }

    #[test]
    fn rewrite_line_skips_comment_lines() {
        let line = "    // anyhow::bail!(\"stay\") is a comment";
        assert_eq!(rewrite_line(line, "anyhow", "amber_anyhow"), line);
    }

    #[test]
    fn rewrite_line_handles_bare_and_aliased_use() {
        assert_eq!(
            rewrite_line("use anyhow;", "anyhow", "amber_anyhow"),
            "use crate::amber_anyhow;"
        );
        assert_eq!(
            rewrite_line("use anyhow as ah;", "anyhow", "amber_anyhow"),
            "use crate::amber_anyhow as ah;"
        );
    }

    #[test]
    fn rewrite_line_maps_hyphenated_crate_paths() {
        assert_eq!(
            rewrite_line(
                "use tracing_subscriber::fmt;",
                "tracing_subscriber",
                "amber_tracing_subscriber"
            ),
            "use crate::amber_tracing_subscriber::fmt;"
        );
    }

    #[test]
    fn rewrite_line_leaves_similar_crate_names_alone() {
        let line = "use anyhow_extra::thing;";
        assert_eq!(rewrite_line(line, "anyhow", "amber_anyhow"), line);
    }

    #[test]
    fn rewrite_source_counts_changes_and_preserves_lines() {
        let content = "use anyhow::Result;\n// anyhow:: stays\nfn main() {}\n";
        let (output, changed) = rewrite_source(content, "anyhow", "amber_anyhow");
        assert_eq!(changed, 1);
        assert_eq!(
            output,
            "use crate::amber_anyhow::Result;\n// anyhow:: stays\nfn main() {}\n"
        );
    }

    #[test]
    fn rewrite_source_preserves_missing_trailing_newline() {
        let (output, changed) = rewrite_source("use anyhow::Result;", "anyhow", "amber_anyhow");
        assert_eq!(changed, 1);
        assert_eq!(output, "use crate::amber_anyhow::Result;");
    }

    #[test]
    fn remove_dependency_removes_from_both_sections() {
        let toml = "\
[package]
name = \"x\"

[dependencies]
anyhow = \"1.0\"
serde = \"1\"

[dev-dependencies]
anyhow = \"1.0\"
";
        let edit = remove_dependency(toml, "anyhow").unwrap().unwrap();
        assert_eq!(edit.sections, vec!["dependencies", "dev-dependencies"]);
        assert!(!edit.content.contains("anyhow"));
        assert!(edit.content.contains("serde = \"1\""));
        assert!(edit.content.contains("[dev-dependencies]"));
    }

    #[test]
    fn remove_dependency_handles_inline_table_form() {
        let toml =
            "[dependencies]\nanyhow = { version = \"1.0\", features = [\"std\"] }\nserde = \"1\"\n";
        let edit = remove_dependency(toml, "anyhow").unwrap().unwrap();
        assert_eq!(edit.sections, vec!["dependencies"]);
        assert!(!edit.content.contains("anyhow"));
        assert!(edit.content.contains("serde = \"1\""));
    }

    #[test]
    fn remove_dependency_returns_none_when_absent() {
        let toml = "[dependencies]\nserde = \"1\"\n";
        assert!(remove_dependency(toml, "anyhow").unwrap().is_none());
    }

    #[test]
    fn has_mod_decl_detects_declarations() {
        assert!(has_mod_decl("mod amber_anyhow;\n", "amber_anyhow"));
        assert!(has_mod_decl("pub mod amber_anyhow;\n", "amber_anyhow"));
        assert!(has_mod_decl(
            "pub(crate) mod amber_anyhow;\n",
            "amber_anyhow"
        ));
        assert!(!has_mod_decl("// mod amber_anyhow;\n", "amber_anyhow"));
        assert!(!has_mod_decl("mod other;\n", "amber_anyhow"));
    }

    #[test]
    fn ensure_mod_decl_appends_only_when_missing() {
        let updated = ensure_mod_decl("fn main() {}\n", "amber_anyhow").unwrap();
        assert!(updated.ends_with("mod amber_anyhow;\n"));
        assert!(ensure_mod_decl(&updated, "amber_anyhow").is_none());
    }

    #[test]
    fn find_entry_point_prefers_lib_rs() {
        let temp = crate::temp::tempdir().unwrap();
        let src = temp.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "").unwrap();
        fs::write(src.join("main.rs"), "").unwrap();
        assert_eq!(find_entry_point(&src).unwrap(), src.join("lib.rs"));
    }

    #[test]
    fn find_entry_point_falls_back_to_main_rs() {
        let temp = crate::temp::tempdir().unwrap();
        let src = temp.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.rs"), "").unwrap();
        assert_eq!(find_entry_point(&src).unwrap(), src.join("main.rs"));
    }

    #[test]
    fn find_entry_point_errors_without_entry() {
        let temp = crate::temp::tempdir().unwrap();
        assert!(find_entry_point(temp.path()).is_err());
    }

    #[test]
    fn snapshot_roundtrip_restores_and_deletes() {
        let temp = crate::temp::tempdir().unwrap();
        let existing = temp.path().join("existing.txt");
        fs::write(&existing, b"original").unwrap();
        let created = temp.path().join("created.txt");

        let snapshots = vec![
            snapshot_file(&existing).unwrap(),
            snapshot_file(&created).unwrap(),
        ];
        fs::write(&existing, b"modified").unwrap();
        fs::write(&created, b"new").unwrap();

        restore_snapshots(&snapshots);
        assert_eq!(fs::read(&existing).unwrap(), b"original");
        assert!(!created.exists());
    }

    #[test]
    fn plan_combines_entry_rewrite_and_mod_decl() {
        let temp = crate::temp::tempdir().unwrap();
        let project = temp.path();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n\n[dependencies]\nanyhow = \"1.0\"\n",
        )
        .unwrap();
        let src = project.join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.rs"), "use anyhow::Result;\n\nfn main() {}\n").unwrap();
        let replacement = project.join("amber_anyhow.rs");
        fs::write(
            &replacement,
            "pub type Result<T> = std::result::Result<T, ()>;\n",
        )
        .unwrap();

        let plan = plan_migration(&project.join("Cargo.toml"), "anyhow", &replacement).unwrap();
        let entry = plan.entry_new_content.unwrap();
        assert!(
            entry.contains("use crate::amber_anyhow::Result;"),
            "entry imports rewritten: {entry}"
        );
        assert!(
            entry.ends_with("mod amber_anyhow;\n"),
            "mod declaration appended: {entry}"
        );
        assert!(plan.mod_decl_added);
        assert_eq!(plan.entry_changed_lines, 1);
        assert!(
            plan.rewrites.is_empty(),
            "entry point must not appear in the general rewrite list"
        );
    }
}
