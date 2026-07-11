//! The `score` subcommand.
use crate::amber_anyhow::Result;

use crate::analysis::usage::UsageAnalyzer;
use crate::cli::{build_analyzer, build_classifier, load_config, Cli, OutputFormat};
use crate::reporting::formatters::{ConsoleReporter, EmojiReporter};
use std::path::Path;
use tracing::info;

/// Run the `score` command for `crate_name`.
///
/// # Errors
///
/// Returns an error if the dependency cannot be found or usage analysis fails.
pub fn run(cli: &Cli, manifest_path: &Path, crate_name: &str) -> Result<i32> {
    info!("Scoring dependency: {crate_name}");
    let config = load_config(cli, manifest_path)?;
    let analyzer = build_analyzer(manifest_path, cli)?;
    let dep = analyzer.get_dependency(crate_name)?;
    let usage = UsageAnalyzer::new(manifest_path)?;
    let usage_stats = usage.analyze_crate_usage(crate_name)?;
    let classifier = build_classifier(&config);
    let score = classifier.score_dependency(&dep, &usage_stats);
    if matches!(cli.output_format(), OutputFormat::Emoji) {
        EmojiReporter::new().print_score_card(crate_name, &score, &usage_stats);
    } else {
        ConsoleReporter::new().print_score_card(crate_name, &score, &usage_stats);
    }
    Ok(0)
}
