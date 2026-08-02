//! Integration tests for the `amber migrate` subcommand.
//!
//! The whole file is compiled only when the `migrate` Cargo feature is
//! enabled; run with `cargo test --features migrate`. Each test copies the
//! `migrate_project` fixture into a temporary directory so migrations never
//! touch the checked-in fixture. The fixture depends only on `anyhow`, which
//! is pinned by its `Cargo.lock` to a version present in the local cargo
//! cache, so the post-migration `cargo check` runs offline (the migrated
//! project has no remaining external dependencies).
#![cfg(feature = "migrate")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin_path() -> PathBuf {
    option_env!("CARGO_BIN_EXE_amber")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.push("target/debug/amber");
            path
        })
}

fn fixture_dir() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures");
    path.push("migrate_project");
    path
}

fn copy_dir_all(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_all(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn copy_fixture_to_temp() -> amber::temp::TempDir {
    let temp = amber::temp::tempdir().unwrap();
    copy_dir_all(&fixture_dir(), temp.path());
    temp
}

fn run(args: &[&str]) -> (std::process::ExitStatus, String, String) {
    let mut cmd = Command::new(bin_path());
    cmd.args(args).env("NO_COLOR", "1");
    let output = cmd.output().expect("failed to run amber binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status, stdout, stderr)
}

fn run_migrate(
    project: &Path,
    crate_name: &str,
    replace_with: &Path,
    extra: &[&str],
) -> (std::process::ExitStatus, String, String) {
    let mut args = vec![
        project.to_str().unwrap().to_string(),
        "migrate".to_string(),
        crate_name.to_string(),
        "--replace-with".to_string(),
        replace_with.to_str().unwrap().to_string(),
    ];
    args.extend(extra.iter().map(ToString::to_string));
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(&refs)
}

/// Byte snapshots of every file a migration may touch.
fn snapshot_project(project: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    ["Cargo.toml", "Cargo.lock", "src/main.rs", "src/helpers.rs"]
        .iter()
        .map(|rel| {
            let path = project.join(rel);
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
}

#[test]
fn migrate_success_removes_dep_and_rewrites_imports() {
    let temp = copy_fixture_to_temp();
    let project = temp.path();
    let (status, stdout, stderr) =
        run_migrate(project, "anyhow", &project.join("amber_anyhow.rs"), &[]);
    assert!(status.success(), "migrate failed: {stderr}");

    // Dependency removed from both tables, rest of the manifest preserved.
    let manifest = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(!manifest.contains("anyhow"), "dep removed: {manifest}");
    assert!(manifest.contains("[dependencies]"));
    assert!(manifest.contains("[dev-dependencies]"));
    assert!(manifest.contains("name = \"migrate_project\""));

    // Imports and fully-qualified paths rewritten; comment untouched.
    let main = fs::read_to_string(project.join("src/main.rs")).unwrap();
    assert!(
        main.contains("use crate::amber_anyhow::{Context, Result};"),
        "main.rs: {main}"
    );
    assert!(
        main.contains("crate::amber_anyhow::anyhow(format!"),
        "main.rs: {main}"
    );
    assert!(main.contains("mod amber_anyhow;"), "main.rs: {main}");
    assert!(
        main.contains("// This comment mentions anyhow::Context"),
        "comment line must not be rewritten"
    );

    let helpers = fs::read_to_string(project.join("src/helpers.rs")).unwrap();
    assert!(
        helpers.contains("use crate::amber_anyhow::Result;"),
        "helpers.rs: {helpers}"
    );
    assert!(
        helpers.contains("crate::amber_anyhow::Result<&'static str>"),
        "helpers.rs: {helpers}"
    );

    // Module copied into src/ and the lock file no longer mentions anyhow.
    assert!(project.join("src/amber_anyhow.rs").is_file());
    let lock = fs::read_to_string(project.join("Cargo.lock")).unwrap();
    assert!(!lock.contains("name = \"anyhow\""), "lock updated: {lock}");

    // Summary mentions the module and the passing check.
    assert!(stdout.contains("amber_anyhow"), "summary: {stdout}");
    assert!(stdout.contains("cargo check passed"), "summary: {stdout}");
}

#[test]
fn migrate_rollback_on_compile_failure() {
    let temp = copy_fixture_to_temp();
    let project = temp.path();
    let before = snapshot_project(project);

    let (status, _stdout, stderr) = run_migrate(
        project,
        "anyhow",
        &project.join("amber_anyhow_broken.rs"),
        &[],
    );
    assert!(!status.success(), "broken replacement must fail");
    assert!(
        stderr.contains("rolled back"),
        "error must mention rollback: {stderr}"
    );

    for (path, bytes) in before {
        assert_eq!(
            fs::read(&path).unwrap(),
            bytes,
            "{} must be restored byte-identically",
            path.display()
        );
    }
    assert!(
        !project.join("src/amber_anyhow_broken.rs").exists(),
        "copied module must be deleted on rollback"
    );
}

#[test]
fn migrate_dry_run_changes_nothing() {
    let temp = copy_fixture_to_temp();
    let project = temp.path();
    let before = snapshot_project(project);

    let (status, stdout, stderr) = run_migrate(
        project,
        "anyhow",
        &project.join("amber_anyhow.rs"),
        &["--dry-run"],
    );
    assert!(status.success(), "dry run failed: {stderr}");
    assert!(stdout.contains("Dry run"), "stdout: {stdout}");
    assert!(stdout.contains("no files written"), "stdout: {stdout}");

    for (path, bytes) in before {
        assert_eq!(
            fs::read(&path).unwrap(),
            bytes,
            "{} must be unchanged by --dry-run",
            path.display()
        );
    }
    assert!(
        !project.join("src/amber_anyhow.rs").exists(),
        "dry run must not copy the module"
    );
}

#[test]
fn migrate_missing_dependency_errors() {
    let temp = copy_fixture_to_temp();
    let project = temp.path();
    let before = snapshot_project(project);

    let (status, _stdout, stderr) =
        run_migrate(project, "serde", &project.join("amber_anyhow.rs"), &[]);
    assert!(!status.success(), "unknown dependency must fail");
    assert!(
        stderr.contains("not a direct dependency"),
        "error message: {stderr}"
    );

    for (path, bytes) in before {
        assert_eq!(
            fs::read(&path).unwrap(),
            bytes,
            "{} must be unchanged",
            path.display()
        );
    }
    assert!(!project.join("src/amber_anyhow.rs").exists());
}

#[test]
fn migrate_missing_replacement_file_errors() {
    let temp = copy_fixture_to_temp();
    let project = temp.path();
    let (status, _stdout, stderr) =
        run_migrate(project, "anyhow", &project.join("does_not_exist.rs"), &[]);
    assert!(!status.success(), "missing replacement must fail");
    assert!(stderr.contains("does not exist"), "error message: {stderr}");
}
