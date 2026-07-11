use tracing::{debug, info};

use super::rules::{
    categorize_crate, category_risk_penalty, maintenance_score, transitive_weight_score,
    usage_complexity_score, FREQUENTLY_REPLACEABLE, HEAVY_TRANSITIVE_CRATES, NEVER_REPLACE,
};
use crate::analysis::types::{CrateUsage, Dependency};
use crate::config::Weights;

/// Safety classification for a dependency replacement
#[derive(Debug, Clone)]
pub struct ReplacementScore {
    /// Overall replaceability score (0-100)
    pub overall: u8,
    /// Confidence in the analysis (0-100)
    pub confidence: u8,
    /// Safety classification
    pub classification: SafetyClass,
    /// Individual dimension scores
    pub dimensions: ScoreDimensions,
    /// Human-readable reasoning
    pub reasoning: Vec<String>,
    /// Recommended action
    pub recommendation: ReplacementRecommendation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetyClass {
    SafeToReplace,    // ★★★★★
    LowRisk,          // ★★★★☆
    MediumRisk,       // ★★★☆☆
    HighRisk,         // ★★☆☆☆
    DoNotReplace,     // ★☆☆☆☆
    SecurityCritical, // ◆◆◆◆◆ (special)
}

impl SafetyClass {
    #[must_use]
    pub const fn as_stars(&self) -> &'static str {
        match self {
            Self::SafeToReplace => "★★★★★",
            Self::LowRisk => "★★★★☆",
            Self::MediumRisk => "★★★☆☆",
            Self::HighRisk => "★★☆☆☆",
            Self::DoNotReplace => "★☆☆☆☆",
            Self::SecurityCritical => "◆◆◆◆◆",
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SafeToReplace => "Safe to Replace",
            Self::LowRisk => "Low Risk",
            Self::MediumRisk => "Medium Risk",
            Self::HighRisk => "High Risk",
            Self::DoNotReplace => "Do Not Replace",
            Self::SecurityCritical => "Security Critical",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScoreDimensions {
    /// How complex is the usage (0-100, higher = simpler/replaceable)
    pub usage_simplicity: u8,
    /// How many transitive deps would be removed (0-100)
    pub transitive_value: u8,
    /// Security risk of replacement (0-100, higher = safer to replace)
    pub security_safety: u8,
    /// Maintenance burden score (0-100, higher = more burden = more value in replacing)
    pub maintenance_burden: u8,
    /// How well can we validate a replacement (0-100)
    pub testability: u8,
    /// API coverage (lower coverage = higher score, 0-100)
    pub api_surface: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplacementRecommendation {
    /// Proceed with automated replacement
    Proceed,
    /// Generate proposal for human review
    Propose,
    /// Only replace with extensive testing
    Caution,
    /// Do not replace
    Block,
    /// Security critical - never replace
    SecurityBlock,
}

impl ReplacementRecommendation {
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::Proceed => "Low risk - automated replacement recommended",
            Self::Propose => "Generate proposal for human review",
            Self::Caution => "Replace only with extensive differential testing",
            Self::Block => "Do not replace - risk exceeds value",
            Self::SecurityBlock => "NEVER replace - security critical dependency",
        }
    }
}

/// Classifies dependencies by replacement safety.
///
/// The classifier can be configured with per-dimension weights via
/// [`Self::with_weights`]. If no weights are supplied, the built-in defaults
/// are used.
pub struct SafetyClassifier {
    weights: Weights,
}

impl SafetyClassifier {
    #[must_use]
    pub fn new() -> Self {
        Self {
            weights: Weights::default(),
        }
    }

    /// Create a classifier with custom per-dimension weights.
    ///
    /// Weights are assumed to sum to `1.0`. The caller is responsible for
    /// validation; unexpected totals will still produce a score in `0..=100`
    /// but may shift the meaning of the result.
    #[must_use]
    pub const fn with_weights(weights: Weights) -> Self {
        Self { weights }
    }

    /// Score a single dependency for replaceability
    pub fn score_dependency(&self, dep: &Dependency, usage: &CrateUsage) -> ReplacementScore {
        debug!("Scoring dependency: {}", dep.name);

        let mut reasoning = Vec::new();
        let mut dimensions = ScoreDimensions::default();

        // Dimension 1: Usage simplicity (0-100)
        let usage_score = Self::score_usage_simplicity(usage, dep);
        dimensions.usage_simplicity = usage_score;
        reasoning.push(format!(
            "Usage simplicity: {}% ({} unique APIs from {} call sites)",
            usage_score,
            usage.unique_api_usage,
            usage.call_sites.len()
        ));

        // Dimension 2: Transitive dependency value (0-100)
        let transitive_score = Self::score_transitive_value(dep);
        dimensions.transitive_value = transitive_score;
        reasoning.push(format!(
            "Transitive value: {}% (removes {} transitive deps)",
            transitive_score,
            dep.total_transitive_count()
        ));

        // Dimension 3: Security safety (0-100)
        let security_score = Self::score_security_safety(dep);
        dimensions.security_safety = security_score;
        reasoning.push(format!(
            "Security safety: {}% (category: {})",
            security_score,
            categorize_crate(&dep.name)
        ));

        // Dimension 4: Maintenance burden (0-100)
        let maint_score = Self::score_maintenance_burden(dep);
        dimensions.maintenance_burden = maint_score;
        reasoning.push(format!(
            "Maintenance burden: {}% ({} CVEs, score: {})",
            maint_score, dep.cve_count, dep.maintenance_score
        ));

        // Dimension 5: Testability (0-100)
        let test_score = Self::score_testability(usage);
        dimensions.testability = test_score;
        reasoning.push(format!(
            "Testability: {}% ({} affected files)",
            test_score,
            usage.affected_files.len()
        ));

        // Dimension 6: API surface (0-100, lower coverage = higher score)
        let api_surface = if usage.api_coverage_percent < 10.0 {
            90u8
        } else if usage.api_coverage_percent < 30.0 {
            70u8
        } else if usage.api_coverage_percent < 60.0 {
            40u8
        } else {
            10u8
        };
        dimensions.api_surface = api_surface;
        reasoning.push(format!(
            "API surface: {}% (using ~{:.0}% of public APIs)",
            api_surface, usage.api_coverage_percent
        ));

        // Calculate weighted overall score
        let overall = self.calculate_overall_score(&dimensions, dep);

        // Determine confidence in the assessment
        let confidence = Self::confidence_score(usage, dep);

        // Determine classification and recommendation
        let (classification, recommendation) = Self::classify(dep, overall, &dimensions, usage);

        let rule_applied = if NEVER_REPLACE.contains(&dep.name.as_str()) {
            "NEVER_REPLACE list match"
        } else if FREQUENTLY_REPLACEABLE.contains(&dep.name.as_str()) {
            "FREQUENTLY_REPLACEABLE list match"
        } else {
            "No special category rule"
        };
        reasoning.push(format!("Category rule applied: {rule_applied}"));

        info!(
            "Scored {}: overall={}, class={:?}",
            dep.name, overall, classification
        );

        reasoning.push(format!(
            "Confidence: {confidence}% (based on usage visibility)"
        ));

        ReplacementScore {
            overall,
            confidence,
            classification,
            dimensions,
            reasoning,
            recommendation,
        }
    }

    fn score_usage_simplicity(usage: &CrateUsage, _dep: &Dependency) -> u8 {
        let base = usage_complexity_score(usage.unique_api_usage, usage.call_sites.len());

        // Bonus for trivial usage
        let trivial_bonus: i16 = if usage.is_trivial_usage { 10 } else { 0 };

        // Bonus for unused dependencies
        let unused_bonus: i16 = if usage.imported_items.is_empty() {
            20
        } else {
            0
        };

        let score = 50 + base + trivial_bonus + unused_bonus;
        u8::try_from(score.clamp(0, 100)).unwrap_or(0)
    }

    fn score_transitive_value(dep: &Dependency) -> u8 {
        let score = transitive_weight_score(dep.total_transitive_count());
        let heavy_bonus: i16 = if HEAVY_TRANSITIVE_CRATES.contains(&dep.name.as_str()) {
            10
        } else {
            0
        };

        u8::try_from((50 + score + heavy_bonus).clamp(0, 100)).unwrap_or(0)
    }

    fn score_security_safety(dep: &Dependency) -> u8 {
        let penalty = category_risk_penalty(&dep.name);
        let cve_penalty: i16 = if dep.cve_count > 0 { -50 } else { 0 };
        let base = 50i16 + penalty + cve_penalty;

        // Security-sensitive crates get extra scrutiny
        if dep.is_security_sensitive() && penalty < 0 {
            return u8::try_from(base.clamp(0, 30)).unwrap_or(0); // Max 30 for security-sensitive
        }

        u8::try_from(base.clamp(0, 100)).unwrap_or(0)
    }

    fn score_maintenance_burden(dep: &Dependency) -> u8 {
        let score = maintenance_score(
            dep.cve_count,
            dep.maintenance_score,
            4.0, // Assume 4 releases/year as default
        );
        u8::try_from((50 + score).clamp(0, 100)).unwrap_or(0)
    }

    fn score_testability(usage: &CrateUsage) -> u8 {
        // More affected files = harder to test replacement thoroughly
        let file_penalty: i16 = match usage.affected_files.len() {
            0 => 20, // Unused - easy to test (just remove)
            1 => 15,
            2..=5 => 5,
            6..=15 => -5,
            _ => -15,
        };

        // Trivial usage is easier to validate
        let trivial_bonus: i16 = if usage.is_trivial_usage { 15 } else { 0 };

        u8::try_from((50 + file_penalty + trivial_bonus).clamp(0, 100)).unwrap_or(0)
    }

    fn confidence_score(usage: &CrateUsage, _dep: &Dependency) -> u8 {
        if usage.imported_items.is_empty() {
            return 100; // Unused dependencies are easy to remove with certainty
        }

        let base = 85u8;
        let api_penalty = match usage.unique_api_usage {
            0..=3 => 0,
            4..=10 => 10,
            11..=30 => 25,
            _ => 40,
        };
        let file_penalty = match usage.affected_files.len() {
            0..=1 => 0,
            2..=5 => 5,
            6..=15 => 15,
            _ => 25,
        };
        let call_penalty = match usage.call_sites.len() {
            0..=5 => 0,
            6..=20 => 5,
            21..=100 => 15,
            _ => 25,
        };

        base.saturating_sub(api_penalty)
            .saturating_sub(file_penalty)
            .saturating_sub(call_penalty)
    }

    fn calculate_overall_score(&self, dimensions: &ScoreDimensions, dep: &Dependency) -> u8 {
        // Weighted average using floating-point weights, then rounded back to an
        // integer in the 0..=100 range. Weights are expected to sum to 1.0.
        let pairs = [
            (dimensions.usage_simplicity, self.weights.usage_simplicity),
            (dimensions.transitive_value, self.weights.transitive_value),
            (dimensions.security_safety, self.weights.security_safety),
            (
                dimensions.maintenance_burden,
                self.weights.maintenance_burden,
            ),
            (dimensions.testability, self.weights.testability),
            (dimensions.api_surface, self.weights.api_surface),
        ];
        let weighted = pairs
            .iter()
            .map(|(value, weight)| f64::from(*value) * weight)
            .sum::<f64>();

        // Apply category overrides
        let adjusted = if NEVER_REPLACE.contains(&dep.name.as_str()) {
            weighted.min(20.0) // Hard cap at 20
        } else if FREQUENTLY_REPLACEABLE.contains(&dep.name.as_str()) {
            (weighted + 10.0).min(100.0) // Boost by 10
        } else {
            weighted
        };

        #[allow(clippy::cast_possible_truncation)]
        let clamped = adjusted.clamp(0.0, 100.0).round() as i64;
        u8::try_from(clamped).unwrap_or(0)
    }

    fn classify(
        dep: &Dependency,
        overall: u8,
        dimensions: &ScoreDimensions,
        usage: &CrateUsage,
    ) -> (SafetyClass, ReplacementRecommendation) {
        // Security-critical override
        if NEVER_REPLACE.contains(&dep.name.as_str()) {
            return (
                SafetyClass::SecurityCritical,
                ReplacementRecommendation::SecurityBlock,
            );
        }

        // Any known advisory blocks replacement regardless of score.
        if dep.cve_count > 0 {
            return (
                SafetyClass::SecurityCritical,
                ReplacementRecommendation::SecurityBlock,
            );
        }

        // Unused dependency special case
        if usage.imported_items.is_empty() {
            return (
                SafetyClass::SafeToReplace,
                ReplacementRecommendation::Proceed,
            );
        }

        // Score-based classification
        match overall {
            0..=15 => (SafetyClass::DoNotReplace, ReplacementRecommendation::Block),
            16..=35 => (SafetyClass::HighRisk, ReplacementRecommendation::Caution),
            36..=55 => (SafetyClass::MediumRisk, ReplacementRecommendation::Propose),
            56..=75 => (SafetyClass::LowRisk, ReplacementRecommendation::Propose),
            _ => {
                // 76-100
                if dimensions.security_safety < 30 {
                    // High security risk despite good score
                    (SafetyClass::MediumRisk, ReplacementRecommendation::Caution)
                } else {
                    (
                        SafetyClass::SafeToReplace,
                        ReplacementRecommendation::Proceed,
                    )
                }
            }
        }
    }
}

impl Default for SafetyClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::types::{Dependency, DependencyKind, DependencySource};

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
    fn unused_dependency_is_safe_to_replace() {
        let classifier = SafetyClassifier::new();
        let dep = dummy_dependency("tap");
        let usage = CrateUsage::default();
        let score = classifier.score_dependency(&dep, &usage);

        assert_eq!(score.classification, SafetyClass::SafeToReplace);
        assert_eq!(score.recommendation, ReplacementRecommendation::Proceed);
        assert_eq!(score.confidence, 100);
    }

    #[test]
    fn crypto_dependency_is_security_critical() {
        let classifier = SafetyClassifier::new();
        let dep = dummy_dependency("ring");
        let usage = CrateUsage::default();
        let score = classifier.score_dependency(&dep, &usage);

        assert_eq!(score.classification, SafetyClass::SecurityCritical);
        assert_eq!(
            score.recommendation,
            ReplacementRecommendation::SecurityBlock
        );
    }

    #[test]
    fn cve_override_blocks_replacement() {
        let classifier = SafetyClassifier::new();
        let mut dep = dummy_dependency("tap");
        dep.cve_count = 1;
        let usage = CrateUsage::default();
        let score = classifier.score_dependency(&dep, &usage);

        assert_eq!(score.classification, SafetyClass::SecurityCritical);
        assert_eq!(
            score.recommendation,
            ReplacementRecommendation::SecurityBlock
        );
    }

    #[test]
    fn trivial_usage_boosts_score() {
        let classifier = SafetyClassifier::new();
        let dep = dummy_dependency("tap");
        let usage = CrateUsage {
            crate_name: "tap".to_string(),
            imported_items: vec![],
            call_sites: vec![],
            is_trivial_usage: true,
            ..Default::default()
        };
        let score = classifier.score_dependency(&dep, &usage);

        assert!(
            matches!(
                score.classification,
                SafetyClass::SafeToReplace | SafetyClass::LowRisk
            ),
            "unexpected classification {:?}",
            score.classification
        );
    }

    #[test]
    fn overall_score_is_within_bounds() {
        let classifier = SafetyClassifier::new();
        let dep = dummy_dependency("tap");
        let usage = CrateUsage::default();
        let score = classifier.score_dependency(&dep, &usage);

        assert!(score.overall <= 100);
        assert!(score.confidence <= 100);
    }

    #[test]
    fn safety_class_methods_cover_all_variants() {
        for class in [
            SafetyClass::SafeToReplace,
            SafetyClass::LowRisk,
            SafetyClass::MediumRisk,
            SafetyClass::HighRisk,
            SafetyClass::DoNotReplace,
            SafetyClass::SecurityCritical,
        ] {
            assert!(!class.as_stars().is_empty());
            assert!(!class.name().is_empty());
        }
    }

    #[test]
    fn recommendation_descriptions_are_non_empty() {
        for rec in [
            ReplacementRecommendation::Proceed,
            ReplacementRecommendation::Propose,
            ReplacementRecommendation::Caution,
            ReplacementRecommendation::Block,
            ReplacementRecommendation::SecurityBlock,
        ] {
            assert!(!rec.description().is_empty());
        }
    }

    #[test]
    fn medium_risk_dependency_is_classified() {
        let classifier = SafetyClassifier::new();
        let dep = dummy_dependency("heavy_crate");
        let usage = CrateUsage {
            crate_name: "heavy_crate".to_string(),
            imported_items: (0..20)
                .map(|i| crate::analysis::types::ImportedItem {
                    name: format!("api_{i}"),
                    kind: crate::analysis::types::ItemKind::Function,
                    path: format!("heavy_crate::api_{i}"),
                    location: crate::analysis::types::Location::new("src/lib.rs", i, 1),
                })
                .collect(),
            call_sites: (0..200)
                .map(|i| crate::analysis::types::CallSite {
                    function_name: format!("call_{i}"),
                    kind: crate::analysis::types::UsageKind::FunctionCall,
                    location: crate::analysis::types::Location::new("src/lib.rs", i, 1),
                    context: "fn main()".to_string(),
                })
                .collect(),
            affected_files: vec!["src/lib.rs".to_string()],
            unique_api_usage: 20,
            api_coverage_percent: 80.0,
            is_trivial_usage: false,
            ..Default::default()
        };
        let score = classifier.score_dependency(&dep, &usage);
        assert_eq!(score.classification, SafetyClass::MediumRisk);
    }

    #[test]
    fn api_surface_branches_are_scored() {
        let classifier = SafetyClassifier::new();
        let dep = dummy_dependency("tap");

        for (pct, expected) in [(5.0, 90), (20.0, 70), (45.0, 40), (80.0, 10)] {
            let usage = CrateUsage {
                crate_name: "tap".to_string(),
                imported_items: vec![crate::analysis::types::ImportedItem {
                    name: "x".to_string(),
                    kind: crate::analysis::types::ItemKind::Function,
                    path: "tap::x".to_string(),
                    location: crate::analysis::types::Location::new("src/lib.rs", 1, 1),
                }],
                api_coverage_percent: pct,
                ..Default::default()
            };
            let score = classifier.score_dependency(&dep, &usage);
            assert_eq!(
                score.dimensions.api_surface, expected,
                "api_surface wrong for {pct}%"
            );
        }
    }

    #[test]
    fn testability_penalizes_many_affected_files() {
        let classifier = SafetyClassifier::new();
        let dep = dummy_dependency("tap");

        let few_files = CrateUsage {
            crate_name: "tap".to_string(),
            affected_files: (0..5).map(|i| format!("src/{i}.rs")).collect(),
            is_trivial_usage: false,
            ..Default::default()
        };
        assert_eq!(
            classifier
                .score_dependency(&dep, &few_files)
                .dimensions
                .testability,
            55
        );

        let many_files = CrateUsage {
            crate_name: "tap".to_string(),
            affected_files: (0..20).map(|i| format!("src/{i}.rs")).collect(),
            is_trivial_usage: false,
            ..Default::default()
        };
        assert_eq!(
            classifier
                .score_dependency(&dep, &many_files)
                .dimensions
                .testability,
            35
        );
    }

    #[test]
    fn confidence_drops_with_heavy_usage() {
        let classifier = SafetyClassifier::new();
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
        let score = classifier.score_dependency(&dep, &usage);
        assert_eq!(score.confidence, 0);
    }

    #[test]
    fn low_risk_dependency_is_classified() {
        let classifier = SafetyClassifier::new();
        // A frequently-replaceable crate with moderate usage lands in LowRisk.
        let dep = dummy_dependency("itertools");
        let usage = CrateUsage {
            crate_name: "itertools".to_string(),
            imported_items: (0..3)
                .map(|i| crate::analysis::types::ImportedItem {
                    name: format!("api_{i}"),
                    kind: crate::analysis::types::ItemKind::Function,
                    path: format!("itertools::api_{i}"),
                    location: crate::analysis::types::Location::new("src/lib.rs", i, 1),
                })
                .collect(),
            call_sites: (0..10)
                .map(|i| crate::analysis::types::CallSite {
                    function_name: format!("call_{i}"),
                    kind: crate::analysis::types::UsageKind::FunctionCall,
                    location: crate::analysis::types::Location::new("src/lib.rs", i, 1),
                    context: "fn main()".to_string(),
                })
                .collect(),
            affected_files: vec!["src/lib.rs".to_string()],
            unique_api_usage: 3,
            api_coverage_percent: 20.0,
            is_trivial_usage: false,
            ..Default::default()
        };
        let score = classifier.score_dependency(&dep, &usage);
        assert_eq!(score.classification, SafetyClass::LowRisk);
        assert_eq!(score.recommendation, ReplacementRecommendation::Propose);
    }

    #[test]
    #[allow(clippy::default_constructed_unit_structs)]
    fn default_classifier_matches_new() {
        let dep = dummy_dependency("tap");
        let usage = CrateUsage::default();
        assert_eq!(
            SafetyClassifier::default()
                .score_dependency(&dep, &usage)
                .overall,
            SafetyClassifier::new()
                .score_dependency(&dep, &usage)
                .overall
        );
    }

    #[test]
    fn custom_weights_change_overall_score() {
        let dep = dummy_dependency("tap");
        let usage = CrateUsage {
            crate_name: "tap".to_string(),
            unique_api_usage: 1,
            api_coverage_percent: 5.0,
            is_trivial_usage: true,
            ..Default::default()
        };

        let default_score = SafetyClassifier::new()
            .score_dependency(&dep, &usage)
            .overall;

        // Emphasize API surface heavily so the high api_surface score dominates.
        let heavy_api_weights = crate::config::Weights {
            usage_simplicity: 0.0,
            transitive_value: 0.0,
            security_safety: 0.0,
            maintenance_burden: 0.0,
            testability: 0.0,
            api_surface: 1.0,
        };
        let weighted_score = SafetyClassifier::with_weights(heavy_api_weights)
            .score_dependency(&dep, &usage)
            .overall;

        assert_ne!(default_score, weighted_score);
        assert!(weighted_score > default_score);
    }
}
