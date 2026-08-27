// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! Contextual SWOT (Strengths / Weaknesses / Opportunities / Threats) review
//! of a generated replacement proposal.
//!
//! Unlike the raw dimension scores in [`crate::scoring::classifier`], a SWOT
//! analysis is meant to be read directly by a human deciding whether to merge
//! a generated replacement. Every bullet is derived from the same evidence
//! the classifier already collected (dimension scores, usage stats, category
//! rules, and — when available — the actual validated proposal), so the
//! review reflects this specific crate and this specific project rather than
//! generic advice.

use serde::{Deserialize, Serialize};

use crate::analysis::types::{CrateUsage, Dependency};
use crate::replacement::generator::ReplacementProposal;
use crate::scoring::classifier::ReplacementScore;
use crate::scoring::rules::{categorize_crate, FREQUENTLY_REPLACEABLE, HEAVY_TRANSITIVE_CRATES};

/// A contextual SWOT review of one replacement candidate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SwotAnalysis {
    /// Evidence favoring the replacement.
    pub strengths: Vec<String>,
    /// Evidence that makes the replacement harder or riskier to get right.
    pub weaknesses: Vec<String>,
    /// Secondary gains a successful replacement would unlock.
    pub opportunities: Vec<String>,
    /// Risks that could surface after the replacement ships.
    pub threats: Vec<String>,
}

impl SwotAnalysis {
    /// Whether any quadrant has content. A dependency with no usage at all
    /// can legitimately produce an empty analysis (nothing to weigh).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strengths.is_empty()
            && self.weaknesses.is_empty()
            && self.opportunities.is_empty()
            && self.threats.is_empty()
    }

    /// Render as a Markdown section (`## SWOT Analysis` with four
    /// subsections), suitable for embedding in a directive or PR body.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut lines = Vec::new();
        lines.push("## SWOT Analysis".to_string());
        lines.push(String::new());

        let quadrant = |lines: &mut Vec<String>, title: &str, items: &[String]| {
            lines.push(format!("### {title}"));
            lines.push(String::new());
            if items.is_empty() {
                lines.push("- None identified".to_string());
            } else {
                for item in items {
                    lines.push(format!("- {item}"));
                }
            }
            lines.push(String::new());
        };

        quadrant(&mut lines, "Strengths", &self.strengths);
        quadrant(&mut lines, "Weaknesses", &self.weaknesses);
        quadrant(&mut lines, "Opportunities", &self.opportunities);
        quadrant(&mut lines, "Threats", &self.threats);

        lines.join("\n")
    }
}

/// Build a [`SwotAnalysis`] from a dependency's score and usage.
///
/// Optionally enriched with a generated [`ReplacementProposal`] when one
/// already exists (its validation result is folded in as a strength or
/// weakness).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn analyze(
    dep: &Dependency,
    usage: &CrateUsage,
    score: &ReplacementScore,
    proposal: Option<&ReplacementProposal>,
) -> SwotAnalysis {
    let dims = &score.dimensions;
    let mut swot = SwotAnalysis::default();

    // --- Strengths -------------------------------------------------------
    if dims.usage_simplicity >= 70 {
        swot.strengths.push(format!(
            "Narrow usage surface: {} unique API(s) across {} call site(s), easy to reimplement and verify",
            usage.unique_api_usage,
            usage.call_sites.len()
        ));
    }
    if dims.transitive_value >= 70 {
        swot.strengths.push(format!(
            "Removes {} transitive dependenc{} from the build graph",
            dep.total_transitive_count(),
            if dep.total_transitive_count() == 1 {
                "y"
            } else {
                "ies"
            }
        ));
    }
    if dims.testability >= 70 {
        swot.strengths.push(format!(
            "Confined to {} file(s), so a replacement is cheap to differentially test",
            usage.affected_files.len().max(1)
        ));
    }
    if !usage.has_usage() {
        swot.strengths.push(
            "Dependency is unused in this project; removal carries no behavioral risk".to_string(),
        );
    }
    if let Some(p) = proposal {
        if let Some(report) = &p.validation_report {
            if report.success {
                swot.strengths.push(
                    "Generated replacement already compiles cleanly under `cargo check`"
                        .to_string(),
                );
            }
        }
    }
    if FREQUENTLY_REPLACEABLE.contains(&dep.name.as_str()) {
        swot.strengths.push(format!(
            "`{}` is a commonly replaced crate with an established, low-risk pattern",
            dep.name
        ));
    }

    // --- Weaknesses --------------------------------------------------------
    if dims.api_surface < 40 {
        swot.weaknesses.push(format!(
            "Uses ~{:.0}% of the crate's public API; a hand-written module must cover a wide surface",
            usage.api_coverage_percent
        ));
    }
    if dims.testability < 40 {
        swot.weaknesses.push(format!(
            "Spread across {} files, making full behavioral parity harder to confirm",
            usage.affected_files.len()
        ));
    }
    if dims.usage_simplicity < 40 && usage.has_usage() {
        swot.weaknesses.push(format!(
            "Usage is non-trivial ({} unique APIs, {} call sites); higher chance of subtly divergent behavior",
            usage.unique_api_usage,
            usage.call_sites.len()
        ));
    }
    if usage.used_in_public_api {
        swot.weaknesses.push(
            "Crate types/behavior are exposed through this project's own public API, so downstream consumers are also affected"
                .to_string(),
        );
    }
    if let Some(p) = proposal {
        if let Some(report) = &p.validation_report {
            if !report.success {
                swot.weaknesses.push(
                    "Generated replacement did not pass validation as-is; treat as a starting point, not a drop-in"
                        .to_string(),
                );
            }
        }
    }

    // --- Opportunities -------------------------------------------------------
    if HEAVY_TRANSITIVE_CRATES.contains(&dep.name.as_str()) {
        swot.opportunities.push(format!(
            "`{}` is a known heavy transitive contributor; replacing it should measurably improve compile time and binary size",
            dep.name
        ));
    }
    if dims.maintenance_burden >= 65 && dep.cve_count == 0 {
        swot.opportunities.push(format!(
            "Maintenance score is {}/100; replacing removes reliance on an upstream that itself carries maintenance risk",
            dep.maintenance_score
        ));
    }
    if let Some(p) = proposal {
        if !p.estimated_compile_time_reduction.is_empty() {
            swot.opportunities.push(format!(
                "Estimated compile-time reduction: {}",
                p.estimated_compile_time_reduction
            ));
        }
        if !p.estimated_binary_size_reduction.is_empty() {
            swot.opportunities.push(format!(
                "Estimated binary-size reduction: {}",
                p.estimated_binary_size_reduction
            ));
        }
    }

    // --- Threats -------------------------------------------------------------
    if dep.is_security_sensitive() {
        swot.threats.push(format!(
            "`{}` falls in a security-sensitive category ({}); a hand-rolled replacement risks reintroducing subtle vulnerabilities",
            dep.name,
            categorize_crate(&dep.name)
        ));
    }
    if dims.security_safety < 40 {
        swot.threats.push(
            "Security-safety dimension scored low; extra scrutiny is warranted even if the overall score looks favorable"
                .to_string(),
        );
    }
    if score.confidence < 50 {
        swot.threats.push(format!(
            "Assessment confidence is only {}/100 — usage may be under-detected (macros, dynamic dispatch, re-exports)",
            score.confidence
        ));
    }
    if dep.cve_count > 0 {
        swot.threats.push(format!(
            "{} known advisor{} against the current version; replacing without addressing the root cause is unsafe",
            dep.cve_count,
            if dep.cve_count == 1 { "y" } else { "ies" }
        ));
    }
    // Note: `proposal.risk_notes` mirrors `score.reasoning` (generic
    // per-dimension commentary, not curated risks) and is already reflected
    // above via the dimension-based bullets, so it is not duplicated here.

    swot
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::types::{DependencyKind, DependencySource};
    use crate::scoring::classifier::SafetyClassifier;

    fn dummy_dependency(name: &str) -> Dependency {
        Dependency {
            name: name.to_string(),
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
            maintenance_score: 50,
            cve_count: 0,
            license: None,
            download_count: 0,
        }
    }

    #[test]
    fn unused_dependency_yields_a_strength_and_no_threats() {
        let dep = dummy_dependency("tap");
        let usage = CrateUsage::default();
        let score = SafetyClassifier::new().score_dependency(&dep, &usage);
        let swot = analyze(&dep, &usage, &score, None);

        assert!(swot
            .strengths
            .iter()
            .any(|s| s.contains("unused in this project")));
        assert!(swot.threats.is_empty());
    }

    #[test]
    fn security_sensitive_crate_surfaces_a_threat() {
        let dep = dummy_dependency("my_auth_helper");
        let usage = CrateUsage {
            crate_name: "my_auth_helper".to_string(),
            call_sites: vec![crate::analysis::types::CallSite {
                function_name: "verify".to_string(),
                kind: crate::analysis::types::UsageKind::FunctionCall,
                location: crate::analysis::types::Location::new("src/lib.rs", 1, 1),
                context: "auth check".to_string(),
            }],
            ..Default::default()
        };
        let score = SafetyClassifier::new().score_dependency(&dep, &usage);
        let swot = analyze(&dep, &usage, &score, None);

        assert!(swot
            .threats
            .iter()
            .any(|t| t.contains("security-sensitive")));
    }

    #[test]
    fn heavy_transitive_crate_surfaces_an_opportunity() {
        let mut dep = dummy_dependency("clap");
        dep.transitive_deps = (0..10).map(|i| format!("dep{i}")).collect();
        let usage = CrateUsage::default();
        let score = SafetyClassifier::new().score_dependency(&dep, &usage);
        let swot = analyze(&dep, &usage, &score, None);

        assert!(swot
            .opportunities
            .iter()
            .any(|o| o.contains("heavy transitive contributor")));
    }

    #[test]
    fn low_confidence_scoring_surfaces_a_threat() {
        let dep = dummy_dependency("tap");
        let usage = CrateUsage {
            crate_name: "tap".to_string(),
            imported_items: (0..40)
                .map(|i| crate::analysis::types::ImportedItem {
                    name: format!("api_{i}"),
                    kind: crate::analysis::types::ItemKind::Function,
                    path: format!("tap::api_{i}"),
                    location: crate::analysis::types::Location::new("src/lib.rs", i, 1),
                })
                .collect(),
            call_sites: (0..150)
                .map(|i| crate::analysis::types::CallSite {
                    function_name: format!("call_{i}"),
                    kind: crate::analysis::types::UsageKind::FunctionCall,
                    location: crate::analysis::types::Location::new("src/lib.rs", i, 1),
                    context: "fn main()".to_string(),
                })
                .collect(),
            affected_files: (0..20).map(|i| format!("src/{i}.rs")).collect(),
            unique_api_usage: 40,
            ..Default::default()
        };
        let score = SafetyClassifier::new().score_dependency(&dep, &usage);
        let swot = analyze(&dep, &usage, &score, None);

        assert!(swot
            .threats
            .iter()
            .any(|t| t.contains("Assessment confidence")));
    }

    #[test]
    fn is_empty_reflects_content() {
        assert!(SwotAnalysis::default().is_empty());
        let mut swot = SwotAnalysis::default();
        swot.strengths.push("x".to_string());
        assert!(!swot.is_empty());
    }
}
