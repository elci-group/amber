//! The default `analyze` command and full-repository analysis flow.
use crate::amber_anyhow::Result;

use crate::analysis::usage::UsageAnalyzer;
use crate::cli::{build_analyzer, build_classifier, load_config, Cli, OutputFormat};
#[cfg(feature = "library")]
use crate::cli::{open_library, use_library};
use crate::replacement::Generator;
use crate::reporting::formatters::{ConsoleReporter, EmojiReporter, JsonReporter, PrReporter};
use crate::reporting::style::Colorize;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use tracing::{info, info_span, warn};

/// Run a full analysis and emit the requested report.
///
/// # Errors
///
/// Returns an error if analysis, scoring, or report generation fails.
#[allow(clippy::too_many_lines)]
pub fn run(cli: &Cli, manifest_path: &Path, output_path: Option<&Path>) -> Result<i32> {
    let config = load_config(cli, manifest_path)?;
    let threshold = config.threshold.unwrap_or(cli.threshold);

    // Phase 1: Repository Analysis
    let _phase = info_span!("phase", phase = "index").entered();
    info!("Phase 1: Indexing repository dependencies...");
    let analyzer = build_analyzer(manifest_path, cli)?;
    let deps = analyzer.list_dependencies(cli.transitive, !cli.no_dev)?;

    if deps.is_empty() {
        println!(
            "{}",
            "No dependencies found. Is this a Rust project?".yellow()
        );
        return Ok(0);
    }

    println!(
        "  {} {} dependencies indexed",
        "✓".green(),
        deps.len().to_string().bold()
    );

    // Phase 2: Usage Analysis
    let _phase = info_span!("phase", phase = "usage").entered();
    info!("Phase 2: Analyzing contextual usage...");
    let usage_analyzer = UsageAnalyzer::new(manifest_path)?;
    let usage_stats = usage_analyzer.analyze_all_usage(&deps)?;
    println!(
        "  {} Usage patterns extracted for {} crates",
        "✓".green(),
        usage_stats.len().to_string().bold()
    );

    // Phase 3: Safety Classification
    let _phase = info_span!("phase", phase = "classify").entered();
    info!("Phase 3: Classifying replacement safety...");
    let classifier = build_classifier(&config);
    let scores: Vec<_> = deps
        .iter()
        .map(|dep| {
            let usage = usage_stats.get(&dep.name).cloned().unwrap_or_default();
            classifier.score_dependency(dep, &usage)
        })
        .collect();
    println!("  {} Safety scores computed", "✓".green());

    // Phase 4: Generate report
    let _phase = info_span!("phase", phase = "report").entered();
    info!("Phase 4: Generating report...");
    match cli.output_format() {
        OutputFormat::Console => {
            let reporter = ConsoleReporter::new();
            reporter.print_full_report(&deps, &usage_stats, &scores, threshold);
        }
        OutputFormat::Json => {
            let reporter = JsonReporter::new();
            let json = reporter.generate_json(&deps, &usage_stats, &scores)?;
            if let Some(path) = output_path {
                std::fs::write(path, json)?;
                println!("\n{} Report written to {}", "✓".green(), path.display());
            } else {
                println!("{json}");
            }
        }
        OutputFormat::Pr => {
            let reporter = PrReporter::new();
            let pr_body = reporter.generate_pr_body(&deps, &usage_stats, &scores, threshold);
            if let Some(path) = output_path {
                std::fs::write(path, pr_body)?;
                println!(
                    "\n{} PR description written to {}",
                    "✓".green(),
                    path.display()
                );
            } else {
                println!("{pr_body}");
            }
        }
        OutputFormat::Sarif => {
            let reporter = crate::reporting::formatters::SarifReporter::new();
            let sarif = reporter.generate_sarif(&deps, &usage_stats, &scores)?;
            if let Some(path) = output_path {
                std::fs::write(path, sarif)?;
                println!(
                    "\n{} SARIF report written to {}",
                    "✓".green(),
                    path.display()
                );
            } else {
                println!("{sarif}");
            }
        }
        OutputFormat::Emoji => {
            let reporter = EmojiReporter::new();
            reporter.print_full_report(&deps, &usage_stats, &scores, threshold);
        }
    }

    // Phase 5: Replacement proposals (if requested)
    if cli.propose {
        let _phase = info_span!("phase", phase = "proposals").entered();
        info!("Phase 5: Generating replacement proposals...");
        let generator = Generator::new(PathBuf::from("amber_proposals"));
        #[cfg(feature = "library")]
        let mut library_store = if use_library(cli, &config) {
            Some(open_library(&config)?)
        } else {
            None
        };

        let mut generated = 0usize;
        let mut skipped_unsupported = 0usize;
        let mut validation_failed = 0usize;

        for (dep, score) in deps.iter().zip(scores.iter()) {
            if score.overall < threshold {
                continue;
            }

            let _crate_span = info_span!("generate_proposal", crate = %dep.name).entered();
            let usage = usage_stats.get(&dep.name).cloned().unwrap_or_default();
            info!(score = score.overall, "Generating replacement proposal");

            #[cfg(feature = "library")]
            let result = library_store.as_mut().map_or_else(
                || generator.generate_replacement(&dep.name, &usage, score),
                |store| {
                    generator.generate_replacement_with_library(&dep.name, &usage, score, store)
                },
            );
            #[cfg(not(feature = "library"))]
            let result = generator.generate_replacement(&dep.name, &usage, score);

            match result {
                Ok(_) => {
                    generated += 1;
                    info!("Replacement proposal generated for {}", dep.name);
                }
                Err(ref e) if e.to_string().starts_with("unsupported crate") => {
                    skipped_unsupported += 1;
                    // The generator already emits a structured warning for
                    // unsupported crates; avoid logging twice.
                }
                Err(e) => {
                    validation_failed += 1;
                    warn!(error = %e, "Failed to generate replacement for {}", dep.name);
                }
            }
        }

        println!();
        println!(
            "  {} {} replacement(s) generated in {}",
            "✓".green(),
            generated.to_string().bold(),
            generator.output_dir().display().to_string().cyan()
        );
        if skipped_unsupported > 0 {
            println!(
                "  {} {} crate(s) skipped (unsupported)",
                "⊘".yellow(),
                skipped_unsupported.to_string().bold()
            );
        }
        if validation_failed > 0 {
            println!(
                "  {} {} proposal(s) failed validation",
                "✗".red(),
                validation_failed.to_string().bold()
            );
        }
    }

    // Phase 6: Policy enforcement
    let mut exit_code = 0;
    if let Some(policy) = &config.policy {
        let dep_names: HashSet<String> = deps.iter().map(|d| d.name.clone()).collect();
        let violations = policy.validate(&dep_names);
        if !violations.is_empty() {
            println!();
            println!("  {}", "Policy Violations".red().bold());
            for v in &violations {
                println!("    {} {}", "✗".red(), v.message());
            }
            if policy.strict {
                exit_code = 2;
            } else if exit_code == 0 {
                exit_code = 1;
            }
        }
    }

    // Non-zero exit if there are actionable findings above threshold
    if exit_code == 0 {
        let actionable = scores.iter().filter(|s| s.overall >= threshold).count();
        if actionable > 0 {
            exit_code = 1;
        }
    }

    Ok(exit_code)
}
