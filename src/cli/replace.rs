//! The `replace` subcommand.
use crate::amber_anyhow::Result;

use crate::analysis::usage::UsageAnalyzer;
use crate::cli::paths::validate_output_path;
use crate::cli::{build_analyzer, build_classifier, load_config, Cli};
#[cfg(feature = "library")]
use crate::cli::{open_library, use_library};
use crate::replacement::Generator;
use crate::reporting::formatters::ConsoleReporter;
use std::path::Path;
use tracing::{info, warn};

/// Run the `replace` command for `crate_name`, writing output to `out_dir`.
///
/// # Errors
///
/// Returns an error if `out_dir` escapes the target project root, the
/// dependency cannot be analyzed, or the replacement cannot be generated.
pub fn run(cli: &Cli, manifest_path: &Path, crate_name: &str, out_dir: &Path) -> Result<i32> {
    info!("Generating replacement for: {crate_name}");
    let out_dir = validate_output_path(&cli.path, out_dir)?;
    Generator::validate_crate_name(crate_name)?;
    let config = load_config(cli, manifest_path)?;
    let analyzer = build_analyzer(manifest_path, cli)?;
    let dep = analyzer.get_dependency(crate_name)?;
    let usage = UsageAnalyzer::new(manifest_path)?;
    let usage_stats = usage.analyze_crate_usage(crate_name)?;
    let classifier = build_classifier(&config);
    let score = classifier.score_dependency(&dep, &usage_stats);

    let threshold = config.threshold.unwrap_or(cli.threshold);

    if score.overall < threshold {
        warn!(
            "Score ({}) below threshold ({}). Use --threshold to override.",
            score.overall, threshold
        );
        return Ok(0);
    }

    let generator = Generator::new(out_dir);
    #[cfg(feature = "library")]
    let proposal = if use_library(cli, &config) {
        let mut library_store = open_library(&config)?;
        generator.generate_replacement_with_library(
            crate_name,
            &usage_stats,
            &score,
            &mut library_store,
        )?
    } else {
        generator.generate_replacement(crate_name, &usage_stats, &score)?
    };
    #[cfg(not(feature = "library"))]
    let proposal = generator.generate_replacement(crate_name, &usage_stats, &score)?;
    let reporter = ConsoleReporter::new();
    reporter.print_replacement_proposal(&proposal);
    Ok(0)
}
