//! The `directives` subcommand.
use crate::amber_anyhow::Result;

use crate::analysis::usage::UsageAnalyzer;
use crate::cli::paths::validate_output_path;
use crate::cli::{build_analyzer, build_classifier, load_config, Cli};
use crate::replacement::{DirectiveContext, DirectiveGenerator};
use crate::reporting::style::Colorize;
use std::path::Path;
use tracing::info;

/// Run the `directives` command for `crate_name`.
///
/// # Errors
///
/// Returns an error if `output` escapes the target project root, the
/// dependency cannot be analyzed, or the directive cannot be written.
pub fn run(
    cli: &Cli,
    manifest_path: &Path,
    crate_name: &str,
    output: Option<&Path>,
) -> Result<i32> {
    info!("Generating technical directive for: {crate_name}");
    let config = load_config(cli, manifest_path)?;
    let analyzer = build_analyzer(manifest_path, cli)?;
    let dep = analyzer.get_dependency(crate_name)?;
    let usage = UsageAnalyzer::new(manifest_path)?;
    let usage_stats = usage.analyze_crate_usage(crate_name)?;
    let classifier = build_classifier(&config);
    let score = classifier.score_dependency(&dep, &usage_stats);

    let project_name = manifest_path
        .parent()
        .and_then(|p| p.file_name())
        .map_or_else(
            || "unknown".to_string(),
            |n| n.to_string_lossy().to_string(),
        );
    let context = DirectiveContext {
        project_name,
        manifest_path: manifest_path.to_path_buf(),
        msrv: None,
    };

    let directive = DirectiveGenerator::generate(&dep, &usage_stats, &score, &context);
    let markdown = directive.to_markdown();

    if let Some(path) = output {
        let path = validate_output_path(&cli.path, path)?;
        std::fs::write(&path, &markdown)?;
        println!("\n{} Directive written to {}", "✓".green(), path.display());
    } else {
        println!("{markdown}");
    }

    Ok(0)
}
