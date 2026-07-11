use crate::amber_anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::analysis::types::{CrateUsage, Dependency};
use crate::replacement::generator::Generator;
use crate::scoring::classifier::{ReplacementRecommendation, ReplacementScore};

/// Estimated impact of replacing a dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstimatedImpact {
    /// Estimated compile-time reduction (human-readable range).
    pub compile_time_reduction: String,
    /// Estimated binary-size reduction (human-readable range).
    pub binary_size_reduction: String,
}

/// A scoped technical directive for replacing a dependency.
///
/// Directives are human/agent-readable instruction documents tailored to the
/// assessed project's actual usage of a single dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalDirective {
    /// Name of the project being analyzed.
    pub project_name: String,
    /// Path to the project's `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// Crate targeted for replacement.
    pub target_crate: String,
    /// Version of the targeted crate.
    pub target_version: String,
    /// Human-readable safety classification (e.g. "Safe to Replace").
    pub safety_classification: String,
    /// Overall replacement safety score (0-100).
    pub safety_score: u8,
    /// Confidence in the assessment (0-100).
    pub confidence: u8,
    /// Human-readable rationale drawn from the score reasoning.
    pub rationale: Vec<String>,
    /// One-paragraph summary of the replacement scope.
    pub scope_summary: String,
    /// Files that reference the targeted crate.
    pub affected_files: Vec<String>,
    /// Whether the crate is used in the project's public API.
    pub used_in_public_api: bool,
    /// Number of transitive dependencies brought in by the crate.
    pub transitive_dependency_count: usize,
    /// Dependency kind ("Normal", "Dev", "Build", etc.).
    pub dependency_kind: String,
    /// Minimum Supported Rust Version, if known.
    pub msrv: Option<String>,
    /// Ordered implementation steps.
    pub implementation_steps: Vec<String>,
    /// Acceptance criteria for the replacement.
    pub acceptance_criteria: Vec<String>,
    /// Testing plan for the replacement.
    pub testing_plan: Vec<String>,
    /// Rollback plan if the replacement fails.
    pub rollback_plan: Vec<String>,
    /// Risk notes from the safety classifier.
    pub risk_notes: Vec<String>,
    /// Estimated compile-time and binary-size impact.
    pub estimated_impact: EstimatedImpact,
}

/// Context used when generating a [`TechnicalDirective`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectiveContext {
    /// Name of the project being analyzed.
    pub project_name: String,
    /// Path to the project's `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// Minimum Supported Rust Version, if known.
    pub msrv: Option<String>,
}

/// Generates scoped technical directives for proposed dependency rewrites.
pub struct DirectiveGenerator;

impl DirectiveGenerator {
    /// Generate a technical directive for a dependency.
    ///
    /// The directive is scoped based on the safety score, recommendation,
    /// usage pattern, public-API exposure, dependency kind, and known
    /// crate-specific guidance.
    #[must_use]
    pub fn generate(
        dep: &Dependency,
        usage: &CrateUsage,
        score: &ReplacementScore,
        context: &DirectiveContext,
    ) -> TechnicalDirective {
        let implementation_steps = Self::build_implementation_steps(dep, usage, score);
        let testing_plan = Self::build_testing_plan(dep, usage, score);
        let acceptance_criteria = Self::build_acceptance_criteria(dep, usage);
        let rollback_plan = Self::build_rollback_plan(dep, usage);
        let rationale = Self::build_rationale(score, usage);
        let scope_summary = Self::build_scope_summary(dep, usage, score);

        TechnicalDirective {
            project_name: context.project_name.clone(),
            manifest_path: context.manifest_path.clone(),
            target_crate: dep.name.clone(),
            target_version: dep.version.clone(),
            safety_classification: score.classification.name().to_string(),
            safety_score: score.overall,
            confidence: score.confidence,
            rationale,
            scope_summary,
            affected_files: usage.affected_files.clone(),
            used_in_public_api: usage.used_in_public_api,
            transitive_dependency_count: dep.total_transitive_count(),
            dependency_kind: format!("{:?}", dep.kind),
            msrv: context.msrv.clone(),
            implementation_steps,
            acceptance_criteria,
            testing_plan,
            rollback_plan,
            risk_notes: score.reasoning.clone(),
            estimated_impact: EstimatedImpact {
                compile_time_reduction: Generator::estimate_compile_reduction(&dep.name),
                binary_size_reduction: Generator::estimate_binary_reduction(&dep.name),
            },
        }
    }

    fn build_rationale(score: &ReplacementScore, usage: &CrateUsage) -> Vec<String> {
        let mut rationale = score.reasoning.clone();

        if usage.imported_items.is_empty() {
            rationale.push(
                "The dependency is declared but has no detected source usage; removal is the safest action."
                    .to_string(),
            );
        } else if usage.is_trivial_usage {
            rationale.push(format!(
                "Usage is trivial ({} call sites, {} unique APIs), making a focused replacement feasible.",
                usage.call_sites.len(),
                usage.unique_api_usage
            ));
        } else {
            rationale.push(format!(
                "Usage is non-trivial ({} call sites, {} unique APIs); replacement requires careful validation.",
                usage.call_sites.len(),
                usage.unique_api_usage
            ));
        }

        rationale.push(format!(
            "Recommended action: {}",
            score.recommendation.description()
        ));

        rationale
    }

    fn build_scope_summary(
        dep: &Dependency,
        usage: &CrateUsage,
        score: &ReplacementScore,
    ) -> String {
        let kind_label = format!("{:?}", dep.kind);
        let api_text = if usage.imported_items.is_empty() {
            "no detected API usage".to_string()
        } else {
            format!(
                "{} unique APIs across {} call sites in {} file(s)",
                usage.unique_api_usage,
                usage.call_sites.len(),
                usage.affected_files.len()
            )
        };

        let public_api_text = if usage.used_in_public_api {
            "appears in the public API"
        } else {
            "does not appear in the public API"
        };

        format!(
            "Replace the {} dependency `{}` v{} ({}). The assessment found {}. It {}. Classification: {} (score: {}/100, confidence: {}/100).",
            kind_label.to_lowercase(),
            dep.name,
            dep.version,
            if dep.total_transitive_count() > 0 {
                format!("brings in {} transitive dependencies", dep.total_transitive_count())
            } else {
                "has no recorded transitive dependencies".to_string()
            },
            api_text,
            public_api_text,
            score.classification.name(),
            score.overall,
            score.confidence
        )
    }

    fn build_implementation_steps(
        dep: &Dependency,
        usage: &CrateUsage,
        score: &ReplacementScore,
    ) -> Vec<String> {
        let mut steps = vec![
            format!("Audit all usage of `{}` across affected files.", dep.name),
            "Review the generated replacement module in `amber_out/` or `amber_proposals/`."
                .to_string(),
        ];

        if usage.imported_items.is_empty() {
            steps.push(format!(
                "Remove `{}` from `Cargo.toml` and run `cargo check`.",
                dep.name
            ));
            return steps;
        }

        steps.extend(Self::crate_specific_steps(&dep.name, usage));

        match score.recommendation {
            ReplacementRecommendation::Proceed => {
                steps.push(format!(
                    "Apply the replacement module and replace `{}` call sites incrementally.",
                    dep.name
                ));
            }
            ReplacementRecommendation::Propose => {
                steps.push(format!(
                    "Draft a replacement PR for `{}` and request human review before merging.",
                    dep.name
                ));
            }
            ReplacementRecommendation::Caution => {
                steps.push(format!(
                    "Prototype the replacement for `{}` behind a feature flag or in a separate branch.",
                    dep.name
                ));
            }
            ReplacementRecommendation::Block | ReplacementRecommendation::SecurityBlock => {
                steps.push(format!(
                    "Do not proceed with replacing `{}` unless the blockers are resolved.",
                    dep.name
                ));
            }
        }

        steps.push("Update `Cargo.toml` to remove or replace the dependency entry.".to_string());
        steps.push("Run `cargo check`, `cargo clippy`, and the full test suite.".to_string());

        steps
    }

    fn crate_specific_steps(crate_name: &str, usage: &CrateUsage) -> Vec<String> {
        match crate_name {
            "anyhow" => vec![
                "Introduce a minimal custom `Error` type wrapping `std::error::Error`.".to_string(),
                "Replace `crate::amber_anyhow::Result<T>` with `std::result::Result<T, CustomError>`.".to_string(),
                "Convert `bail!` and `Context` usages to explicit `Err(...)` or `map_err`.".to_string(),
            ],
            "thiserror" => vec![
                "Replace `#[derive(Error)]` with manual `std::fmt::Display` and `std::error::Error` impls.".to_string(),
                "Expand `#[from]` and `#[source]` into explicit conversion methods.".to_string(),
            ],
            "serde" => vec![
                "Identify all `Serialize`/`Deserialize` derives and manual impls.".to_string(),
                "Replace derives with hand-written `serde::Serialize`/`Deserialize` impls or consider a smaller serialization library.".to_string(),
                "Update attribute macros (`#[serde(...)]`) to match the new implementation.".to_string(),
            ],
            "serde_json" => vec![
                "Replace `serde_json::to_string`/`from_str` calls with targeted JSON helpers or a lighter parser.".to_string(),
                "Audit error handling at every serialization boundary.".to_string(),
            ],
            "chrono" => vec![
                "Replace `DateTime<Utc>` with `std::time::SystemTime` where feasible.".to_string(),
                "Re-implement required formatting/parsing with `std::fmt` and `strptime`-style logic only for used patterns.".to_string(),
            ],
            "regex" => vec![
                "Map each used regex to an equivalent `std::str::pattern` or hand-rolled parser.".to_string(),
                "Add targeted property tests that mirror the original regex semantics.".to_string(),
            ],
            "itertools" => vec![
                "Replace each used `itertools` adapter with standard-library iterators or a small local helper.".to_string(),
                "Pay special attention to `flatten_ok`, `group_by`, and `collect_tuple` replacements.".to_string(),
            ],
            "colored" | "owo-colors" | "yansi" | "ansi_term" => vec![
                "Replace color helpers with ANSI escape sequences or a minimal internal wrapper.".to_string(),
                "Respect `NO_COLOR` and `TERM` environment variables in the wrapper.".to_string(),
            ],
            "lazy_static" => vec![
                "Replace `lazy_static!` with `std::sync::OnceLock` or `static_assertions`-free equivalents.".to_string(),
            ],
            "once_cell" => vec![
                "Replace `once_cell::sync::Lazy` with `std::sync::LazyLock` (Rust 1.80+) or `std::sync::OnceLock`.".to_string(),
                "Replace `once_cell::unsync::Lazy` with local initialization helpers where appropriate.".to_string(),
            ],
            "log" => vec![
                "Introduce a tiny internal logging facade matching the macros actually used.".to_string(),
                "Replace `log::info!`, `log::error!`, etc., with the internal macros.".to_string(),
            ],
            "env_logger" => vec![
                "Remove `env_logger` initialization and replace with `eprintln!`/internal logging based on `RUST_LOG`.".to_string(),
            ],
            "reqwest" => vec![
                "Audit HTTP method/timeout/redirect usage and replace with a minimal HTTP client (e.g., `std::net` + `ureq` if already in tree, or a lightweight internal wrapper).".to_string(),
                "Ensure TLS configuration remains equivalent.".to_string(),
            ],
            "tokio" => vec![
                "Identify runtime features used (spawn, channels, timers, IO).".to_string(),
                "Migrate to `std::thread`/`std::sync` for CPU-bound work or a smaller async runtime if async is still required.".to_string(),
            ],
            "uuid" => vec![
                "Replace UUID generation with `std::time`/`std::sync::atomic` based identifiers or a minimal local v4 implementation.".to_string(),
                "Preserve parsing/serialization behavior for used formats.".to_string(),
            ],
            "tempfile" => vec![
                "Replace `tempfile::TempDir`/`NamedTempFile` with `std::env::temp_dir()` and `std::fs` helpers.".to_string(),
                "Ensure cleanup on drop/panic using a local RAII guard.".to_string(),
            ],
            "clap" => vec![
                "Map parsed CLI arguments to a plain struct.".to_string(),
                "Replace derived `Parser` with manual `std::env::args` parsing for the used flags/subcommands.".to_string(),
            ],
            _ => vec![
                format!(
                    "Audit public API usage of `{}` and provide equivalent local implementations for {} used item(s).",
                    crate_name,
                    usage.unique_api_usage
                ),
                "Write behavioral tests that lock the observed contract before replacing.".to_string(),
            ],
        }
    }

    fn build_acceptance_criteria(dep: &Dependency, usage: &CrateUsage) -> Vec<String> {
        let mut criteria = vec![
            "Project compiles cleanly with `cargo check --all-targets`.".to_string(),
            "All existing tests pass with `cargo test --all-targets`.".to_string(),
        ];

        if usage.used_in_public_api {
            criteria.push(format!(
                "Downstream consumers of `{}` in the public API continue to compile.",
                dep.name
            ));
        }

        if !usage.imported_items.is_empty() {
            criteria.push(format!(
                "Every `{}` call site behaves identically to the original implementation.",
                dep.name
            ));
        }

        criteria.push("No new `cargo clippy` warnings are introduced.".to_string());
        criteria.push("Memory-safety and error-handling paths are preserved.".to_string());

        criteria
    }

    fn build_testing_plan(
        dep: &Dependency,
        usage: &CrateUsage,
        score: &ReplacementScore,
    ) -> Vec<String> {
        let mut tests = vec![
            format!(
                "Run the existing test suite before any `{}` changes.",
                dep.name
            ),
            format!(
                "Test every {} call site(s) of `{}` after replacement.",
                usage.call_sites.len(),
                dep.name
            ),
        ];

        if usage.affected_files.len() > 5 {
            tests.push("Run integration tests across all affected modules.".to_string());
        }

        if !usage.is_trivial_usage {
            tests.push("Add property-based or differential tests for edge cases.".to_string());
        }

        if usage.used_in_public_api {
            tests.push("Verify downstream API compatibility for public-API usage.".to_string());
        }

        if matches!(
            score.recommendation,
            ReplacementRecommendation::Caution
                | ReplacementRecommendation::Block
                | ReplacementRecommendation::SecurityBlock
        ) {
            tests.push("Perform extensive differential testing against the original crate before considering merge.".to_string());
        }

        tests.push("Benchmark compile time and binary size before/after replacement.".to_string());

        tests
    }

    fn build_rollback_plan(dep: &Dependency, usage: &CrateUsage) -> Vec<String> {
        let mut plan = vec![
            format!(
                "Ensure `{}` v{} is pinned or available in `Cargo.lock` before changes.",
                dep.name, dep.version
            ),
            "Create a feature branch or draft PR for the replacement work.".to_string(),
        ];

        if usage.affected_files.len() > 1 {
            plan.push(
                "Replace one module/file at a time so each step is independently revertible."
                    .to_string(),
            );
        }

        plan.extend([
            "Commit after each replacement milestone to enable `git revert`.".to_string(),
            "Keep the original dependency declaration commented during development, removing it only after validation.".to_string(),
            "If regressions are found, restore the original `Cargo.toml` entry and revert source changes.".to_string(),
        ]);

        plan
    }
}

impl TechnicalDirective {
    /// Render the directive as a well-structured Markdown document.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!("# Technical Directive: `{}`", self.target_crate));
        lines.push(String::new());

        lines.push("## Project Context".to_string());
        lines.push(String::new());
        lines.push(format!("- **Project:** `{}`", self.project_name));
        lines.push(format!(
            "- **Manifest:** `{}`",
            self.manifest_path.display()
        ));
        lines.push(format!("- **Target crate:** `{}`", self.target_crate));
        lines.push(format!("- **Dependency kind:** {}", self.dependency_kind));
        if let Some(msrv) = &self.msrv {
            lines.push(format!("- **MSRV:** {msrv}"));
        }
        lines.push(String::new());

        lines.push("## Safety Assessment".to_string());
        lines.push(String::new());
        lines.push(format!(
            "- **Classification:** {}",
            self.safety_classification
        ));
        lines.push(format!("- **Safety score:** {}/100", self.safety_score));
        lines.push(format!("- **Confidence:** {}/100", self.confidence));
        lines.push(String::new());

        lines.push("## Scope Summary".to_string());
        lines.push(String::new());
        lines.push(self.scope_summary.clone());
        lines.push(String::new());

        if !self.affected_files.is_empty() {
            lines.push("### Affected Files".to_string());
            for file in &self.affected_files {
                lines.push(format!("- `{file}`"));
            }
            lines.push(String::new());
        }

        if self.used_in_public_api {
            lines.push(
                "⚠️ This dependency is used in the project's public API. Replacement may be a breaking change."
                    .to_string(),
            );
            lines.push(String::new());
        }

        lines.push("## Rationale".to_string());
        lines.push(String::new());
        for item in &self.rationale {
            lines.push(format!("- {item}"));
        }
        lines.push(String::new());

        lines.push("## Implementation Steps".to_string());
        lines.push(String::new());
        for (idx, step) in self.implementation_steps.iter().enumerate() {
            lines.push(format!("{}. {step}", idx + 1));
        }
        lines.push(String::new());

        lines.push("## Acceptance Criteria".to_string());
        lines.push(String::new());
        for item in &self.acceptance_criteria {
            lines.push(format!("- [ ] {item}"));
        }
        lines.push(String::new());

        lines.push("## Testing Plan".to_string());
        lines.push(String::new());
        for item in &self.testing_plan {
            lines.push(format!("- {item}"));
        }
        lines.push(String::new());

        lines.push("## Rollback Plan".to_string());
        lines.push(String::new());
        for item in &self.rollback_plan {
            lines.push(format!("- {item}"));
        }
        lines.push(String::new());

        lines.push("## Risk Notes".to_string());
        lines.push(String::new());
        for item in &self.risk_notes {
            lines.push(format!("- {item}"));
        }
        lines.push(String::new());

        lines.push("## Estimated Impact".to_string());
        lines.push(String::new());
        lines.push(format!(
            "- **Compile-time reduction:** {}",
            self.estimated_impact.compile_time_reduction
        ));
        lines.push(format!(
            "- **Binary-size reduction:** {}",
            self.estimated_impact.binary_size_reduction
        ));
        lines.push(String::new());

        lines.push("---".to_string());
        lines.push("*Generated by Amber — review before execution.*".to_string());
        lines.push(String::new());

        lines.join("\n")
    }

    /// Serialize the directive to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

impl Default for DirectiveGenerator {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::types::{
        CallSite, DependencyKind, DependencySource, ImportedItem, ItemKind, Location, UsageKind,
    };
    use crate::scoring::classifier::{ReplacementRecommendation, SafetyClass, ScoreDimensions};

    fn sample_dep(name: &str) -> Dependency {
        Dependency {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            source: DependencySource::CratesIo,
            kind: DependencyKind::Normal,
            features: Vec::new(),
            optional: false,
            uses_default_features: true,
            transitive_deps: vec!["a".to_string(), "b".to_string()],
            loc_approx: 1000,
            public_api_count: 20,
            last_release: None,
            maintenance_score: 80,
            cve_count: 0,
            license: Some("MIT".to_string()),
            download_count: 1000,
        }
    }

    fn sample_context() -> DirectiveContext {
        DirectiveContext {
            project_name: "test_project".to_string(),
            manifest_path: PathBuf::from("/tmp/test/Cargo.toml"),
            msrv: Some("1.80".to_string()),
        }
    }

    fn sample_score() -> ReplacementScore {
        ReplacementScore {
            overall: 85,
            confidence: 90,
            classification: SafetyClass::SafeToReplace,
            dimensions: ScoreDimensions::default(),
            reasoning: vec!["Low API surface".to_string()],
            recommendation: ReplacementRecommendation::Proceed,
        }
    }

    fn sample_usage() -> CrateUsage {
        CrateUsage {
            crate_name: "anyhow".to_string(),
            imported_items: vec![ImportedItem {
                name: "Result".to_string(),
                kind: ItemKind::Type,
                path: "anyhow::Result".to_string(),
                location: Location::new("src/lib.rs", 1, 1),
            }],
            import_count: 1,
            call_sites: vec![CallSite {
                function_name: "bail".to_string(),
                kind: UsageKind::MacroInvocation,
                location: Location::new("src/lib.rs", 2, 5),
                context: "fn main()".to_string(),
            }],
            affected_files: vec!["src/lib.rs".to_string()],
            unique_api_usage: 1,
            api_coverage_percent: 5.0,
            used_in_public_api: false,
            used_features: Vec::new(),
            is_trivial_usage: true,
        }
    }

    #[test]
    fn generates_directive_for_trivial_usage() {
        let dep = sample_dep("anyhow");
        let usage = sample_usage();
        let score = sample_score();
        let context = sample_context();

        let directive = DirectiveGenerator::generate(&dep, &usage, &score, &context);

        assert_eq!(directive.target_crate, "anyhow");
        assert_eq!(directive.safety_score, 85);
        assert!(directive.is_trivial_usage_reflected());
        assert!(directive
            .implementation_steps
            .iter()
            .any(|s| s.contains("custom `Error`")));
    }

    #[test]
    fn generates_directive_for_nontrivial_usage() {
        let dep = sample_dep("anyhow");
        let mut usage = sample_usage();
        usage.is_trivial_usage = false;
        usage.call_sites = (0..20)
            .map(|i| CallSite {
                function_name: format!("call_{i}"),
                kind: UsageKind::FunctionCall,
                location: Location::new("src/lib.rs", i, 1),
                context: "fn main()".to_string(),
            })
            .collect();
        usage.affected_files = (0..10).map(|i| format!("src/{i}.rs")).collect();
        let score = sample_score();
        let context = sample_context();

        let directive = DirectiveGenerator::generate(&dep, &usage, &score, &context);

        assert!(!directive.is_trivial_usage_reflected());
        assert!(directive
            .testing_plan
            .iter()
            .any(|s| s.contains("integration tests")));
        assert!(directive
            .rollback_plan
            .iter()
            .any(|s| s.contains("one module/file at a time")));
    }

    #[test]
    fn public_api_usage_adds_criteria_and_warning() {
        let dep = sample_dep("anyhow");
        let mut usage = sample_usage();
        usage.used_in_public_api = true;
        let score = sample_score();
        let context = sample_context();

        let directive = DirectiveGenerator::generate(&dep, &usage, &score, &context);

        assert!(directive.used_in_public_api);
        assert!(directive
            .acceptance_criteria
            .iter()
            .any(|s| s.contains("Downstream consumers")));
        let markdown = directive.to_markdown();
        assert!(markdown.contains("public API"));
    }

    #[test]
    fn known_crate_guidance_includes_specific_steps() {
        let dep = sample_dep("chrono");
        let usage = sample_usage();
        let score = sample_score();
        let context = sample_context();

        let directive = DirectiveGenerator::generate(&dep, &usage, &score, &context);

        assert!(directive
            .implementation_steps
            .iter()
            .any(|s| s.contains("SystemTime")));
    }

    #[test]
    fn unknown_crate_uses_generic_guidance() {
        let dep = sample_dep("unknown_crate");
        let usage = sample_usage();
        let score = sample_score();
        let context = sample_context();

        let directive = DirectiveGenerator::generate(&dep, &usage, &score, &context);

        assert!(directive
            .implementation_steps
            .iter()
            .any(|s| s.contains("Audit public API usage")));
    }

    #[test]
    fn markdown_contains_expected_sections() {
        let dep = sample_dep("anyhow");
        let usage = sample_usage();
        let score = sample_score();
        let context = sample_context();

        let directive = DirectiveGenerator::generate(&dep, &usage, &score, &context);
        let markdown = directive.to_markdown();

        assert!(markdown.contains("# Technical Directive: `anyhow`"));
        assert!(markdown.contains("## Project Context"));
        assert!(markdown.contains("## Safety Assessment"));
        assert!(markdown.contains("## Scope Summary"));
        assert!(markdown.contains("## Rationale"));
        assert!(markdown.contains("## Implementation Steps"));
        assert!(markdown.contains("## Acceptance Criteria"));
        assert!(markdown.contains("## Testing Plan"));
        assert!(markdown.contains("## Rollback Plan"));
        assert!(markdown.contains("## Risk Notes"));
        assert!(markdown.contains("## Estimated Impact"));
        assert!(markdown.contains("Target crate"));
    }

    #[test]
    fn json_serialization_round_trips() {
        let dep = sample_dep("anyhow");
        let usage = sample_usage();
        let score = sample_score();
        let context = sample_context();

        let directive = DirectiveGenerator::generate(&dep, &usage, &score, &context);
        let json = directive.to_json().unwrap();

        assert!(json.contains("\"target_crate\": \"anyhow\""));
        assert!(json.contains("\"safety_score\": 85"));

        let parsed: TechnicalDirective = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.target_crate, directive.target_crate);
        assert_eq!(parsed.safety_score, directive.safety_score);
    }

    #[test]
    fn unused_dependency_directive_recommends_removal() {
        let dep = sample_dep("tap");
        let usage = CrateUsage::default();
        let score = ReplacementScore {
            overall: 95,
            confidence: 100,
            classification: SafetyClass::SafeToReplace,
            dimensions: ScoreDimensions::default(),
            reasoning: vec!["No usage detected".to_string()],
            recommendation: ReplacementRecommendation::Proceed,
        };
        let context = sample_context();

        let directive = DirectiveGenerator::generate(&dep, &usage, &score, &context);

        assert!(directive
            .implementation_steps
            .iter()
            .any(|s| s.contains("Remove") && s.contains("Cargo.toml")));
        assert!(directive
            .rationale
            .iter()
            .any(|s| s.contains("no detected source usage")));
    }

    #[test]
    fn blocker_recommendation_warns_against_replacement() {
        let dep = sample_dep("ring");
        let usage = sample_usage();
        let score = ReplacementScore {
            overall: 10,
            confidence: 90,
            classification: SafetyClass::SecurityCritical,
            dimensions: ScoreDimensions::default(),
            reasoning: vec!["Security critical".to_string()],
            recommendation: ReplacementRecommendation::SecurityBlock,
        };
        let context = sample_context();

        let directive = DirectiveGenerator::generate(&dep, &usage, &score, &context);

        assert!(directive
            .implementation_steps
            .iter()
            .any(|s| s.contains("Do not proceed")));
        assert!(directive
            .testing_plan
            .iter()
            .any(|s| s.contains("differential testing")));
    }

    impl TechnicalDirective {
        fn is_trivial_usage_reflected(&self) -> bool {
            self.scope_summary.contains("Usage is trivial")
                || self
                    .rationale
                    .iter()
                    .any(|s| s.contains("Usage is trivial"))
        }
    }
}
