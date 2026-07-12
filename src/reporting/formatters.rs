use crate::amber_anyhow::Result;
use crate::reporting::style::Colorize;
use comfy_table::{
    modifiers::UTF8_ROUND_CORNERS, Attribute, Cell, CellAlignment, ColumnConstraint,
    ContentArrangement, Table, Width,
};
use serde_json::json;
use std::collections::HashMap;

use crate::analysis::types::{CrateUsage, Dependency};
use crate::replacement::directives::TechnicalDirective;
use crate::replacement::generator::ReplacementProposal;
use crate::scoring::classifier::{ReplacementRecommendation, ReplacementScore, SafetyClass};

/// Pre-computed classification counts used by full reports.
struct ScoreSummary {
    safe_to_replace: usize,
    low_risk: usize,
    medium_risk: usize,
    high_risk: usize,
    do_not_replace: usize,
    above_threshold: usize,
}

impl ScoreSummary {
    fn new(scores: &[ReplacementScore], threshold: u8) -> Self {
        let mut safe_to_replace = 0;
        let mut low_risk = 0;
        let mut medium_risk = 0;
        let mut high_risk = 0;
        let mut do_not_replace = 0;
        let mut above_threshold = 0;

        for score in scores {
            match score.classification {
                SafetyClass::SafeToReplace => safe_to_replace += 1,
                SafetyClass::LowRisk => low_risk += 1,
                SafetyClass::MediumRisk => medium_risk += 1,
                SafetyClass::HighRisk => high_risk += 1,
                SafetyClass::DoNotReplace | SafetyClass::SecurityCritical => do_not_replace += 1,
            }
            if score.overall >= threshold {
                above_threshold += 1;
            }
        }

        Self {
            safe_to_replace,
            low_risk,
            medium_risk,
            high_risk,
            do_not_replace,
            above_threshold,
        }
    }
}

/// Console-based report output
pub struct ConsoleReporter;

impl ConsoleReporter {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn print_dependency_list(&self, deps: &[Dependency]) {
        let mut table = Table::new();
        table.set_header(vec!["Crate", "Version", "Kind", "Transitive Deps"]);
        table.apply_modifier(UTF8_ROUND_CORNERS);

        for dep in deps {
            let kind = format!("{:?}", dep.kind);
            table.add_row(vec![
                dep.name.as_str(),
                dep.version.as_str(),
                kind.as_str(),
                &dep.total_transitive_count().to_string(),
            ]);
        }

        println!("{table}");
        println!("\n  Total: {} dependencies\n", deps.len());
    }

    pub fn print_score_card(&self, crate_name: &str, score: &ReplacementScore, usage: &CrateUsage) {
        println!();
        println!(
            "  {} Score Card for {}",
            "◈".bright_yellow(),
            crate_name.cyan().bold()
        );
        println!();
        println!(
            "  Overall: {} {}/100",
            score.overall.to_string().bold(),
            Self::score_bar(score.overall)
        );
        println!(
            "  Confidence: {} {}/100",
            score.confidence.to_string().bold(),
            Self::score_bar(score.confidence)
        );
        println!(
            "  Class:   {} {}",
            score.classification.as_stars(),
            score.classification.name().dimmed()
        );
        println!("  Action:  {}", score.recommendation.description().italic());
        println!();

        let mut table = Table::new();
        table.set_header(vec!["Dimension", "Score", "Bar"]);
        table.apply_modifier(UTF8_ROUND_CORNERS);

        let dims = [
            ("Usage Simplicity", score.dimensions.usage_simplicity),
            ("Transitive Value", score.dimensions.transitive_value),
            ("Security Safety", score.dimensions.security_safety),
            ("Maintenance Burden", score.dimensions.maintenance_burden),
            ("Testability", score.dimensions.testability),
            ("API Surface", score.dimensions.api_surface),
        ];

        for (name, val) in &dims {
            table.add_row(vec![*name, &val.to_string(), &Self::score_bar(*val)]);
        }

        println!("{table}");
        println!();

        if !usage.imported_items.is_empty() {
            println!("  {} Usage Details:", "→".dimmed());
            for item in &usage.imported_items {
                println!(
                    "    {} {} ({})",
                    "·".dimmed(),
                    item.name,
                    format!("{:?}", item.kind).dimmed()
                );
            }
        }
        println!();
    }

    #[allow(clippy::too_many_lines)]
    pub fn print_full_report(
        &self,
        deps: &[Dependency],
        usage: &HashMap<String, CrateUsage>,
        scores: &[ReplacementScore],
        threshold: u8,
    ) {
        println!();
        println!("  {}", "═".repeat(60).dimmed());
        println!("  {}", "Amber Dependency Analysis Report".bold());
        println!("  {}", "═".repeat(60).dimmed());
        println!();

        let summary = ScoreSummary::new(scores, threshold);

        println!("  {}", "Summary".underline().bold());
        println!();
        println!(
            "  {} Safe to replace",
            format!("{}", summary.safe_to_replace).green().bold()
        );
        println!(
            "  {} Low risk",
            format!("{}", summary.low_risk).bright_green()
        );
        println!(
            "  {} Medium risk",
            format!("{}", summary.medium_risk).yellow()
        );
        println!(
            "  {} High risk / Blocked",
            format!("{}", summary.high_risk + summary.do_not_replace).red()
        );
        println!();
        println!(
            "  {} dependencies above threshold ({})",
            summary.above_threshold.to_string().bold(),
            format!("threshold={threshold}").dimmed()
        );
        println!();

        // Detailed table
        println!("  {}", "Detailed Analysis".underline().bold());
        println!();

        let mut table = Table::new();
        table.set_header(vec![
            Cell::new("Crate").add_attribute(Attribute::Bold),
            Cell::new("Score").add_attribute(Attribute::Bold),
            Cell::new("Class").add_attribute(Attribute::Bold),
            Cell::new("APIs Used").add_attribute(Attribute::Bold),
            Cell::new("Recommendation").add_attribute(Attribute::Bold),
        ]);
        table.apply_modifier(UTF8_ROUND_CORNERS);
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_constraints(vec![
            ColumnConstraint::LowerBoundary(Width::Fixed(16)),
            ColumnConstraint::LowerBoundary(Width::Fixed(8)),
            ColumnConstraint::LowerBoundary(Width::Fixed(11)),
            ColumnConstraint::LowerBoundary(Width::Fixed(10)),
            ColumnConstraint::LowerBoundary(Width::Fixed(16)),
        ]);
        if let Some(col) = table.column_mut(1) {
            col.set_cell_alignment(CellAlignment::Center);
        }
        if let Some(col) = table.column_mut(2) {
            col.set_cell_alignment(CellAlignment::Center);
        }
        if let Some(col) = table.column_mut(3) {
            col.set_cell_alignment(CellAlignment::Center);
        }

        for (dep, score) in deps.iter().zip(scores.iter()) {
            let api_text = match usage.get(&dep.name) {
                Some(u) if !u.imported_items.is_empty() => u.unique_api_usage.to_string(),
                Some(u) if !u.call_sites.is_empty() => u.call_sites.len().to_string(),
                _ => "unused".dimmed(),
            };

            let rec_short = match score.recommendation {
                ReplacementRecommendation::Proceed => "Proceed".green(),
                ReplacementRecommendation::Propose => "Review".yellow(),
                ReplacementRecommendation::Caution => "Caution".bright_red(),
                ReplacementRecommendation::Block => "Block".red(),
                ReplacementRecommendation::SecurityBlock => "SECURE".red().bold(),
            };

            let score_colored = if score.overall >= 75 {
                score.overall.to_string().green()
            } else if score.overall >= 50 {
                score.overall.to_string().yellow()
            } else if score.overall >= 25 {
                score.overall.to_string().bright_red()
            } else {
                score.overall.to_string().red()
            };
            let class_str = score.classification.as_stars().to_string();

            table.add_row(vec![
                dep.name.as_str(),
                score_colored.as_str(),
                class_str.as_str(),
                api_text.as_str(),
                rec_short.as_str(),
            ]);
        }

        println!("{table}");
        println!();
    }

    pub fn print_replacement_proposal(&self, proposal: &ReplacementProposal) {
        println!();
        let source_tag = proposal
            .source
            .as_ref()
            .map_or(String::new(), |s| format!(" ({s})"));
        println!(
            "  {} Replacement Proposal: {} → {}{}",
            "◈".bright_yellow(),
            proposal.original_crate.red(),
            proposal.replacement_module.green(),
            source_tag.dimmed()
        );
        println!();
        println!(
            "  {} Generated module: {} bytes",
            "→".dimmed(),
            proposal.replacement_code.len().to_string().green()
        );
        println!(
            "  {} Compile time: {}",
            "→".dimmed(),
            proposal.estimated_compile_time_reduction.green()
        );
        println!(
            "  {} Binary size:  {}",
            "→".dimmed(),
            proposal.estimated_binary_size_reduction.green()
        );
        println!();
        if let Some(report) = &proposal.validation_report {
            let status = if report.success {
                "validation passed".green()
            } else {
                "validation failed".red()
            };
            println!("  {} Validation: {}", "→".dimmed(), status);
            println!();
        }
        if !proposal.risk_notes.is_empty() {
            println!("  {}", "Risk Notes:".underline());
            for note in &proposal.risk_notes {
                println!("    {} {}", "·".dimmed(), note);
            }
            println!();
        }
        println!("  {}", "Validation Strategy:".underline());
        for strategy in &proposal.validation_strategy {
            println!("    {} {}", "·".dimmed(), strategy);
        }
        println!();
        println!("  {}", "Test Plan:".underline());
        for test in &proposal.test_plan {
            println!("    {} {}", "·".dimmed(), test);
        }
        println!();
        println!(
            "  {} Generated: {}/{}",
            "✓".green(),
            "amber_proposals/".cyan(),
            format!("{}.rs", proposal.replacement_module).cyan()
        );
        println!();
    }

    /// Print a compact summary of a technical directive followed by its Markdown body.
    pub fn print_directive(&self, directive: &TechnicalDirective) {
        println!();
        println!(
            "  {} Technical Directive: {}",
            "◈".bright_yellow(),
            directive.target_crate.cyan().bold()
        );
        println!();
        println!(
            "  {} Score: {}/100 (confidence: {}/100)",
            "→".dimmed(),
            directive.safety_score.to_string().bold(),
            directive.confidence
        );
        println!(
            "  {} Classification: {}",
            "→".dimmed(),
            directive.safety_classification.green()
        );
        println!(
            "  {} Affected files: {}",
            "→".dimmed(),
            directive.affected_files.len().to_string().green()
        );
        if directive.used_in_public_api {
            println!("  {} Public API exposure detected", "⚠".bright_red());
        }
        println!();
        println!("{}", directive.to_markdown());
    }

    fn score_bar(score: u8) -> String {
        let filled = (score as usize) / 5;
        let empty = 20 - filled;
        let bar = "█".repeat(filled) + &"░".repeat(empty);
        match score {
            0..=25 => bar.red(),
            26..=50 => bar.yellow(),
            51..=75 => bar.bright_green(),
            _ => bar.green(),
        }
    }
}

impl Default for ConsoleReporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Emoji-filled report output for terminal sharing and demos.
pub struct EmojiReporter;

impl EmojiReporter {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn print_dependency_list(&self, deps: &[Dependency]) {
        println!("📦 Amber dependency list\n");
        for dep in deps {
            println!(
                "   🔹 {} {} {}",
                dep.name.cyan().bold(),
                dep.version.dimmed(),
                format!("({:?})", dep.kind).dimmed()
            );
        }
        println!("\n   🧮 Total: {} dependencies\n", deps.len());
    }

    pub fn print_score_card(&self, crate_name: &str, score: &ReplacementScore, usage: &CrateUsage) {
        println!();
        println!(
            "   {} Score card for {}",
            Self::class_emoji(&score.classification),
            crate_name.cyan().bold()
        );
        println!(
            "   📊 Overall:   {} {}",
            score.overall.to_string().bold(),
            Self::score_bar(score.overall)
        );
        println!(
            "   🎯 Confidence: {} {}",
            score.confidence.to_string().bold(),
            Self::score_bar(score.confidence)
        );
        println!(
            "   🏷️  Class:     {} {}",
            score.classification.as_stars(),
            score.classification.name().dimmed()
        );
        println!(
            "   ⚡ Action:    {}",
            score.recommendation.description().italic()
        );
        println!();

        if !usage.imported_items.is_empty() {
            println!("   🧩 Usage details:");
            for item in &usage.imported_items {
                println!(
                    "      {} {} ({})",
                    "•".dimmed(),
                    item.name,
                    format!("{:?}", item.kind).dimmed()
                );
            }
            println!();
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn print_full_report(
        &self,
        deps: &[Dependency],
        usage: &HashMap<String, CrateUsage>,
        scores: &[ReplacementScore],
        threshold: u8,
    ) {
        println!();
        println!("   🔥 Amber Dependency Analysis Report 🔥");
        println!("   {}", "═".repeat(55).dimmed());
        println!();

        let summary = ScoreSummary::new(scores, threshold);

        println!("   📋 Summary");
        println!();
        println!(
            "   ✅ Safe to replace:     {}",
            format!("{}", summary.safe_to_replace).green().bold()
        );
        println!(
            "   🟢 Low risk:            {}",
            format!("{}", summary.low_risk).bright_green()
        );
        println!(
            "   🟡 Medium risk:         {}",
            format!("{}", summary.medium_risk).yellow()
        );
        println!(
            "   🛑 High risk / Blocked: {}",
            format!("{}", summary.high_risk + summary.do_not_replace).red()
        );
        println!();
        println!(
            "   🎯 {} dependencies above threshold ({})",
            summary.above_threshold.to_string().bold(),
            format!("threshold={threshold}").dimmed()
        );
        println!();

        println!("   📊 Detailed Analysis");
        println!();

        let mut table = Table::new();
        table.set_header(vec![
            Cell::new("Crate").add_attribute(Attribute::Bold),
            Cell::new("Score").add_attribute(Attribute::Bold),
            Cell::new("Class").add_attribute(Attribute::Bold),
            Cell::new("APIs").add_attribute(Attribute::Bold),
            Cell::new("Recommendation").add_attribute(Attribute::Bold),
        ]);
        table.apply_modifier(UTF8_ROUND_CORNERS);
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_constraints(vec![
            ColumnConstraint::LowerBoundary(Width::Fixed(16)),
            ColumnConstraint::LowerBoundary(Width::Fixed(8)),
            ColumnConstraint::LowerBoundary(Width::Fixed(6)),
            ColumnConstraint::LowerBoundary(Width::Fixed(8)),
            ColumnConstraint::LowerBoundary(Width::Fixed(16)),
        ]);
        if let Some(col) = table.column_mut(1) {
            col.set_cell_alignment(CellAlignment::Center);
        }
        if let Some(col) = table.column_mut(2) {
            col.set_cell_alignment(CellAlignment::Center);
        }
        if let Some(col) = table.column_mut(3) {
            col.set_cell_alignment(CellAlignment::Center);
        }

        for (dep, score) in deps.iter().zip(scores.iter()) {
            let api_text = match usage.get(&dep.name) {
                Some(u) if !u.imported_items.is_empty() => u.unique_api_usage.to_string(),
                Some(u) if !u.call_sites.is_empty() => u.call_sites.len().to_string(),
                _ => "unused 💤".dimmed(),
            };

            let rec_emoji = Self::recommendation_emoji(&score.recommendation);
            let score_colored = if score.overall >= 75 {
                score.overall.to_string().green()
            } else if score.overall >= 50 {
                score.overall.to_string().yellow()
            } else if score.overall >= 25 {
                score.overall.to_string().bright_red()
            } else {
                score.overall.to_string().red()
            };

            table.add_row(vec![
                dep.name.as_str(),
                score_colored.as_str(),
                Self::class_emoji(&score.classification),
                api_text.as_str(),
                rec_emoji,
            ]);
        }

        println!("{table}");
        println!();
    }

    pub fn print_replacement_proposal(&self, proposal: &ReplacementProposal) {
        println!();
        let source_tag = proposal
            .source
            .as_ref()
            .map_or(String::new(), |s| format!(" ({s})"));
        println!(
            "   🛠️  Replacement proposal: {} → {}{}",
            proposal.original_crate.red(),
            proposal.replacement_module.green(),
            source_tag.dimmed()
        );
        println!(
            "   📄 Generated module: {} bytes",
            proposal.replacement_code.len().to_string().green()
        );
        if let Some(report) = &proposal.validation_report {
            let icon = if report.success { "✅" } else { "❌" };
            let summary = if report.success {
                "validation passed"
            } else {
                "validation failed"
            };
            println!("   {icon} Validation: {summary}");
        }
        println!();
    }

    /// Print a compact emoji summary of a technical directive followed by its Markdown body.
    pub fn print_directive(&self, directive: &TechnicalDirective) {
        println!();
        println!(
            "   📋 Technical directive for {}",
            directive.target_crate.cyan().bold()
        );
        println!(
            "   📊 Score: {}/100 (confidence: {}/100)",
            directive.safety_score.to_string().bold(),
            directive.confidence
        );
        println!(
            "   🏷️  Classification: {}",
            directive.safety_classification.green()
        );
        println!(
            "   📁 Affected files: {}",
            directive.affected_files.len().to_string().green()
        );
        if directive.used_in_public_api {
            println!("   ⚠️  Public API exposure detected");
        }
        println!();
        println!("{}", directive.to_markdown());
    }

    const fn class_emoji(class: &SafetyClass) -> &'static str {
        match class {
            SafetyClass::SafeToReplace => "✅",
            SafetyClass::LowRisk => "🟢",
            SafetyClass::MediumRisk => "🟡",
            SafetyClass::HighRisk => "🟠",
            SafetyClass::DoNotReplace => "🔴",
            SafetyClass::SecurityCritical => "🛡️",
        }
    }

    const fn recommendation_emoji(rec: &ReplacementRecommendation) -> &'static str {
        match rec {
            ReplacementRecommendation::Proceed => "🚀 Proceed",
            ReplacementRecommendation::Propose => "📝 Review",
            ReplacementRecommendation::Caution => "⚠️  Caution",
            ReplacementRecommendation::Block => "🛑 Block",
            ReplacementRecommendation::SecurityBlock => "🛡️  Secure",
        }
    }

    fn score_bar(score: u8) -> String {
        let filled = (score as usize).div_ceil(10);
        let empty = 10_usize.saturating_sub(filled);
        "🟩".repeat(filled) + &"⬜".repeat(empty)
    }
}

impl Default for EmojiReporter {
    fn default() -> Self {
        Self::new()
    }
}

/// JSON report output
pub struct JsonReporter;

impl JsonReporter {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Generate a JSON report.
    ///
    /// # Errors
    ///
    /// Returns an error if the report cannot be serialized to JSON.
    pub fn generate_json(
        &self,
        deps: &[Dependency],
        usage: &HashMap<String, CrateUsage>,
        scores: &[ReplacementScore],
    ) -> Result<String> {
        let report = deps
            .iter()
            .zip(scores.iter())
            .map(|(dep, score)| {
                let usage_stats = usage.get(&dep.name).cloned().unwrap_or_default();
                json!({
                    "crate": dep.name,
                    "version": dep.version,
                    "score": {
                        "overall": score.overall,
                        "confidence": score.confidence,
                        "classification": score.classification.name(),
                        "classification_stars": score.classification.as_stars(),
                        "dimensions": {
                            "usage_simplicity": score.dimensions.usage_simplicity,
                            "transitive_value": score.dimensions.transitive_value,
                            "security_safety": score.dimensions.security_safety,
                            "maintenance_burden": score.dimensions.maintenance_burden,
                            "testability": score.dimensions.testability,
                            "api_surface": score.dimensions.api_surface,
                        },
                        "recommendation": match score.recommendation {
                            super::super::scoring::classifier::ReplacementRecommendation::Proceed => "proceed",
                            super::super::scoring::classifier::ReplacementRecommendation::Propose => "propose",
                            super::super::scoring::classifier::ReplacementRecommendation::Caution => "caution",
                            super::super::scoring::classifier::ReplacementRecommendation::Block => "block",
                            super::super::scoring::classifier::ReplacementRecommendation::SecurityBlock => "security_block",
                        },
                    },
                    "usage": {
                        "imported_items": usage_stats.imported_items.len(),
                        "unique_apis": usage_stats.unique_api_usage,
                        "call_sites": usage_stats.call_sites.len(),
                        "affected_files": usage_stats.affected_files.len(),
                        "is_trivial": usage_stats.is_trivial_usage,
                    },
                    "metadata": {
                        "transitive_deps": dep.transitive_deps.len(),
                        "license": dep.license,
                        "maintenance_score": dep.maintenance_score,
                        "cve_count": dep.cve_count,
                    },
                    "reasoning": score.reasoning,
                })
            })
            .collect::<Vec<_>>();

        Ok(serde_json::to_string_pretty(&json!({
            "amber_version": env!("CARGO_PKG_VERSION"),
            "total_dependencies": deps.len(),
            "results": report,
        }))?)
    }

    /// Generate JSON for a single technical directive.
    ///
    /// # Errors
    ///
    /// Returns an error if the directive cannot be serialized to JSON.
    pub fn generate_directive_json(&self, directive: &TechnicalDirective) -> Result<String> {
        directive.to_json()
    }
}

impl Default for JsonReporter {
    fn default() -> Self {
        Self::new()
    }
}

/// PR description report format
pub struct PrReporter;

impl PrReporter {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn generate_pr_body(
        &self,
        deps: &[Dependency],
        usage: &HashMap<String, CrateUsage>,
        scores: &[ReplacementScore],
        threshold: u8,
    ) -> String {
        let mut lines = Vec::new();

        lines.push("# Amber Dependency Reduction Report".to_string());
        lines.push(String::new());
        lines.push(format!(
            "> Generated by Amber v{} — Autonomous Dependency Reduction Engine",
            env!("CARGO_PKG_VERSION")
        ));
        lines.push(String::new());

        // Summary
        let actionable: Vec<_> = deps
            .iter()
            .zip(scores.iter())
            .filter(|(_, s)| s.overall >= threshold)
            .collect();

        lines.push("## Summary".to_string());
        lines.push(String::new());
        lines.push(format!("- **Total dependencies analyzed:** {}", deps.len()));
        lines.push(format!(
            "- **Above threshold ({}):** {}",
            threshold,
            actionable.len()
        ));
        lines.push("- **Estimated compile time reduction:** See details below".to_string());
        lines.push("- **Estimated binary size reduction:** See details below".to_string());
        lines.push(String::new());

        // Actionable items
        if !actionable.is_empty() {
            lines.push("## Proposed Replacements".to_string());
            lines.push(String::new());

            for (dep, score) in &actionable {
                let usage_stats = usage.get(&dep.name).cloned().unwrap_or_default();
                lines.push(format!(
                    "### `{}` → `amber::{}`",
                    dep.name,
                    dep.name.replace('-', "_")
                ));
                lines.push(String::new());
                lines.push(format!(
                    "- **Safety score:** {}/100 {} (confidence: {}/100)",
                    score.overall,
                    score.classification.as_stars(),
                    score.confidence
                ));
                lines.push(format!(
                    "- **Classification:** {}",
                    score.classification.name()
                ));
                lines.push(format!(
                    "- **API usage:** {} unique APIs from {} call sites",
                    usage_stats.unique_api_usage,
                    usage_stats.call_sites.len()
                ));
                lines.push(format!(
                    "- **Affected files:** {}",
                    usage_stats.affected_files.len()
                ));
                if !dep.transitive_deps.is_empty() {
                    lines.push(format!(
                        "- **Transitive deps removed:** {}",
                        dep.transitive_deps.len()
                    ));
                }
                lines.push(format!(
                    "- **Recommendation:** {}",
                    score.recommendation.description()
                ));
                lines.push(String::new());

                if !score.reasoning.is_empty() {
                    lines.push("**Reasoning:**".to_string());
                    for r in &score.reasoning {
                        lines.push(format!("- {r}"));
                    }
                    lines.push(String::new());
                }
            }
        }

        // Security-critical (never replace)
        let critical: Vec<_> = deps
            .iter()
            .zip(scores.iter())
            .filter(|(_, s)| {
                matches!(
                    s.classification,
                    super::super::scoring::classifier::SafetyClass::SecurityCritical
                )
            })
            .collect();

        if !critical.is_empty() {
            lines.push("## Security Critical (Do Not Replace)".to_string());
            lines.push(String::new());
            for (dep, score) in &critical {
                lines.push(format!(
                    "- `{}` — {} — {}",
                    dep.name,
                    score.classification.as_stars(),
                    score
                        .reasoning
                        .first()
                        .map_or("Security critical", String::as_str)
                ));
            }
            lines.push(String::new());
        }

        // Validation checklist
        lines.push("## Validation Checklist".to_string());
        lines.push(String::new());
        lines.push("- [ ] All existing tests pass".to_string());
        lines.push("- [ ] No compilation warnings introduced".to_string());
        lines.push("- [ ] Performance benchmarks show no regression".to_string());
        lines.push("- [ ] Binary size reduced or neutral".to_string());
        lines.push("- [ ] Compile time reduced or neutral".to_string());
        lines.push("- [ ] Differential tests pass (if applicable)".to_string());
        lines.push(String::new());

        lines.push("---".to_string());
        lines.push(
            "*This report was auto-generated by Amber. Review carefully before merging.*"
                .to_string(),
        );

        lines.join("\n")
    }
}

impl Default for PrReporter {
    fn default() -> Self {
        Self::new()
    }
}

/// SARIF report output for integration with static-analysis tools.
pub struct SarifReporter;

impl SarifReporter {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Generate a SARIF report.
    ///
    /// # Errors
    ///
    /// Returns an error if the report cannot be serialized to JSON.
    pub fn generate_sarif(
        &self,
        deps: &[Dependency],
        usage: &HashMap<String, CrateUsage>,
        scores: &[ReplacementScore],
    ) -> Result<String> {
        let results: Vec<_> = deps
            .iter()
            .zip(scores.iter())
            .filter_map(|(dep, score)| {
                let usage_stats = usage.get(&dep.name).cloned().unwrap_or_default();
                if usage_stats.imported_items.is_empty() && usage_stats.call_sites.is_empty() {
                    // Unused dependency
                    return Some(json!({
                        "ruleId": "unused-dependency",
                        "level": "warning",
                        "message": {
                            "text": format!("Dependency '{}' is declared but not used in source code", dep.name)
                        },
                        "locations": []
                    }));
                }

                if score.overall >= 75 {
                    return Some(json!({
                        "ruleId": "replaceable-dependency",
                        "level": "note",
                        "message": {
                            "text": format!(
                                "Dependency '{}' is a candidate for replacement (score: {})",
                                dep.name, score.overall
                            )
                        },
                        "locations": usage_stats.call_sites.iter().map(|cs| {
                            json!({
                                "physicalLocation": {
                                    "artifactLocation": { "uri": cs.location.file },
                                    "region": {
                                        "startLine": cs.location.line,
                                        "startColumn": cs.location.column,
                                    }
                                }
                            })
                        }).collect::<Vec<_>>()
                    }));
                }

                None
            })
            .collect();

        let sarif = json!({
            "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "amber",
                        "version": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://github.com/elci-group/amber",
                        "rules": [
                            {
                                "id": "unused-dependency",
                                "name": "UnusedDependency",
                                "shortDescription": { "text": "A dependency is declared but never used in source code." },
                                "fullDescription": { "text": "Dependencies that are declared in Cargo.toml but have no imports or call sites in the project's source can be removed safely." },
                                "defaultConfiguration": { "level": "warning" },
                                "help": { "text": "Remove the unused dependency from Cargo.toml and run cargo check." }
                            },
                            {
                                "id": "replaceable-dependency",
                                "name": "ReplaceableDependency",
                                "shortDescription": { "text": "A dependency is a strong candidate for replacement or removal." },
                                "fullDescription": { "text": "Amber's safety classifier scored this dependency high enough that an automated or semi-automated replacement is recommended." },
                                "defaultConfiguration": { "level": "note" },
                                "help": { "text": "Review the generated replacement proposal and run the validation checklist before merging." }
                            }
                        ]
                    }
                },
                "results": results,
            }]
        });

        Ok(serde_json::to_string_pretty(&sarif)?)
    }
}

impl Default for SarifReporter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::types::{
        CallSite, DependencyKind, DependencySource, ImportedItem, ItemKind, Location, UsageKind,
    };
    use crate::replacement::generator::ReplacementProposal;
    use crate::replacement::validator::{StageResult, ValidationReport, ValidationStage};
    use crate::scoring::classifier::{ReplacementRecommendation, SafetyClass, ScoreDimensions};
    use std::time::Duration;

    fn sample_dep(name: &str) -> Dependency {
        Dependency {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            source: DependencySource::CratesIo,
            kind: DependencyKind::Normal,
            features: Vec::new(),
            optional: false,
            uses_default_features: true,
            transitive_deps: Vec::new(),
            loc_approx: 1000,
            public_api_count: 20,
            last_release: None,
            maintenance_score: 80,
            cve_count: 0,
            license: Some("MIT".to_string()),
            download_count: 1000,
        }
    }

    fn sample_score(overall: u8) -> ReplacementScore {
        ReplacementScore {
            overall,
            confidence: 90,
            classification: SafetyClass::SafeToReplace,
            dimensions: ScoreDimensions::default(),
            reasoning: vec!["Reason A".to_string()],
            recommendation: ReplacementRecommendation::Proceed,
        }
    }

    fn score_with(
        overall: u8,
        classification: SafetyClass,
        recommendation: ReplacementRecommendation,
    ) -> ReplacementScore {
        ReplacementScore {
            overall,
            confidence: 90,
            classification,
            dimensions: ScoreDimensions::default(),
            reasoning: vec!["Reason".to_string()],
            recommendation,
        }
    }

    fn sample_usage() -> CrateUsage {
        CrateUsage {
            crate_name: "sample".to_string(),
            imported_items: vec![ImportedItem {
                name: "helper".to_string(),
                kind: ItemKind::Function,
                path: "sample::helper".to_string(),
                location: Location::new("src/lib.rs", 1, 1),
            }],
            import_count: 1,
            call_sites: vec![CallSite {
                function_name: "helper".to_string(),
                kind: UsageKind::FunctionCall,
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
    fn json_report_contains_score_and_usage() {
        let dep = sample_dep("sample");
        let score = sample_score(85);
        let usage = sample_usage();
        let mut map = HashMap::new();
        map.insert(dep.name.clone(), usage);

        let json = JsonReporter::new()
            .generate_json(&[dep], &map, &[score])
            .unwrap();
        assert!(json.contains("\"crate\": \"sample\""));
        assert!(json.contains("\"overall\": 85"));
        assert!(json.contains("\"confidence\": 90"));
        assert!(json.contains("\"recommendation\": \"proceed\""));
    }

    #[test]
    fn pr_report_contains_summary_and_proposed_replacements() {
        let dep = sample_dep("sample");
        let score = sample_score(85);
        let usage = sample_usage();
        let mut map = HashMap::new();
        map.insert(dep.name.clone(), usage);

        let body = PrReporter::new().generate_pr_body(&[dep], &map, &[score], 75);
        assert!(body.contains("# Amber Dependency Reduction Report"));
        assert!(body.contains("Total dependencies analyzed:** 1"));
        assert!(body.contains("Above threshold (75):** 1"));
        assert!(body.contains("### `sample`"));
        assert!(body.contains("Validation Checklist"));
    }

    #[test]
    fn sarif_report_flags_replaceable_dependency() {
        let dep = sample_dep("sample");
        let score = sample_score(85);
        let usage = sample_usage();
        let mut map = HashMap::new();
        map.insert(dep.name.clone(), usage);

        let sarif = SarifReporter::new()
            .generate_sarif(&[dep], &map, &[score])
            .unwrap();
        assert!(sarif.contains("\"ruleId\": \"replaceable-dependency\""));
        assert!(sarif.contains("sample"));
        assert!(sarif.contains("\"version\": \"2.1.0\""));
    }

    #[test]
    fn sarif_report_flags_unused_dependency() {
        let dep = sample_dep("unused");
        let score = sample_score(95);
        let usage = CrateUsage::default();
        let mut map = HashMap::new();
        map.insert(dep.name.clone(), usage);

        let sarif = SarifReporter::new()
            .generate_sarif(&[dep], &map, &[score])
            .unwrap();
        assert!(sarif.contains("\"ruleId\": \"unused-dependency\""));
    }

    #[test]
    fn json_report_covers_all_recommendations() {
        let usage = sample_usage();
        let mut map = HashMap::new();
        map.insert("sample".to_string(), usage);

        for (rec, expected) in [
            (ReplacementRecommendation::Proceed, "proceed"),
            (ReplacementRecommendation::Propose, "propose"),
            (ReplacementRecommendation::Caution, "caution"),
            (ReplacementRecommendation::Block, "block"),
            (ReplacementRecommendation::SecurityBlock, "security_block"),
        ] {
            let dep = sample_dep("sample");
            let score = score_with(50, SafetyClass::MediumRisk, rec);
            let json = JsonReporter::new()
                .generate_json(&[dep], &map, &[score])
                .unwrap();
            assert!(
                json.contains(&format!("\"recommendation\": \"{expected}\"")),
                "missing {expected} in {json}"
            );
        }
    }

    #[test]
    fn pr_report_lists_security_critical() {
        let dep = sample_dep("crypto");
        let mut score = sample_score(95);
        score.classification = SafetyClass::SecurityCritical;
        score.recommendation = ReplacementRecommendation::SecurityBlock;
        let usage = sample_usage();
        let mut map = HashMap::new();
        map.insert(dep.name.clone(), usage);

        let body = PrReporter::new().generate_pr_body(&[dep], &map, &[score], 75);
        assert!(body.contains("## Security Critical (Do Not Replace)"));
        assert!(body.contains("crypto"));
    }

    #[test]
    fn pr_report_lists_transitive_deps_removed() {
        let mut dep = sample_dep("sample");
        dep.transitive_deps = vec!["a".to_string(), "b".to_string()];
        let score = sample_score(85);
        let usage = sample_usage();
        let mut map = HashMap::new();
        map.insert(dep.name.clone(), usage);

        let body = PrReporter::new().generate_pr_body(&[dep], &map, &[score], 75);
        assert!(body.contains("Transitive deps removed:** 2"));
    }

    #[test]
    fn sarif_report_omits_below_threshold() {
        let dep = sample_dep("sample");
        let score = sample_score(50);
        let usage = sample_usage();
        let mut map = HashMap::new();
        map.insert(dep.name.clone(), usage);

        let sarif = SarifReporter::new()
            .generate_sarif(&[dep], &map, &[score])
            .unwrap();
        assert!(!sarif.contains("\"ruleId\": \"replaceable-dependency\""));
        assert!(!sarif.contains("\"ruleId\": \"unused-dependency\""));
    }

    #[test]
    fn sarif_report_includes_rule_metadata() {
        let dep = sample_dep("sample");
        let score = sample_score(85);
        let usage = sample_usage();
        let mut map = HashMap::new();
        map.insert(dep.name.clone(), usage);

        let sarif = SarifReporter::new()
            .generate_sarif(&[dep], &map, &[score])
            .unwrap();
        assert!(sarif.contains("\"rules\""));
        assert!(sarif.contains("\"id\": \"unused-dependency\""));
        assert!(sarif.contains("\"id\": \"replaceable-dependency\""));
        assert!(sarif.contains("\"name\": \"UnusedDependency\""));
        assert!(sarif.contains("\"name\": \"ReplaceableDependency\""));
        assert!(sarif.contains("shortDescription"));
        assert!(sarif.contains("fullDescription"));
        assert!(sarif.contains("defaultConfiguration"));
        assert!(sarif.contains("\"help\""));
        assert!(sarif.contains("A dependency is declared but never used"));
        assert!(sarif.contains("Remove the unused dependency from Cargo.toml"));
        assert!(sarif.contains("Review the generated replacement proposal"));
    }

    fn sample_proposal() -> ReplacementProposal {
        ReplacementProposal {
            original_crate: "sample".to_string(),
            replacement_module: "amber_sample".to_string(),
            replacement_code: "pub fn demo() {}".to_string(),
            estimated_compile_time_reduction: "-5%".to_string(),
            estimated_binary_size_reduction: "-10KB".to_string(),
            validation_strategy: vec!["Run tests".to_string()],
            risk_notes: vec!["Low risk".to_string()],
            test_plan: vec!["Check call sites".to_string()],
            validation_report: Some(ValidationReport {
                success: true,
                stages: vec![StageResult {
                    stage: ValidationStage::Check,
                    passed: true,
                    stderr: String::new(),
                    duration: Duration::ZERO,
                }],
                stderr_summary: String::new(),
                duration: Duration::ZERO,
            }),
            source: None,
        }
    }

    #[allow(dead_code)]
    fn sample_directive() -> TechnicalDirective {
        TechnicalDirective {
            project_name: "sample_project".to_string(),
            manifest_path: std::path::PathBuf::from("/tmp/sample/Cargo.toml"),
            target_crate: "anyhow".to_string(),
            target_version: "1.0.0".to_string(),
            safety_classification: "Safe to Replace".to_string(),
            safety_score: 85,
            confidence: 90,
            rationale: vec!["Low API surface".to_string()],
            scope_summary: "Replace anyhow usage in sample_project.".to_string(),
            affected_files: vec!["src/lib.rs".to_string()],
            used_in_public_api: false,
            transitive_dependency_count: 2,
            dependency_kind: "Normal".to_string(),
            msrv: Some("1.80".to_string()),
            implementation_steps: vec!["Audit usage".to_string()],
            acceptance_criteria: vec!["Tests pass".to_string()],
            testing_plan: vec!["Run tests".to_string()],
            rollback_plan: vec!["Revert Cargo.toml".to_string()],
            risk_notes: vec!["Low risk".to_string()],
            estimated_impact: crate::replacement::directives::EstimatedImpact {
                compile_time_reduction: "-2-5%".to_string(),
                binary_size_reduction: "-30KB-100KB".to_string(),
            },
        }
    }

    #[test]
    fn console_reporter_prints_dependency_list() {
        let reporter = ConsoleReporter::new();
        reporter.print_dependency_list(&[sample_dep("sample")]);
    }

    #[test]
    fn console_reporter_prints_score_card() {
        let reporter = ConsoleReporter::new();
        reporter.print_score_card("sample", &sample_score(85), &sample_usage());
    }

    #[test]
    fn console_reporter_prints_full_report() {
        let reporter = ConsoleReporter::new();
        let dep = sample_dep("sample");
        let score = sample_score(85);
        let usage = sample_usage();
        let mut map = HashMap::new();
        map.insert(dep.name.clone(), usage);
        reporter.print_full_report(&[dep], &map, &[score], 75);
    }

    #[test]
    fn console_reporter_prints_replacement_proposal() {
        let reporter = ConsoleReporter::new();
        reporter.print_replacement_proposal(&sample_proposal());
    }

    #[test]
    fn console_reporter_prints_failed_validation_in_red() {
        let reporter = ConsoleReporter::new();
        let mut proposal = sample_proposal();
        if let Some(report) = &mut proposal.validation_report {
            report.success = false;
        }
        reporter.print_replacement_proposal(&proposal);
    }

    #[test]
    #[allow(clippy::default_constructed_unit_structs)]
    fn reporter_defaults_match_new() {
        ConsoleReporter::default().print_dependency_list(&[sample_dep("sample")]);

        assert_eq!(
            JsonReporter::default()
                .generate_json(
                    &[sample_dep("sample")],
                    &HashMap::new(),
                    &[sample_score(85)]
                )
                .unwrap(),
            JsonReporter::new()
                .generate_json(
                    &[sample_dep("sample")],
                    &HashMap::new(),
                    &[sample_score(85)]
                )
                .unwrap()
        );
        assert_eq!(
            PrReporter::default().generate_pr_body(
                &[sample_dep("sample")],
                &HashMap::new(),
                &[sample_score(85)],
                75
            ),
            PrReporter::new().generate_pr_body(
                &[sample_dep("sample")],
                &HashMap::new(),
                &[sample_score(85)],
                75
            )
        );
        assert_eq!(
            SarifReporter::default()
                .generate_sarif(
                    &[sample_dep("sample")],
                    &HashMap::new(),
                    &[sample_score(85)]
                )
                .unwrap(),
            SarifReporter::new()
                .generate_sarif(
                    &[sample_dep("sample")],
                    &HashMap::new(),
                    &[sample_score(85)]
                )
                .unwrap()
        );
    }

    #[test]
    fn console_reporter_covers_all_score_bars() {
        let reporter = ConsoleReporter::new();
        for score in [10, 35, 60, 90] {
            reporter.print_score_card("sample", &sample_score(score), &sample_usage());
        }
    }

    #[test]
    fn console_reporter_prints_all_recommendations() {
        let reporter = ConsoleReporter::new();
        let deps_and_scores = [
            (
                "safe",
                score_with(
                    90,
                    SafetyClass::SafeToReplace,
                    ReplacementRecommendation::Proceed,
                ),
            ),
            (
                "low",
                score_with(70, SafetyClass::LowRisk, ReplacementRecommendation::Propose),
            ),
            (
                "medium",
                score_with(
                    50,
                    SafetyClass::MediumRisk,
                    ReplacementRecommendation::Caution,
                ),
            ),
            (
                "high",
                score_with(25, SafetyClass::HighRisk, ReplacementRecommendation::Block),
            ),
            (
                "critical",
                score_with(
                    10,
                    SafetyClass::SecurityCritical,
                    ReplacementRecommendation::SecurityBlock,
                ),
            ),
        ];
        let deps: Vec<_> = deps_and_scores
            .iter()
            .map(|(name, _)| sample_dep(name))
            .collect();
        let scores: Vec<_> = deps_and_scores
            .iter()
            .map(|(_, score)| score.clone())
            .collect();
        let map = HashMap::new();
        reporter.print_full_report(&deps, &map, &scores, 50);
    }

    #[test]
    fn console_reporter_prints_all_score_cards() {
        let reporter = ConsoleReporter::new();
        for (class, rec) in [
            (
                SafetyClass::SafeToReplace,
                ReplacementRecommendation::Proceed,
            ),
            (SafetyClass::LowRisk, ReplacementRecommendation::Propose),
            (SafetyClass::MediumRisk, ReplacementRecommendation::Caution),
            (SafetyClass::HighRisk, ReplacementRecommendation::Block),
            (
                SafetyClass::SecurityCritical,
                ReplacementRecommendation::SecurityBlock,
            ),
        ] {
            reporter.print_score_card("sample", &score_with(50, class, rec), &sample_usage());
        }
    }

    #[test]
    fn emoji_reporter_prints_dependency_list() {
        let reporter = EmojiReporter::new();
        reporter.print_dependency_list(&[sample_dep("sample"), sample_dep("other")]);
    }

    #[test]
    fn emoji_reporter_prints_score_card() {
        let reporter = EmojiReporter::new();
        reporter.print_score_card("sample", &sample_score(85), &sample_usage());
    }

    #[test]
    fn emoji_reporter_prints_full_report() {
        let reporter = EmojiReporter::new();
        let dep = sample_dep("sample");
        let score = sample_score(85);
        let usage = sample_usage();
        let mut map = HashMap::new();
        map.insert(dep.name.clone(), usage);
        reporter.print_full_report(&[dep], &map, &[score], 75);
    }

    #[test]
    fn emoji_reporter_prints_replacement_proposal() {
        let reporter = EmojiReporter::new();
        reporter.print_replacement_proposal(&sample_proposal());
    }

    #[test]
    fn emoji_reporter_prints_failed_validation() {
        let reporter = EmojiReporter::new();
        let mut proposal = sample_proposal();
        if let Some(report) = &mut proposal.validation_report {
            report.success = false;
        }
        reporter.print_replacement_proposal(&proposal);
    }

    #[test]
    fn emoji_reporter_prints_all_score_cards() {
        let reporter = EmojiReporter::new();
        for (class, rec) in [
            (
                SafetyClass::SafeToReplace,
                ReplacementRecommendation::Proceed,
            ),
            (SafetyClass::LowRisk, ReplacementRecommendation::Propose),
            (SafetyClass::MediumRisk, ReplacementRecommendation::Caution),
            (SafetyClass::HighRisk, ReplacementRecommendation::Block),
            (
                SafetyClass::SecurityCritical,
                ReplacementRecommendation::SecurityBlock,
            ),
        ] {
            reporter.print_score_card("sample", &score_with(50, class, rec), &sample_usage());
        }
    }

    #[test]
    fn emoji_reporter_prints_all_recommendations() {
        let reporter = EmojiReporter::new();
        let deps_and_scores = [
            (
                "safe",
                score_with(
                    90,
                    SafetyClass::SafeToReplace,
                    ReplacementRecommendation::Proceed,
                ),
            ),
            (
                "low",
                score_with(70, SafetyClass::LowRisk, ReplacementRecommendation::Propose),
            ),
            (
                "medium",
                score_with(
                    50,
                    SafetyClass::MediumRisk,
                    ReplacementRecommendation::Caution,
                ),
            ),
            (
                "high",
                score_with(25, SafetyClass::HighRisk, ReplacementRecommendation::Block),
            ),
            (
                "critical",
                score_with(
                    10,
                    SafetyClass::SecurityCritical,
                    ReplacementRecommendation::SecurityBlock,
                ),
            ),
            (
                "blocked",
                score_with(
                    5,
                    SafetyClass::DoNotReplace,
                    ReplacementRecommendation::Block,
                ),
            ),
        ];
        let deps: Vec<_> = deps_and_scores
            .iter()
            .map(|(name, _)| sample_dep(name))
            .collect();
        let scores: Vec<_> = deps_and_scores
            .iter()
            .map(|(_, score)| score.clone())
            .collect();
        let map = HashMap::new();
        reporter.print_full_report(&deps, &map, &scores, 50);
    }

    #[test]
    #[allow(clippy::default_constructed_unit_structs)]
    fn emoji_reporter_default_matches_new() {
        EmojiReporter::default().print_dependency_list(&[sample_dep("sample")]);
    }

    #[test]
    fn emoji_reporter_covers_score_bars() {
        let reporter = EmojiReporter::new();
        for score in [0, 5, 25, 50, 75, 100] {
            reporter.print_score_card("sample", &sample_score(score), &sample_usage());
        }
    }
}
