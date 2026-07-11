//! The `list` subcommand.
use crate::amber_anyhow::Result;

use crate::cli::{build_analyzer, Cli, OutputFormat};
use crate::reporting::formatters::{ConsoleReporter, EmojiReporter};
use std::path::Path;
use tracing::info;

/// Run the `list` command.
///
/// # Errors
///
/// Returns an error if dependency analysis fails.
pub fn run(cli: &Cli, manifest_path: &Path) -> Result<i32> {
    info!("Listing all dependencies...");
    let analyzer = build_analyzer(manifest_path, cli)?;
    let deps = analyzer.list_dependencies(cli.transitive, !cli.no_dev)?;
    if matches!(cli.output_format(), OutputFormat::Emoji) {
        EmojiReporter::new().print_dependency_list(&deps);
    } else {
        ConsoleReporter::new().print_dependency_list(&deps);
    }
    Ok(0)
}
