use crate::amber_anyhow::{Context, Result};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::temp::TempDir;
use tracing::{debug, info, instrument, warn};

/// Maximum time to wait for any single validation stage.
const VALIDATION_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum stderr lines stored in a [`StageResult`] summary.
const STDERR_SUMMARY_LINES: usize = 50;

/// Stage of validation run against a replacement module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationStage {
    /// `cargo check`
    Check,
    /// `cargo test`
    Test,
    /// `rustfmt --check`
    Format,
}

impl ValidationStage {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Check => "cargo check",
            Self::Test => "cargo test",
            Self::Format => "rustfmt --check",
        }
    }

    #[must_use]
    pub const fn is_required(&self) -> bool {
        match self {
            Self::Check | Self::Test => true,
            Self::Format => false,
        }
    }
}

/// Result of running a single validation stage.
#[derive(Debug, Clone)]
pub struct StageResult {
    pub stage: ValidationStage,
    pub passed: bool,
    pub stderr: String,
    pub duration: Duration,
}

/// Structured report produced by validating a replacement module.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub success: bool,
    pub stages: Vec<StageResult>,
    pub stderr_summary: String,
    pub duration: Duration,
}

impl ValidationReport {
    /// Return the first failing stage, if any.
    #[must_use]
    pub fn first_failure(&self) -> Option<&StageResult> {
        self.stages.iter().find(|s| !s.passed)
    }
}

/// Validates that a generated replacement module compiles and is well-formed.
pub struct Validator;

impl Validator {
    /// Create a new validator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Validate that `replacement_code` compiles as a standalone Rust module.
    ///
    /// A temporary Cargo project is created, the code is written as a module,
    /// and `cargo check`, `cargo test`, and (if available) `rustfmt --check` are
    /// run. The returned [`ValidationReport`] describes the outcome of every
    /// stage. A stage that fails does not prevent later stages from running,
    /// but a required failure makes `report.success == false`.
    ///
    /// # Errors
    ///
    /// Returns an error only if the temporary project cannot be created, the
    /// module cannot be written, or a stage cannot be executed (e.g. timeout).
    #[instrument(skip(self, replacement_code), fields(module_name = %module_name))]
    pub fn validate(&self, module_name: &str, replacement_code: &str) -> Result<ValidationReport> {
        Self::validate_module_name(module_name)?;
        info!("Validating replacement module: {}", module_name);

        let temp = TempDir::new().context("failed to create temporary directory")?;
        let project_dir = temp.path();
        debug!("Validation temp directory: {}", project_dir.display());

        Self::write_cargo_toml(project_dir, module_name)?;
        Self::write_module(project_dir, module_name, replacement_code)?;

        let start = Instant::now();
        let stages = vec![
            Self::run_stage(project_dir, ValidationStage::Check)?,
            // cargo test is only meaningful if compilation already succeeded, but
            // running it regardless lets us capture any runtime/doctest failures.
            Self::run_stage(project_dir, ValidationStage::Test)?,
            Self::run_stage(project_dir, ValidationStage::Format)?,
        ];

        let duration = start.elapsed();
        let success = stages.iter().all(|s| s.passed || !s.stage.is_required());

        let stderr_summary = stages
            .iter()
            .filter(|s| !s.passed)
            .map(|s| format!("{} failed:\n{}", s.stage.name(), s.stderr))
            .collect::<Vec<_>>()
            .join("\n---\n");

        if success {
            info!(
                "Replacement module {} passed validation in {:?}",
                module_name, duration
            );
        } else {
            warn!(
                "Replacement module {} failed validation in {:?}",
                module_name, duration
            );
        }

        Ok(ValidationReport {
            success,
            stages,
            stderr_summary,
            duration,
        })
    }

    fn run_stage(project_dir: &Path, stage: ValidationStage) -> Result<StageResult> {
        debug!("Running {}", stage.name());
        let start = Instant::now();

        let (program, args) = match stage {
            ValidationStage::Check => ("cargo", vec!["check"]),
            ValidationStage::Test => ("cargo", vec!["test"]),
            ValidationStage::Format => ("rustfmt", vec!["--check"]),
        };

        let mut child = Command::new(program)
            .args(&args)
            .current_dir(project_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to spawn {program}"))?;

        let mut killed_for_timeout = false;
        let wait_start = Instant::now();
        while child
            .try_wait()
            .with_context(|| format!("failed to wait for {program}"))?
            .is_none()
        {
            if wait_start.elapsed() >= VALIDATION_TIMEOUT {
                let _ = child.kill();
                killed_for_timeout = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        if killed_for_timeout {
            return Ok(StageResult {
                stage,
                passed: false,
                stderr: format!("{program} timed out after {VALIDATION_TIMEOUT:?}"),
                duration: start.elapsed(),
            });
        }

        let output = child
            .wait_with_output()
            .with_context(|| format!("failed to read {program} output"))?;

        let stderr_full = String::from_utf8_lossy(&output.stderr).to_string();
        let stderr = summarize_stderr(&stderr_full);

        let passed = output.status.success();
        let duration = start.elapsed();

        // If rustfmt is not installed, treat the stage as passed (soft check).
        let passed = if !passed && stage == ValidationStage::Format {
            let not_installed = stderr_full.to_lowercase().contains("command not found")
                || stderr_full
                    .to_lowercase()
                    .contains("no such file or directory");
            if not_installed {
                debug!("rustfmt not installed; skipping format stage");
                true
            } else {
                false
            }
        } else {
            passed
        };

        Ok(StageResult {
            stage,
            passed,
            stderr,
            duration,
        })
    }

    /// Validate that `module_name` is safe to use as a file name and Cargo
    /// package identifier.
    fn validate_module_name(module_name: &str) -> Result<()> {
        let mut chars = module_name.chars();
        let first = chars
            .next()
            .ok_or_else(|| crate::anyhow!("module name must not be empty"))?;
        if !(first.is_ascii_alphabetic() || first == '_') {
            crate::bail!("module name '{module_name}' must start with a letter or underscore");
        }
        if !module_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            crate::bail!(
                "module name '{module_name}' contains invalid characters; only alphanumeric and '_' are allowed"
            );
        }
        Ok(())
    }

    fn write_cargo_toml(project_dir: &Path, module_name: &str) -> Result<()> {
        let cargo_toml = format!(
            r#"[package]
name = "{module_name}"
version = "0.1.0"
edition = "2021"
"#
        );
        fs::write(project_dir.join("Cargo.toml"), cargo_toml)?;
        Ok(())
    }

    fn write_module(project_dir: &Path, module_name: &str, replacement_code: &str) -> Result<()> {
        let src_dir = project_dir.join("src");
        fs::create_dir_all(&src_dir)?;

        let module_file = format!("{module_name}.rs");
        fs::write(src_dir.join(&module_file), replacement_code)?;

        let lib_rs = format!(
            r"mod {module_name};
pub use {module_name}::*;
"
        );
        fs::write(src_dir.join("lib.rs"), lib_rs)?;
        Ok(())
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

fn summarize_stderr(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().collect();
    if lines.len() <= STDERR_SUMMARY_LINES {
        stderr.to_string()
    } else {
        let mut summary = lines[..STDERR_SUMMARY_LINES].join("\n");
        let _ = write!(
            summary,
            "\n... {} more lines truncated",
            lines.len() - STDERR_SUMMARY_LINES
        );
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_module_passes_validation() {
        let validator = Validator::new();
        let code = r"pub fn answer() -> i32 { 42 }";
        let report = validator.validate("valid_mod", code).unwrap();
        assert!(report.success, "expected valid module to pass: {report:?}");
        assert!(!report.stages.is_empty());
    }

    #[test]
    fn invalid_module_fails_validation() {
        let validator = Validator::new();
        let code = r"pub fn broken( { }";
        let report = validator.validate("invalid_mod", code).unwrap();
        assert!(!report.success);
        assert!(report.first_failure().is_some());
    }

    #[test]
    fn module_name_with_separator_is_rejected() {
        let validator = Validator::new();
        assert!(validator.validate("foo/bar", "pub fn x() {}").is_err());
    }

    #[test]
    fn module_name_starting_with_digit_is_rejected() {
        let validator = Validator::new();
        assert!(validator.validate("1foo", "pub fn x() {}").is_err());
    }

    #[test]
    #[allow(clippy::default_constructed_unit_structs)]
    fn default_matches_new() {
        assert_eq!(
            Validator::default()
                .validate("valid_mod", "pub fn x() {}")
                .unwrap()
                .success,
            Validator::new()
                .validate("valid_mod", "pub fn x() {}")
                .unwrap()
                .success
        );
    }
}
