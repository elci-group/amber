//! Validation of user-supplied output paths.
//!
//! The `replace`, `directives`, and `analyze` commands write generated files
//! to user-controlled locations. Every destination is resolved against the
//! target project root (`cli.path`) and rejected when it escapes that root,
//! so `..` segments or absolute paths cannot make Amber write outside the
//! analyzed project.

use crate::amber_anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Resolve `output` against `project_root` and ensure it stays inside it.
///
/// Relative paths are interpreted relative to the project root. The nearest
/// existing ancestor of the destination is canonicalized and the trailing
/// (not-yet-created) components are re-joined, so symlinks and `..` segments
/// are resolved before the containment check. The returned path is the
/// resolved destination and is what callers must write to.
///
/// # Errors
///
/// Returns an error if the project root cannot be canonicalized or the
/// resolved destination escapes it.
pub fn validate_output_path(project_root: &Path, output: &Path) -> Result<PathBuf> {
    let root = project_root
        .canonicalize()
        .with_context(|| format!("failed to resolve project root {}", project_root.display()))?;

    let candidate = if output.is_absolute() {
        output.to_path_buf()
    } else {
        root.join(output)
    };
    let resolved = canonicalize_nearest_existing(&candidate)?;

    if !resolved.starts_with(&root) {
        crate::bail!(
            "output path {} escapes the project root {}",
            output.display(),
            root.display()
        );
    }
    Ok(resolved)
}

/// Canonicalize the nearest existing ancestor of `path`, then re-join the
/// trailing components that do not exist yet.
fn canonicalize_nearest_existing(path: &Path) -> Result<PathBuf> {
    let mut ancestor = path.to_path_buf();
    let mut remainder = PathBuf::new();
    loop {
        if let Ok(canonical) = ancestor.canonicalize() {
            return Ok(canonical.join(&remainder));
        }
        let Some(name) = ancestor.file_name() else {
            crate::bail!("output path {} has no existing ancestor", path.display());
        };
        remainder = if remainder.as_os_str().is_empty() {
            PathBuf::from(name)
        } else {
            Path::new(name).join(&remainder)
        };
        ancestor.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_output_path_rejects_parent_escape() {
        let project = crate::temp::tempdir().unwrap();
        let result = validate_output_path(project.path(), Path::new("../escape.rs"));
        let err = result.expect_err("a parent-directory escape must be rejected");
        assert!(err.to_string().contains("../escape.rs"));
    }

    #[test]
    fn validate_output_path_rejects_absolute_outside_project() {
        let project = crate::temp::tempdir().unwrap();
        let outside = crate::temp::tempdir().unwrap();
        let target = outside.path().join("report.json");
        let result = validate_output_path(project.path(), &target);
        let err = result.expect_err("an absolute path outside the project must be rejected");
        assert!(err.to_string().contains("report.json"));
    }

    #[test]
    fn validate_output_path_accepts_nested_inside_project() {
        let project = crate::temp::tempdir().unwrap();
        let resolved =
            validate_output_path(project.path(), Path::new("nested/deep/report.json")).unwrap();
        let root = project.path().canonicalize().unwrap();
        assert_eq!(resolved, root.join("nested/deep/report.json"));
    }

    #[test]
    fn validate_output_path_accepts_default_relative_dir() {
        let project = crate::temp::tempdir().unwrap();
        let resolved = validate_output_path(project.path(), Path::new("amber_out")).unwrap();
        let root = project.path().canonicalize().unwrap();
        assert_eq!(resolved, root.join("amber_out"));
    }
}
