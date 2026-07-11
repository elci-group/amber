//! The `library` subcommand (requires the `library` feature).

use crate::amber_anyhow::{Context, Result};
use crate::cli::{load_config, open_library, Cli};
use crate::reporting::style::Colorize;
use std::path::Path;
use tracing::info;

/// Run a library management command.
///
/// # Errors
///
/// Returns an error if the library cannot be opened or the command fails.
pub fn run(cli: &Cli, manifest_path: &Path, command: &super::LibraryCommands) -> Result<i32> {
    let config = load_config(cli, manifest_path)?;
    let mut store = open_library(&config)?;

    match command {
        super::LibraryCommands::List => {
            let entries = store.list();
            if entries.is_empty() {
                println!("  No replacement modules stored in the library.");
            } else {
                println!("  Stored replacement modules ({}):", entries.len());
                for entry in &entries {
                    println!(
                        "    - {} ({}): {} [{}]",
                        entry.crate_name,
                        entry.module_name,
                        entry.code.len(),
                        entry.source.as_str()
                    );
                }
            }
        }
        super::LibraryCommands::Import { path, crate_name } => {
            let code = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read module from {}", path.display()))?;
            let module_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("imported")
                .to_string();
            let entry = crate::library::LibraryEntry::new(
                crate_name.clone(),
                module_name,
                code,
                crate::library::EntrySource::Imported,
            );
            store.insert(&entry)?;
            println!("  Imported replacement for `{crate_name}` into the library.");
        }
        super::LibraryCommands::Export { crate_name, out } => {
            let entry = store
                .find(crate_name)
                .ok_or_else(|| crate::anyhow!("No library entry for `{crate_name}`"))?;
            std::fs::write(out, &entry.code)
                .with_context(|| format!("Failed to write module to {}", out.display()))?;
            println!(
                "  Exported replacement for `{crate_name}` to {}.",
                out.display()
            );
        }
        super::LibraryCommands::Path => {
            let path = config
                .library
                .as_ref()
                .map_or_else(crate::config::LibraryConfig::default, Clone::clone)
                .resolved_path();
            println!("{}", path.display());
        }
        super::LibraryCommands::Search { query } => {
            info!("Searching library for `{query}`");
            let matches = store.search(query);
            if matches.is_empty() {
                println!("  No library entries matching `{query}`.");
            } else {
                println!(
                    "  Found {} matching entr{}:",
                    matches.len(),
                    if matches.len() == 1 { "y" } else { "ies" }
                );
                for entry in &matches {
                    println!(
                        "    - {} ({}) [{}]",
                        entry.crate_name.cyan().bold(),
                        entry.module_name,
                        entry.source.as_str()
                    );
                }
            }
        }
        super::LibraryCommands::Remove { crate_name } => {
            info!("Removing `{crate_name}` from library");
            if store.remove(crate_name)? {
                println!("  Removed replacement for `{crate_name}` from the library.");
            } else {
                println!("  No library entry for `{crate_name}` to remove.");
            }
        }
    }

    Ok(0)
}
