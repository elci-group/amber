use crate::amber_anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, instrument, warn};

use crate::analysis::types::CrateUsage;
use crate::replacement::templates::{ReplacementTemplate, TemplateOutcome};
use crate::replacement::validator::{ValidationReport, Validator};
use crate::scoring::classifier::ReplacementScore;

#[cfg(feature = "library")]
use crate::library::{EntrySource, LibraryEntry, LibraryStore};

/// Origin of a generated replacement proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalSource {
    /// Hand-written template in `src/replacement/template_data/`.
    DedicatedTemplate,
    /// Generated from usage data (e.g. `itertools`).
    UsageDriven,
    /// Loaded from the Padagonia replacement library.
    Library,
}

impl std::fmt::Display for ProposalSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DedicatedTemplate => write!(f, "dedicated template"),
            Self::UsageDriven => write!(f, "usage-driven"),
            Self::Library => write!(f, "library"),
        }
    }
}

/// A generated replacement proposal
#[derive(Debug, Clone)]
pub struct ReplacementProposal {
    pub original_crate: String,
    pub replacement_module: String,
    pub replacement_code: String,
    pub estimated_compile_time_reduction: String,
    pub estimated_binary_size_reduction: String,
    pub validation_strategy: Vec<String>,
    pub risk_notes: Vec<String>,
    pub test_plan: Vec<String>,
    pub validation_report: Option<ValidationReport>,
    pub source: Option<ProposalSource>,
}

/// Generates replacement implementations for dependencies
pub struct Generator {
    output_dir: PathBuf,
}

impl Generator {
    #[must_use]
    pub const fn new(output_dir: PathBuf) -> Self {
        Self { output_dir }
    }

    /// Return the directory where replacement modules are written.
    #[must_use]
    pub fn output_dir(&self) -> &std::path::Path {
        &self.output_dir
    }

    /// Validate that `crate_name` only contains characters safe for use in
    /// filesystem paths and Cargo identifiers.
    ///
    /// # Errors
    ///
    /// Returns an error if the crate name is empty, starts with an invalid
    /// character, or contains characters other than ASCII alphanumeric, `-`,
    /// or `_`.
    pub fn validate_crate_name(crate_name: &str) -> Result<()> {
        let mut chars = crate_name.chars();
        let first = chars
            .next()
            .ok_or_else(|| crate::anyhow!("crate name must not be empty"))?;
        if !first.is_ascii_alphanumeric() {
            crate::bail!("crate name '{crate_name}' must start with an alphanumeric character");
        }
        if !crate_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            crate::bail!(
                "crate name '{crate_name}' contains invalid characters; only alphanumeric, '-', and '_' are allowed"
            );
        }
        Ok(())
    }

    /// Generate a replacement proposal for a dependency.
    ///
    /// # Errors
    ///
    /// Returns an error if the crate is unsupported, the replacement module
    /// cannot be written to disk, or validation fails.
    #[instrument(skip(self, usage, score), fields(crate_name = %crate_name))]
    pub fn generate_replacement(
        &self,
        crate_name: &str,
        usage: &CrateUsage,
        score: &ReplacementScore,
    ) -> Result<ReplacementProposal> {
        Self::validate_crate_name(crate_name)?;
        info!(
            score = score.overall,
            "Generating replacement for {}", crate_name
        );

        let template = ReplacementTemplate::for_crate(crate_name, usage);
        let (replacement_code, source) = match template.generate_code() {
            TemplateOutcome::Dedicated(code) => (code, ProposalSource::DedicatedTemplate),
            TemplateOutcome::UsageDriven(code) => (code, ProposalSource::UsageDriven),
            TemplateOutcome::Unsupported { reason } => {
                warn!(reason = %reason, "Skipping unsupported crate {}", crate_name);
                crate::bail!("unsupported crate '{}': {reason}", crate_name);
            }
        };

        let replacement_module = format!("amber_{}_redux", crate_name.replace('-', "_"));

        // Write the replacement module to disk
        let module_path = self.output_dir.join(format!("{replacement_module}.rs"));
        fs::create_dir_all(&self.output_dir)?;
        fs::write(&module_path, &replacement_code)
            .with_context(|| format!("Failed to write replacement to {}", module_path.display()))?;

        info!(path = %module_path.display(), "Wrote replacement module");

        let report = Validator::new().validate(&replacement_module, &replacement_code)?;
        if !report.success {
            warn!(
                crate = %crate_name,
                "Validation failed for {}; removing partial output", crate_name
            );
            if let Err(e) = fs::remove_file(&module_path) {
                debug!(
                    path = %module_path.display(),
                    error = %e,
                    "Failed to remove partial replacement file"
                );
            }
            crate::bail!(
                "replacement module for '{}' failed validation:\n{}",
                crate_name,
                report.stderr_summary
            );
        }

        info!("Replacement for {} passed validation", crate_name);

        Ok(ReplacementProposal {
            original_crate: crate_name.to_string(),
            replacement_module,
            replacement_code,
            estimated_compile_time_reduction: Self::estimate_compile_reduction(crate_name),
            estimated_binary_size_reduction: Self::estimate_binary_reduction(crate_name),
            validation_strategy: Self::build_validation_strategy(crate_name, usage),
            risk_notes: score.reasoning.clone(),
            test_plan: Self::build_test_plan(crate_name, usage),
            validation_report: Some(report),
            source: Some(source),
        })
    }

    /// Generate a replacement proposal, consulting the library first.
    ///
    /// If a module for `crate_name` already exists in `library`, it is returned
    /// directly. Otherwise a new module is generated, validated, stored in the
    /// library, and returned.
    ///
    /// # Errors
    ///
    /// Returns an error if the replacement module cannot be written, the
    /// library cannot be persisted, or validation fails.
    #[cfg(feature = "library")]
    pub fn generate_replacement_with_library(
        &self,
        crate_name: &str,
        usage: &CrateUsage,
        score: &ReplacementScore,
        library: &mut LibraryStore,
    ) -> Result<ReplacementProposal> {
        if let Some(entry) = library.find(crate_name) {
            info!("Loaded replacement for {crate_name} from library");
            return Ok(ReplacementProposal {
                original_crate: crate_name.to_string(),
                replacement_module: entry.module_name,
                replacement_code: entry.code,
                estimated_compile_time_reduction: Self::estimate_compile_reduction(crate_name),
                estimated_binary_size_reduction: Self::estimate_binary_reduction(crate_name),
                validation_strategy: Self::build_validation_strategy(crate_name, usage),
                risk_notes: score.reasoning.clone(),
                test_plan: Self::build_test_plan(crate_name, usage),
                validation_report: None,
                source: Some(ProposalSource::Library),
            });
        }

        let proposal = self.generate_replacement(crate_name, usage, score)?;
        let entry = LibraryEntry::new(
            crate_name.to_string(),
            proposal.replacement_module.clone(),
            proposal.replacement_code.clone(),
            EntrySource::Generated,
        );
        library.insert(&entry)?;
        Ok(proposal)
    }

    #[must_use]
    pub fn estimate_compile_reduction(crate_name: &str) -> String {
        // Rough estimates based on known compile times
        let reduction = match crate_name {
            "syn" => "-15-25%",
            "regex" => "-5-10%",
            "chrono" | "rand" => "-3-8%",
            "serde" | "clap" => "-5-15%",
            "reqwest" | "tokio" => "-10-20%",
            "lazy_static" | "once_cell" | "log" | "env_logger" => "-1-3%",
            "itertools" | "anyhow" | "thiserror" | "uuid" => "-2-5%",
            "colored" | "owo-colors" => "-1-2%",
            _ => "-2-10% (estimated)",
        };
        reduction.to_string()
    }

    #[must_use]
    pub fn estimate_binary_reduction(crate_name: &str) -> String {
        let reduction = match crate_name {
            "syn" => "-500KB-1.5MB",
            "regex" => "-200KB-500KB",
            "chrono" | "rand" => "-100KB-300KB",
            "serde" => "-200KB-800KB",
            "reqwest" => "-500KB-2MB",
            "tokio" => "-300KB-1MB",
            "clap" => "-100KB-500KB",
            "lazy_static" | "once_cell" => "-10KB-50KB",
            "itertools" => "-50KB-150KB",
            "colored" | "owo-colors" => "-10KB-30KB",
            "anyhow" | "thiserror" => "-30KB-100KB",
            "uuid" => "-50KB-200KB",
            "log" | "env_logger" => "-20KB-80KB",
            _ => "-50KB-500KB (estimated)",
        };
        reduction.to_string()
    }

    fn build_validation_strategy(_crate_name: &str, usage: &CrateUsage) -> Vec<String> {
        let mut strategies = vec![
            "Run existing test suite".to_string(),
            "Verify no compilation errors".to_string(),
        ];

        if usage.affected_files.len() > 5 {
            strategies.push("Integration testing across all affected modules".to_string());
        }

        if !usage.is_trivial_usage {
            strategies.push("Property-based testing for edge cases".to_string());
            strategies.push("Differential testing against original implementation".to_string());
        }

        if usage.used_in_public_api {
            strategies.push("Check downstream API compatibility".to_string());
        }

        strategies.push("Benchmark comparison (before/after)".to_string());

        strategies
    }

    fn build_test_plan(crate_name: &str, usage: &CrateUsage) -> Vec<String> {
        let mut tests = vec![format!(
            "Test all {} call sites of {}",
            usage.call_sites.len(),
            crate_name
        )];

        for item in &usage.imported_items {
            tests.push(format!(
                "Verify {}::{} behavior matches original",
                crate_name, item.name
            ));
        }

        tests.push("Performance regression test".to_string());
        tests.push("Memory usage comparison".to_string());
        tests.push("Compile time check".to_string());

        tests
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::classifier::{
        ReplacementRecommendation, ReplacementScore, SafetyClass, ScoreDimensions,
    };

    fn dummy_score() -> ReplacementScore {
        ReplacementScore {
            overall: 80,
            confidence: 90,
            classification: SafetyClass::SafeToReplace,
            dimensions: ScoreDimensions::default(),
            reasoning: vec!["test reason".to_string()],
            recommendation: ReplacementRecommendation::Proceed,
        }
    }

    #[test]
    fn generates_replacement_file_and_proposal() {
        let temp = crate::temp::tempdir().unwrap();
        let generator = Generator::new(temp.path().to_path_buf());
        let usage = CrateUsage::default();
        let score = dummy_score();

        let proposal = generator
            .generate_replacement("anyhow", &usage, &score)
            .expect("generate replacement");

        assert!(temp
            .path()
            .join(&proposal.replacement_module)
            .with_extension("rs")
            .exists());
        assert_eq!(proposal.original_crate, "anyhow");
        assert!(!proposal.replacement_code.is_empty());
        assert!(proposal.validation_report.is_some());
        assert!(proposal.validation_report.unwrap().success);
        assert_eq!(proposal.source, Some(ProposalSource::DedicatedTemplate));
    }

    #[test]
    fn unsupported_crate_does_not_create_file() {
        let temp = crate::temp::tempdir().unwrap();
        let generator = Generator::new(temp.path().to_path_buf());
        let usage = CrateUsage::default();
        let score = dummy_score();

        let result = generator.generate_replacement("unknown_crate_xyz", &usage, &score);
        assert!(result.is_err(), "expected unsupported crate to fail");

        let entries: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .collect();
        assert!(
            entries.is_empty(),
            "no files should be written for unsupported crates"
        );
    }

    #[test]
    fn invalid_replacement_is_not_kept() {
        let temp = crate::temp::tempdir().unwrap();
        let generator = Generator::new(temp.path().to_path_buf());
        // "serde" has no template, so the generator fails before validation.
        // To exercise validation failure we would need a crate whose template
        // produces invalid code; we do not have one by design, so we at least
        // verify that no file is left behind when generation fails.
        let _ = generator.generate_replacement("serde", &CrateUsage::default(), &dummy_score());
        let rs_files: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
            .collect();
        assert!(
            rs_files.is_empty(),
            "failed generation should leave no .rs files"
        );
    }

    #[test]
    fn estimates_for_known_crates_are_consistent() {
        let temp = crate::temp::tempdir().unwrap();
        let generator = Generator::new(temp.path().to_path_buf());
        for crate_name in [
            "anyhow",
            "thiserror",
            "lazy_static",
            "colored",
            "owo-colors",
            "log",
            "env_logger",
            "chrono",
            "itertools",
            "ureq",
        ] {
            let proposal = generator
                .generate_replacement(crate_name, &CrateUsage::default(), &dummy_score())
                .unwrap();
            assert!(!proposal.estimated_compile_time_reduction.is_empty());
            assert!(!proposal.estimated_binary_size_reduction.is_empty());
        }
    }

    #[test]
    fn nontrivial_usage_adds_validation_steps() {
        let temp = crate::temp::tempdir().unwrap();
        let generator = Generator::new(temp.path().to_path_buf());
        let usage = CrateUsage {
            affected_files: vec!["a", "b", "c", "d", "e", "f"]
                .into_iter()
                .map(String::from)
                .collect(),
            used_in_public_api: true,
            is_trivial_usage: false,
            ..Default::default()
        };
        let proposal = generator
            .generate_replacement("anyhow", &usage, &dummy_score())
            .unwrap();
        assert!(proposal
            .validation_strategy
            .iter()
            .any(|s| s.contains("Integration testing")));
        assert!(proposal
            .validation_strategy
            .iter()
            .any(|s| s.contains("downstream API compatibility")));
    }
}
