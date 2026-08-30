// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! The default `analyze` command and full-repository analysis flow.
use crate::amber_anyhow::Result;

use crate::analysis::usage::UsageAnalyzer;
use crate::cli::paths::validate_output_path;
use crate::cli::{build_analyzer, build_classifier, load_config, Cli, OutputFormat};
#[cfg(feature = "library")]
use crate::cli::{open_library, use_library};
use crate::replacement::Generator;
use crate::reporting::formatters::{ConsoleReporter, EmojiReporter, JsonReporter, PrReporter};
use crate::reporting::style::Colorize;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use tracing::{debug, info, info_span, warn};

/// Run a full analysis and emit the requested report.
///
/// # Errors
///
/// Returns an error if `output_path` escapes the target project root, or if
/// analysis, scoring, or report generation fails.
#[allow(clippy::too_many_lines)]
pub fn run(cli: &Cli, manifest_path: &Path, output_path: Option<&Path>) -> Result<i32> {
    let output_path = output_path
        .map(|path| validate_output_path(&cli.path, path))
        .transpose()?;
    let config = load_config(cli, manifest_path)?;
    let threshold = config.threshold.unwrap_or(cli.threshold);

    // For machine-readable formats all diagnostics go to stderr so stdout
    // carries only the report payload.
    let machine_output = matches!(
        cli.output_format(),
        OutputFormat::Json | OutputFormat::Pr | OutputFormat::Sarif
    );
    macro_rules! diag {
        ($($arg:tt)*) => {
            if machine_output { eprintln!($($arg)*); } else { println!($($arg)*); }
        };
    }

    // Phase 1: Repository Analysis
    let _phase = info_span!("phase", phase = "index").entered();
    info!(phase = "index", manifest = %manifest_path.display(), "indexing repository dependencies");
    let analyzer = build_analyzer(manifest_path, cli)?;
    let deps = analyzer.list_dependencies(cli.transitive, !cli.no_dev)?;

    if deps.is_empty() {
        diag!(
            "{}",
            "No dependencies found. Is this a Rust project?".yellow()
        );
        return Ok(0);
    }

    diag!(
        "  {} {} dependencies indexed",
        "✓".green(),
        deps.len().to_string().bold()
    );

    // Phase 2: Usage Analysis
    let _phase = info_span!("phase", phase = "usage").entered();
    info!(
        phase = "usage",
        dependency_count = deps.len(),
        "analyzing contextual usage"
    );
    let usage_analyzer = if let Some(snapshot_path) = &cli.snapshot {
        UsageAnalyzer::with_snapshot(manifest_path, snapshot_path)?
    } else {
        UsageAnalyzer::new(manifest_path)?
    };
    let usage_stats = usage_analyzer.analyze_all_usage(&deps)?;
    diag!(
        "  {} Usage patterns extracted for {} crates",
        "✓".green(),
        usage_stats.len().to_string().bold()
    );

    // Phase 3: Safety Classification
    let _phase = info_span!("phase", phase = "classify").entered();
    info!(
        phase = "classify",
        dependency_count = deps.len(),
        "classifying replacement safety"
    );
    let classifier = build_classifier(&config);
    let scores: Vec<_> = deps
        .iter()
        .map(|dep| {
            let usage = usage_stats.get(&dep.name).cloned().unwrap_or_default();
            classifier.score_dependency(dep, &usage)
        })
        .collect();
    diag!("  {} Safety scores computed", "✓".green());

    // Phase 4: Generate report
    let _phase = info_span!("phase", phase = "report").entered();
    info!(phase = "report", format = ?cli.output_format(), "generating report");
    match cli.output_format() {
        OutputFormat::Console => {
            let reporter = ConsoleReporter::new();
            reporter.print_full_report(&deps, &usage_stats, &scores, threshold);
        }
        OutputFormat::Json => {
            let reporter = JsonReporter::new();
            let json = reporter.generate_json(&deps, &usage_stats, &scores)?;
            if let Some(path) = &output_path {
                std::fs::write(path, json)?;
                diag!("\n{} Report written to {}", "✓".green(), path.display());
            } else {
                println!("{json}");
            }
        }
        OutputFormat::Pr => {
            let reporter = PrReporter::new();
            let pr_body = reporter.generate_pr_body(&deps, &usage_stats, &scores, threshold);
            if let Some(path) = &output_path {
                std::fs::write(path, pr_body)?;
                diag!(
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
            if let Some(path) = &output_path {
                std::fs::write(path, sarif)?;
                diag!(
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
        info!(
            phase = "proposals",
            threshold, "generating replacement proposals"
        );
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
                Ok(ref proposal) => {
                    generated += 1;
                    info!(crate = %dep.name, output_dir = %generator.output_dir().display(), "replacement proposal generated");
                    if cli.swot && matches!(cli.output_format(), OutputFormat::Console) {
                        let swot =
                            crate::scoring::swot::analyze(dep, &usage, score, Some(proposal));
                        ConsoleReporter::new().print_swot(&dep.name, &swot);
                    }
                }
                Err(ref e) if e.to_string().starts_with("unsupported crate") => {
                    skipped_unsupported += 1;
                    debug!(crate = %dep.name, error = %e, "replacement template is unavailable");
                }
                Err(e) => {
                    validation_failed += 1;
                    warn!(error = %e, "Failed to generate replacement for {}", dep.name);
                }
            }
        }

        println!();
        diag!(
            "  {} {} replacement(s) generated in {}",
            "✓".green(),
            generated.to_string().bold(),
            generator.output_dir().display().to_string().cyan()
        );
        if skipped_unsupported > 0 {
            diag!(
                "  {} {} crate(s) skipped (unsupported)",
                "⊘".yellow(),
                skipped_unsupported.to_string().bold()
            );
        }
        if validation_failed > 0 {
            diag!(
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
            diag!("  {}", "Policy Violations".red().bold());
            for v in &violations {
                diag!("    {} {}", "✗".red(), v.message());
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
