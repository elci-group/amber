//! Amber CLI.
//!
//! This module contains the command-line interface for Amber. The binary in
//! `src/main.rs` is a thin wrapper around [`run_cli`].

#![warn(clippy::pedantic, clippy::nursery)]

pub mod analyze;
pub mod directives;
#[cfg(feature = "library")]
pub mod library;
pub mod list;
pub mod paths;
pub mod replace;
pub mod roadmap;
pub mod score;

use crate::amber_anyhow::{Context, Result};
use crate::reporting::style::Colorize;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tracing::info;

// Re-export the color trait so subcommand modules can import it uniformly.
pub use crate::reporting::style::Colorize as _;

use crate::analysis::repo::RepositoryAnalyzer;
use crate::config::Config;
use crate::scoring::classifier::SafetyClassifier;

/// Amber: Autonomous Dependency Reduction Engine
#[derive(Parser, Debug)]
#[command(name = "amber")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Dependency garbage collector for Rust")]
#[command(long_about = "\
Dependency garbage collector for Rust.

Amber analyzes a Cargo project, detects how each third-party crate is used, \
scores every dependency for replaceability, and generates validated, drop-in \
replacement modules for common utility crates.

Generated replacements are emitted as `amber_<crate>_redux` modules and \
validated with `cargo check` before they are reported, so a proposal that \
does not compile never reaches you.")]
#[command(after_help = "\
EXAMPLES:
    amber                              Analyze the current project
    amber --format json .              Emit a machine-readable report
    amber --propose --threshold 70     Propose replacements for high scorers
    amber score anyhow                 Score a single crate
    amber replace serde --out-dir out  Generate a validated replacement module

EXIT STATUS:
    0  success, no policy violations
    1  analysis found policy violations (strict mode)
    2  a command or I/O error occurred

DOCUMENTATION:
    Man page: docs/man/amber.1 (install to a manpath and run `man amber`).
    Reproducible demos: docs/vhs/README.md.")]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    /// Path to the Rust project to analyze
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Output format
    #[arg(
        short,
        long,
        value_enum,
        default_value = "console",
        long_help = "\
Output format for reports. `console` is a human table; `json` is \
machine-readable; `pr` is Markdown suitable for a pull-request comment; \
`sarif` is the static-analysis interchange format for CI; `emoji` is a \
compact, friendly summary."
    )]
    pub format: OutputFormat,

    /// Use emoji output format (shorthand for --format emoji)
    #[arg(long)]
    pub emoji: bool,

    /// Minimum safety score threshold (0-100)
    #[arg(
        short,
        long,
        default_value = "50",
        long_help = "\
Minimum replaceability score (0-100) a dependency must reach before Amber \
flags it as a candidate. Used together with --propose to control how many \
replacement proposals are generated: lower values propose more, higher \
values are conservative."
    )]
    pub threshold: u8,

    /// Show transitive dependencies
    #[arg(short = 'T', long)]
    pub transitive: bool,

    /// Generate replacement proposals
    #[arg(
        short,
        long,
        long_help = "\
After scoring, generate validated replacement modules for every dependency \
that meets --threshold. Proposals are written as `amber_<crate>_redux` files \
and checked with `cargo check` before they are reported."
    )]
    pub propose: bool,

    /// Exclude dev-dependencies from analysis
    #[arg(long)]
    pub no_dev: bool,

    /// Focus on specific crates (comma-separated)
    #[arg(
        short,
        long,
        value_delimiter = ',',
        long_help = "\
Restrict analysis to the named crates only. Accepts a comma-separated list \
or repeated flags, for example `--crates serde,anyhow` or \
`--crates serde --crates anyhow`."
    )]
    pub crates: Vec<String>,

    /// Verbosity level
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Path to .amber.toml config file
    #[arg(
        long,
        long_help = "\
Path to an Amber configuration file. Defaults to `.amber.toml` in the \
target project root. The file can set the threshold, a required/forbidden \
crate policy, and the replacement-library location."
    )]
    pub config: Option<PathBuf>,

    /// Use the Padagonia replacement library (requires `library` feature)
    #[cfg(feature = "library")]
    #[arg(long, overrides_with = "no_library")]
    pub library: bool,

    /// Disable the Padagonia replacement library
    #[cfg(feature = "library")]
    #[arg(long, overrides_with = "library")]
    pub no_library: bool,

    /// Fetch live metadata from crates.io (requires `online` feature)
    #[cfg(feature = "online")]
    #[arg(long)]
    pub online: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

impl Cli {
    #[must_use]
    pub fn output_format(&self) -> OutputFormat {
        if self.emoji {
            OutputFormat::Emoji
        } else {
            self.format.clone()
        }
    }
}

#[derive(Clone, Debug, Default, clap::ValueEnum)]
pub enum OutputFormat {
    #[default]
    Console,
    Json,
    Pr,
    Sarif,
    Emoji,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Analyze dependencies and generate report
    #[command(long_about = "\
Run the full pipeline: parse Cargo metadata, analyze AST usage of every \
dependency, score each crate for replaceability, and emit a report in the \
selected --format. Exits non-zero in strict mode when a policy is violated.")]
    Analyze {
        /// Export report to file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Score a specific dependency's replaceability
    #[command(long_about = "\
Score one crate across usage simplicity, transitive value, security, \
maintenance, testability, and API surface, then print the breakdown.")]
    Score {
        /// Crate name to score
        crate_name: String,
    },
    /// List all dependencies with usage statistics
    List,
    /// Generate replacement code for a dependency
    #[command(long_about = "\
Generate a validated `amber_<crate>_redux` replacement module for the named \
crate and write it under --out-dir. The module is checked with `cargo check`; \
if it does not compile, no file is reported as ready.")]
    Replace {
        /// Crate to replace
        crate_name: String,
        /// Output directory for generated code
        #[arg(short, long, default_value = "amber_out")]
        out_dir: PathBuf,
    },
    /// Show Amber's internal module roadmap
    Roadmap,
    /// Generate a scoped technical directive for a dependency
    #[command(long_about = "\
Emit a focused, implementation-ready directive describing how to replace the \
named crate, including the API surface to cover and the validation steps.")]
    Directives {
        /// Crate name to generate a directive for
        crate_name: String,
        /// Output file path (defaults to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Manage the Padagonia replacement module library (requires `library` feature)
    #[cfg(feature = "library")]
    Library {
        /// Library operation to perform
        #[command(subcommand)]
        command: LibraryCommands,
    },
}

#[cfg(feature = "library")]
#[derive(Subcommand, Debug)]
pub enum LibraryCommands {
    /// List stored replacement modules
    List,
    /// Import a replacement module into the library
    Import {
        /// Path to the .rs file to import
        path: PathBuf,
        /// Crate name this module replaces
        #[arg(long)]
        crate_name: String,
    },
    /// Export a stored module to a file
    Export {
        /// Crate name to export
        crate_name: String,
        /// Output file path
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Show the resolved library file path
    Path,
    /// Search the library for entries matching a query
    Search {
        /// Query to match against crate or module names
        query: String,
    },
    /// Remove all entries for a crate from the library
    Remove {
        /// Crate name to remove
        crate_name: String,
    },
}

/// Entry point used by the `amber` binary.
pub fn run_cli() -> ! {
    let cli = Cli::parse();
    let exit_code = match execute(&cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{} {e}", "error:".red().bold());
            2
        }
    };
    std::process::exit(exit_code);
}

/// Execute the parsed CLI and return an exit code.
///
/// # Errors
///
/// Returns an error if command execution fails.
pub fn execute(cli: &Cli) -> Result<i32> {
    init_tracing(cli.verbose);
    roadmap::print_banner();

    let manifest_path = if cli.path.join("Cargo.toml").exists() {
        cli.path.join("Cargo.toml")
    } else {
        cli.path.clone()
    };

    run(cli, &manifest_path)
}

fn init_tracing(verbose: u8) {
    let filter = match verbose {
        0 => "amber=info",
        1 => "amber=debug",
        _ => "amber=trace",
    };

    let builder = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .without_time();

    // At higher verbosity levels include the target/module path to make
    // per-component debugging easier.
    let builder = if verbose >= 2 {
        builder
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
    } else {
        builder.with_target(false)
    };

    if let Err(e) = builder.try_init() {
        eprintln!("warning: failed to initialize tracing: {e}");
    }
}

/// Run the parsed command against `manifest_path`.
///
/// # Errors
///
/// Returns an error if command execution fails.
#[allow(clippy::too_many_lines)]
pub fn run(cli: &Cli, manifest_path: &Path) -> Result<i32> {
    match &cli.command {
        Some(Commands::Roadmap) => roadmap::run(),
        Some(Commands::List) => list::run(cli, manifest_path),
        Some(Commands::Score { crate_name }) => score::run(cli, manifest_path, crate_name),
        Some(Commands::Replace {
            crate_name,
            out_dir,
        }) => replace::run(cli, manifest_path, crate_name, out_dir),
        Some(Commands::Directives { crate_name, output }) => {
            directives::run(cli, manifest_path, crate_name, output.as_deref())
        }
        #[cfg(feature = "library")]
        Some(Commands::Library { command }) => library::run(cli, manifest_path, command),
        Some(Commands::Analyze { output }) => {
            info!("Starting full repository analysis...");
            analyze::run(cli, manifest_path, output.as_deref())
        }
        None => {
            info!("Starting full repository analysis...");
            analyze::run(cli, manifest_path, None)
        }
    }
}

/// Determine whether the online metadata provider should be used.
#[must_use]
pub const fn use_online(cli: &Cli) -> bool {
    #[cfg(feature = "online")]
    return cli.online;
    #[cfg(not(feature = "online"))]
    {
        let _ = cli;
        false
    }
}

/// Load configuration from the target project directory.
///
/// # Errors
///
/// Returns an error if the config file cannot be read or parsed.
pub fn load_config(cli: &Cli, manifest_path: &Path) -> Result<Config> {
    let config_path = cli.config.as_ref().map_or_else(
        || {
            manifest_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(".amber.toml")
        },
        std::clone::Clone::clone,
    );

    let config: Config = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config from {}", config_path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", config_path.display()))?
    } else {
        Config::default()
    };

    config
        .validate()
        .map_err(|e| crate::anyhow!("Invalid config at {}: {e}", config_path.display()))?;

    Ok(config)
}

/// Build a classifier, honoring config weights if present.
#[must_use]
pub fn build_classifier(config: &Config) -> SafetyClassifier {
    config
        .weights
        .as_ref()
        .map_or_else(SafetyClassifier::new, |weights| {
            SafetyClassifier::with_weights(weights.clone())
        })
}

/// Build a repository analyzer using the configured metadata provider.
///
/// # Errors
///
/// Returns an error if the analyzer cannot be created.
pub fn build_analyzer(manifest_path: &Path, cli: &Cli) -> Result<RepositoryAnalyzer> {
    use crate::metadata::offline::OfflineProvider;
    use crate::metadata::rustsec::RustSecEnricher;

    #[cfg(feature = "online")]
    use crate::metadata::online::CratesIoProvider;

    if use_online(cli) {
        #[cfg(feature = "online")]
        {
            info!("Using online metadata provider (crates.io)");
            return RepositoryAnalyzer::with_provider(
                manifest_path,
                Box::new(RustSecEnricher::new(CratesIoProvider::new())),
            );
        }
        #[cfg(not(feature = "online"))]
        {
            crate::bail!("The --online flag requires the `online` Cargo feature");
        }
    }

    info!("Using offline metadata provider with RustSec enrichment");
    RepositoryAnalyzer::with_provider(
        manifest_path,
        Box::new(RustSecEnricher::new(OfflineProvider::new())),
    )
}

#[cfg(feature = "library")]
#[must_use]
pub fn use_library(cli: &Cli, config: &Config) -> bool {
    if cli.no_library {
        return false;
    }
    if cli.library {
        return true;
    }
    config
        .library
        .as_ref()
        .is_some_and(|library| library.enabled)
}

#[cfg(feature = "library")]
/// Open the replacement library store configured for this run.
///
/// # Errors
///
/// Returns an error if the library cannot be opened.
pub fn open_library(config: &Config) -> Result<crate::library::LibraryStore> {
    let path = config
        .library
        .as_ref()
        .map_or_else(crate::config::LibraryConfig::default, Clone::clone)
        .resolved_path();
    crate::library::LibraryStore::open(&path)
        .with_context(|| format!("Failed to open replacement library at {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture_manifest(name: &str) -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/fixtures");
        path.push(name);
        path.push("Cargo.toml");
        path
    }

    fn copy_fixture_to_temp(name: &str) -> crate::temp::TempDir {
        let temp = crate::temp::tempdir().unwrap();
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        copy_dir_all(&src, temp.path()).unwrap();
        temp
    }

    fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
        fs::create_dir_all(&dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
            } else {
                fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
            }
        }
        Ok(())
    }

    #[test]
    fn run_list_command_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            manifest.parent().unwrap().to_str().unwrap(),
            "list",
        ]);
        assert!(run(&cli, &manifest).is_ok());
    }

    #[test]
    fn run_analyze_command_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--threshold",
            "100",
            manifest.parent().unwrap().to_str().unwrap(),
            "analyze",
        ]);
        assert!(run(&cli, &manifest).is_ok());
    }

    #[test]
    fn run_score_command_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            manifest.parent().unwrap().to_str().unwrap(),
            "score",
            "anyhow",
        ]);
        assert!(run(&cli, &manifest).is_ok());
    }

    #[test]
    fn run_replace_command_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--threshold",
            "0",
            manifest.parent().unwrap().to_str().unwrap(),
            "replace",
            "anyhow",
        ]);
        assert!(run(&cli, &manifest).is_ok());
    }

    #[test]
    fn run_directives_command_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            manifest.parent().unwrap().to_str().unwrap(),
            "directives",
            "anyhow",
        ]);
        assert!(run(&cli, &manifest).is_ok());
    }

    #[test]
    fn run_roadmap_command_succeeds() {
        let cli = Cli::parse_from(["amber", "roadmap"]);
        assert!(run(&cli, Path::new(".")).is_ok());
    }

    #[test]
    fn run_analyze_invalid_manifest_errors() {
        let cli = Cli::parse_from(["amber", "/nonexistent/path", "analyze"]);
        assert!(run(&cli, Path::new("/nonexistent/Cargo.toml")).is_err());
    }

    #[test]
    fn run_score_missing_crate_errors() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            manifest.parent().unwrap().to_str().unwrap(),
            "score",
            "not_present",
        ]);
        assert!(run(&cli, &manifest).is_err());
    }

    #[test]
    fn run_replace_below_threshold_returns_zero() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--threshold",
            "100",
            manifest.parent().unwrap().to_str().unwrap(),
            "replace",
            "anyhow",
        ]);
        assert_eq!(run(&cli, &manifest).unwrap(), 0);
    }

    #[test]
    fn run_analyze_actionable_returns_one() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--threshold",
            "50",
            manifest.parent().unwrap().to_str().unwrap(),
            "analyze",
        ]);
        assert_eq!(run(&cli, &manifest).unwrap(), 1);
    }

    #[test]
    fn run_analyze_strict_policy_returns_two() {
        let temp = copy_fixture_to_temp("sample_project");
        fs::write(
            temp.path().join(".amber.toml"),
            r#"
[policy]
required = []
forbidden = ["anyhow"]
strict = true
"#,
        )
        .unwrap();
        let manifest = temp.path().join("Cargo.toml");
        let cli = Cli::parse_from([
            "amber",
            "--threshold",
            "100",
            temp.path().to_str().unwrap(),
            "analyze",
        ]);
        assert_eq!(run(&cli, &manifest).unwrap(), 2);
    }

    #[test]
    fn run_analyze_with_propose_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--threshold",
            "100",
            "--propose",
            manifest.parent().unwrap().to_str().unwrap(),
            "analyze",
        ]);
        assert!(run(&cli, &manifest).is_ok());
    }

    #[test]
    fn run_analyze_sarif_format_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--format",
            "sarif",
            "--threshold",
            "100",
            manifest.parent().unwrap().to_str().unwrap(),
            "analyze",
        ]);
        assert!(run(&cli, &manifest).is_ok());
    }

    #[test]
    fn run_analyze_emoji_flag_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--emoji",
            "--threshold",
            "100",
            manifest.parent().unwrap().to_str().unwrap(),
            "analyze",
        ]);
        assert!(run(&cli, &manifest).is_ok());
        assert!(matches!(cli.output_format(), OutputFormat::Emoji));
    }

    #[test]
    fn run_list_emoji_flag_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--emoji",
            manifest.parent().unwrap().to_str().unwrap(),
            "list",
        ]);
        assert!(run(&cli, &manifest).is_ok());
        assert!(matches!(cli.output_format(), OutputFormat::Emoji));
    }

    #[test]
    fn run_score_emoji_flag_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--emoji",
            manifest.parent().unwrap().to_str().unwrap(),
            "score",
            "anyhow",
        ]);
        assert!(run(&cli, &manifest).is_ok());
        assert!(matches!(cli.output_format(), OutputFormat::Emoji));
    }

    #[test]
    fn emoji_flag_overrides_format() {
        let cli = Cli::parse_from(["amber", "--format", "json", "--emoji", "."]);
        assert!(matches!(cli.output_format(), OutputFormat::Emoji));
    }

    #[test]
    fn load_config_parses_valid_policy() {
        let temp = crate::temp::tempdir().unwrap();
        let config_path = temp.path().join(".amber.toml");
        fs::write(
            &config_path,
            r#"
[policy]
required = []
forbidden = ["bad_crate"]
strict = true
"#,
        )
        .unwrap();
        let cli = Cli::parse_from(["amber"]);
        let config = load_config(&cli, &config_path).unwrap();
        assert!(config.policy.unwrap().is_forbidden("bad_crate"));
    }

    #[test]
    fn load_config_returns_error_for_invalid_toml() {
        let temp = crate::temp::tempdir().unwrap();
        let config_path = temp.path().join(".amber.toml");
        fs::write(&config_path, "not valid toml").unwrap();
        let cli = Cli::parse_from(["amber"]);
        assert!(load_config(&cli, &config_path).is_err());
    }

    #[test]
    fn execute_roadmap_command_succeeds() {
        let cli = Cli::parse_from(["amber", "roadmap"]);
        assert!(execute(&cli).is_ok());
    }

    #[test]
    fn execute_list_command_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            manifest.parent().unwrap().to_str().unwrap(),
            "list",
        ]);
        assert!(execute(&cli).is_ok());
    }

    #[cfg(feature = "online")]
    #[test]
    fn online_flag_uses_online_provider_without_panic() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--online",
            manifest.parent().unwrap().to_str().unwrap(),
            "list",
        ]);
        assert!(run(&cli, &manifest).is_ok());
    }

    #[test]
    fn execute_errors_on_invalid_manifest() {
        let cli = Cli::parse_from(["amber", "/nonexistent/path", "list"]);
        assert!(execute(&cli).is_err());
    }

    #[test]
    fn run_no_subcommand_runs_analysis() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--threshold",
            "100",
            manifest.parent().unwrap().to_str().unwrap(),
        ]);
        assert!(run(&cli, &manifest).is_ok());
    }

    #[test]
    fn run_analyze_writes_json_to_output_path() {
        let temp = copy_fixture_to_temp("sample_project");
        let manifest = temp.path().join("Cargo.toml");
        let output = temp.path().join("report.json");
        let cli = Cli::parse_from([
            "amber",
            "--format",
            "json",
            "--threshold",
            "100",
            temp.path().to_str().unwrap(),
            "analyze",
            "--output",
            output.to_str().unwrap(),
        ]);
        assert!(run(&cli, &manifest).is_ok());
        assert!(output.exists());
    }

    #[test]
    fn run_analyze_writes_pr_to_output_path() {
        let temp = copy_fixture_to_temp("sample_project");
        let manifest = temp.path().join("Cargo.toml");
        let output = temp.path().join("report.md");
        let cli = Cli::parse_from([
            "amber",
            "--format",
            "pr",
            "--threshold",
            "100",
            temp.path().to_str().unwrap(),
            "analyze",
            "--output",
            output.to_str().unwrap(),
        ]);
        assert!(run(&cli, &manifest).is_ok());
        assert!(output.exists());
    }

    #[test]
    fn run_analyze_writes_sarif_to_output_path() {
        let temp = copy_fixture_to_temp("sample_project");
        let manifest = temp.path().join("Cargo.toml");
        let output = temp.path().join("report.sarif");
        let cli = Cli::parse_from([
            "amber",
            "--format",
            "sarif",
            "--threshold",
            "0",
            manifest.parent().unwrap().to_str().unwrap(),
            "analyze",
            "--output",
            output.to_str().unwrap(),
        ]);
        assert!(run(&cli, &manifest).is_ok());
        assert!(output.exists());
    }

    #[test]
    fn run_analyze_with_propose_generates_proposals() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--threshold",
            "0",
            "--propose",
            manifest.parent().unwrap().to_str().unwrap(),
            "analyze",
        ]);
        assert!(run(&cli, &manifest).is_ok());
    }

    #[test]
    fn run_analyze_empty_deps_returns_zero() {
        let temp = crate::temp::tempdir().unwrap();
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = 'empty'\nversion = '0.1.0'\n",
        )
        .unwrap();
        std::fs::create_dir(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/main.rs"), "fn main() {}").unwrap();
        let manifest = temp.path().join("Cargo.toml");
        let cli = Cli::parse_from(["amber", temp.path().to_str().unwrap(), "analyze"]);
        assert_eq!(run(&cli, &manifest).unwrap(), 0);
    }

    #[test]
    fn execute_trace_filter_initializes() {
        let cli = Cli::parse_from(["amber", "-vv", "roadmap"]);
        assert!(execute(&cli).is_ok());
    }

    #[test]
    fn run_analyze_json_to_stdout_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--format",
            "json",
            "--threshold",
            "100",
            manifest.parent().unwrap().to_str().unwrap(),
            "analyze",
        ]);
        assert!(run(&cli, &manifest).is_ok());
    }

    #[test]
    fn run_analyze_pr_to_stdout_succeeds() {
        let manifest = fixture_manifest("sample_project");
        let cli = Cli::parse_from([
            "amber",
            "--format",
            "pr",
            "--threshold",
            "100",
            manifest.parent().unwrap().to_str().unwrap(),
            "analyze",
        ]);
        assert!(run(&cli, &manifest).is_ok());
    }

    #[test]
    fn run_analyze_non_strict_policy_returns_one() {
        let temp = copy_fixture_to_temp("sample_project");
        fs::write(
            temp.path().join(".amber.toml"),
            r#"
[policy]
required = []
forbidden = ["anyhow"]
strict = false
"#,
        )
        .unwrap();
        let manifest = temp.path().join("Cargo.toml");
        let cli = Cli::parse_from([
            "amber",
            "--threshold",
            "100",
            temp.path().to_str().unwrap(),
            "analyze",
        ]);
        assert_eq!(run(&cli, &manifest).unwrap(), 1);
    }
}
