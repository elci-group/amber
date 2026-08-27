// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! The `benchmark` subcommand for behavioral comparison using jeenome.
//!
//! This module implements runtime behavioral analysis to compare a project's
//! behavior before and after replacing a dependency. It uses strace to capture
//! system calls and jeenome to analyze behavioral patterns.

use crate::amber_anyhow::{Context, Result};
use crate::cli::paths::validate_output_path;
use crate::cli::{build_analyzer, build_classifier, load_config, Cli};
use crate::replacement::generator::Generator;
use crate::reporting::formatters::ConsoleReporter;
use crate::scoring::classifier::ReplacementScore;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

/// Run the `benchmark` command for `crate_name`.
///
/// # Errors
///
/// Returns an error if strace is not available, the dependency cannot be analyzed,
/// or the benchmark cannot be executed.
pub fn run(
    cli: &Cli,
    manifest_path: &Path,
    crate_name: &str,
    out_dir: &Path,
    cargo_cmd: &str,
    skip_baseline: bool,
    skip_replacement: bool,
) -> Result<i32> {
    info!(crate = %crate_name, "starting behavioral benchmark");

    let out_dir = validate_output_path(&cli.path, out_dir)?;
    fs::create_dir_all(&out_dir).context("failed to create output directory")?;

    // Check strace availability
    check_strace()?;

    // Load configuration and analyze dependency
    let config = load_config(cli, manifest_path)?;
    let analyzer = build_analyzer(manifest_path, cli)?;
    let dep = analyzer.get_dependency(crate_name)?;
    let usage = crate::analysis::usage::UsageAnalyzer::new(manifest_path)?;
    let usage_stats = usage.analyze_crate_usage(crate_name)?;
    let classifier = build_classifier(&config);
    let score = classifier.score_dependency(&dep, &usage_stats);

    let threshold = config.threshold.unwrap_or(cli.threshold);
    if score.overall < threshold {
        warn!(
            crate = %crate_name,
            score = score.overall,
            threshold,
            "replacement score is below threshold; use --threshold to override"
        );
        return Ok(0);
    }

    // Generate replacement
    let generator = Generator::new(out_dir.clone());
    let proposal = generator.generate_replacement(crate_name, &usage_stats, &score)?;

    let reporter = ConsoleReporter::new();
    reporter.print_replacement_proposal(&proposal);

    // Define trace file paths
    let baseline_trace = out_dir.join("baseline.strace");
    let replacement_trace = out_dir.join("replacement.strace");

    // Capture baseline behavior
    if !skip_baseline {
        info!(trace = %baseline_trace.display(), "capturing baseline behavior with strace");
        capture_strace(manifest_path, cargo_cmd, &baseline_trace)?;
    } else if !baseline_trace.exists() {
        warn!(trace = %baseline_trace.display(), "requested baseline trace does not exist; capturing it");
        capture_strace(manifest_path, cargo_cmd, &baseline_trace)?;
    }

    // Apply replacement temporarily
    info!(crate = %crate_name, "applying replacement in temporary project");
    let temp_dir = crate::temp::tempdir().context("failed to create temp directory")?;
    let temp_project = apply_replacement_temporarily(manifest_path, &proposal, temp_dir.path())?;

    // Capture replacement behavior
    if !skip_replacement {
        info!(trace = %replacement_trace.display(), "capturing replacement behavior with strace");
        capture_strace(&temp_project, cargo_cmd, &replacement_trace)?;
    } else if !replacement_trace.exists() {
        warn!(trace = %replacement_trace.display(), "requested replacement trace does not exist; capturing it");
        capture_strace(&temp_project, cargo_cmd, &replacement_trace)?;
    }

    // Analyze traces with jeenome
    info!(baseline = %baseline_trace.display(), replacement = %replacement_trace.display(), "analyzing behavioral traces");
    let baseline_analysis = analyze_with_jeenome(&baseline_trace)?;
    let replacement_analysis = analyze_with_jeenome(&replacement_trace)?;

    // Compare behaviors
    let comparison = compare_behaviors(&baseline_analysis, &replacement_analysis);

    // Report results
    print_benchmark_report(&comparison, &score);

    Ok(0)
}

/// Check if strace is available on the system.
fn check_strace() -> Result<()> {
    let result = Command::new("strace").arg("--version").output();

    match result {
        Ok(output) if output.status.success() => Ok(()),
        _ => crate::bail!(
            "strace is not available or not working. Please install strace to use benchmarking."
        ),
    }
}

/// Capture strace output for a cargo command.
fn capture_strace(project_path: &Path, cargo_cmd: &str, output_path: &Path) -> Result<()> {
    let args: Vec<&str> = cargo_cmd.split_whitespace().collect();

    let result = Command::new("strace")
        .args(["-f", "-e", "trace=all", "-o"])
        .arg(output_path)
        .arg("cargo")
        .args(&args)
        .current_dir(project_path)
        .output()
        .context("failed to run strace")?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        crate::bail!("strace capture failed: {stderr}");
    }

    Ok(())
}

/// Apply a replacement module to a temporary project copy.
fn apply_replacement_temporarily(
    manifest_path: &Path,
    proposal: &crate::replacement::generator::ReplacementProposal,
    temp_dir: &Path,
) -> Result<PathBuf> {
    // Copy project to temp directory
    copy_project(manifest_path.parent().unwrap_or(manifest_path), temp_dir)?;

    let temp_manifest = temp_dir.join("Cargo.toml");

    // Write the replacement module
    let module_path = temp_dir.join("src").join(&proposal.replacement_module);
    let module_parent = module_path
        .parent()
        .context("replacement module path has no parent directory")?;
    fs::create_dir_all(module_parent)?;
    fs::write(&module_path, &proposal.replacement_code)?;

    // Add module to lib.rs or main.rs
    integrate_module(temp_dir, &proposal.replacement_module)?;

    Ok(temp_manifest)
}

/// Copy project files to a temporary directory.
fn copy_project(src: &Path, dst: &Path) -> Result<()> {
    let src = fs::canonicalize(src)?;

    for entry in fs::read_dir(&src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());

        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&target)?;
            copy_project(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }

    Ok(())
}

/// Integrate the replacement module into the project.
fn integrate_module(project_dir: &Path, module_name: &str) -> Result<()> {
    let src_dir = project_dir.join("src");

    // Try lib.rs first, then main.rs
    let lib_rs = src_dir.join("lib.rs");
    let main_rs = src_dir.join("main.rs");

    let target_file = if lib_rs.exists() {
        lib_rs
    } else if main_rs.exists() {
        main_rs
    } else {
        crate::bail!("no lib.rs or main.rs found in src/");
    };

    let mut content = fs::read_to_string(&target_file)?;
    let mod_decl = format!("mod {module_name};");

    if !content.contains(&mod_decl) {
        content.push('\n');
        content.push_str(&mod_decl);
        content.push('\n');
        fs::write(&target_file, content)?;
    }

    Ok(())
}

/// Analyze a strace file using jeenome.
fn analyze_with_jeenome(trace_path: &Path) -> Result<BehavioralProfile> {
    let trace_content = fs::read_to_string(trace_path).context("failed to read trace file")?;

    let mut event_count = 0;
    let mut categories: HashMap<EventCategory, usize> = HashMap::new();

    for line in trace_content.lines() {
        if let Some(syscall) = syscall_name(line) {
            event_count += 1;
            *categories
                .entry(EventCategory::from_syscall(syscall))
                .or_insert(0) += 1;
        }
    }

    Ok(BehavioralProfile {
        event_count,
        categories,
    })
}

/// Extract the syscall name from normal, PID-prefixed, timestamped, or resumed
/// `strace` output without pulling a regex/parser runtime into Amber.
fn syscall_name(line: &str) -> Option<&str> {
    let prefix = line.split_once('(')?.0.trim();
    let candidate = if let Some(resumed) = prefix.strip_prefix("<... ") {
        resumed.strip_suffix(" resumed>")?
    } else {
        prefix.split_whitespace().next_back()?
    };
    (!candidate.is_empty()
        && candidate
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'))
    .then_some(candidate)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EventCategory {
    Filesystem,
    Network,
    Process,
    Memory,
    Signals,
    Timing,
    Unknown,
}

impl EventCategory {
    fn from_syscall(syscall: &str) -> Self {
        match syscall {
            "openat" | "open" | "read" | "write" | "close" | "stat" | "lstat" | "fstat"
            | "access" | "unlink" | "unlinkat" | "rename" | "renameat" | "mkdir" | "mkdirat"
            | "rmdir" | "getdents" | "getdents64" | "pread64" | "pwrite64" | "lseek"
            | "readlink" | "readlinkat" | "dup" | "dup2" | "dup3" | "fcntl" | "ioctl" | "fsync"
            | "fdatasync" | "truncate" | "ftruncate" | "creat" | "openat2" => Self::Filesystem,
            "socket" | "connect" | "accept" | "accept4" | "sendto" | "recvfrom" | "sendmsg"
            | "recvmsg" | "bind" | "listen" | "shutdown" | "getsockname" | "getpeername"
            | "socketpair" | "setsockopt" | "getsockopt" | "sendmmsg" | "recvmmsg" => Self::Network,
            "execve" | "execveat" | "fork" | "vfork" | "clone" | "clone3" | "wait4" | "waitid"
            | "exit" | "exit_group" | "kill" | "tkill" | "tgkill" | "setpgid" | "setsid"
            | "setuid" | "setgid" | "prctl" | "pipe" | "pipe2" | "pidfd_open" | "pidfd_getfd"
            | "pidfd_send_signal" => Self::Process,
            "mmap" | "munmap" | "mprotect" | "mremap" | "madvise" | "brk" | "memfd_create"
            | "membarrier" | "mlock" | "munlock" | "mincore" | "msync" => Self::Memory,
            "rt_sigaction" | "rt_sigprocmask" | "rt_sigreturn" | "sigaltstack" | "signalfd"
            | "signalfd4" | "eventfd" | "eventfd2" | "pause" => Self::Signals,
            "nanosleep" | "clock_nanosleep" | "timer_create" | "timer_settime"
            | "timer_gettime" | "timer_delete" | "alarm" | "clock_settime" | "clock_gettime"
            | "clock_getres" | "gettimeofday" | "time" | "timerfd_create" | "timerfd_settime"
            | "timerfd_gettime" => Self::Timing,
            _ => Self::Unknown,
        }
    }
}

/// Behavioral profile extracted from a strace trace.
#[derive(Debug, Clone)]
struct BehavioralProfile {
    event_count: usize,
    categories: HashMap<EventCategory, usize>,
}

/// Compare two behavioral profiles and calculate similarity.
fn compare_behaviors(
    baseline: &BehavioralProfile,
    replacement: &BehavioralProfile,
) -> BehavioralComparison {
    let event_similarity = calculate_event_similarity(baseline, replacement);
    let category_similarity =
        calculate_category_similarity(&baseline.categories, &replacement.categories);

    // Check for new or different behavioral patterns
    let new_categories: Vec<_> = replacement
        .categories
        .keys()
        .filter(|k| !baseline.categories.contains_key(k))
        .copied()
        .collect();

    let missing_categories: Vec<_> = baseline
        .categories
        .keys()
        .filter(|k| !replacement.categories.contains_key(k))
        .copied()
        .collect();

    BehavioralComparison {
        event_similarity,
        category_similarity,
        new_categories,
        missing_categories,
        baseline_event_count: baseline.event_count,
        replacement_event_count: replacement.event_count,
    }
}

/// Calculate similarity between event counts.
#[allow(clippy::cast_precision_loss)] // Event-count ratios are intentionally approximate.
fn calculate_event_similarity(
    baseline: &BehavioralProfile,
    replacement: &BehavioralProfile,
) -> f32 {
    let baseline_count = baseline.event_count as f32;
    let replacement_count = replacement.event_count as f32;

    if baseline_count == 0.0 && replacement_count == 0.0 {
        return 1.0;
    }

    let max_count = baseline_count.max(replacement_count);
    let diff = (baseline_count - replacement_count).abs();

    1.0 - (diff / max_count)
}

/// Calculate similarity between category distributions.
#[allow(clippy::cast_precision_loss)] // Event-count ratios are intentionally approximate.
fn calculate_category_similarity(
    baseline: &HashMap<EventCategory, usize>,
    replacement: &HashMap<EventCategory, usize>,
) -> f32 {
    let all_categories: HashSet<_> = baseline.keys().chain(replacement.keys()).collect();

    if all_categories.is_empty() {
        return 1.0;
    }

    let mut total_diff = 0.0;
    let mut total_weight = 0.0;

    for category in all_categories {
        let baseline_count = *baseline.get(category).unwrap_or(&0) as f32;
        let replacement_count = *replacement.get(category).unwrap_or(&0) as f32;

        let max = baseline_count.max(replacement_count);
        if max > 0.0 {
            let diff = (baseline_count - replacement_count).abs();
            total_diff += diff / max;
            total_weight += 1.0;
        }
    }

    if total_weight > 0.0 {
        1.0 - (total_diff / total_weight)
    } else {
        1.0
    }
}

/// Comparison result between baseline and replacement behaviors.
#[derive(Debug)]
struct BehavioralComparison {
    event_similarity: f32,
    category_similarity: f32,
    new_categories: Vec<EventCategory>,
    missing_categories: Vec<EventCategory>,
    baseline_event_count: usize,
    replacement_event_count: usize,
}

/// Print the benchmark comparison report.
fn print_benchmark_report(comparison: &BehavioralComparison, score: &ReplacementScore) {
    use crate::reporting::style::Colorize;

    println!("\n{}", "=== Behavioral Benchmark Report ===".bold());

    println!("\nStatic Analysis:");
    println!("  Overall score: {}/100", score.overall);
    println!("  Confidence: {:.1}%", score.confidence);

    println!("\nBehavioral Comparison:");
    println!(
        "  Event similarity: {:.1}%",
        comparison.event_similarity * 100.0
    );
    println!(
        "  Category similarity: {:.1}%",
        comparison.category_similarity * 100.0
    );
    println!("  Baseline events: {}", comparison.baseline_event_count);
    println!(
        "  Replacement events: {}",
        comparison.replacement_event_count
    );

    if !comparison.new_categories.is_empty() {
        println!("\n  New behavioral categories:");
        for cat in &comparison.new_categories {
            println!("    - {cat:?}");
        }
    }

    if !comparison.missing_categories.is_empty() {
        println!("\n  Missing behavioral categories:");
        for cat in &comparison.missing_categories {
            println!("    - {cat:?}");
        }
    }

    let overall_similarity = comparison
        .event_similarity
        .midpoint(comparison.category_similarity);
    println!(
        "\nOverall behavioral similarity: {:.1}%",
        overall_similarity * 100.0
    );

    if overall_similarity > 0.9 {
        println!(
            "{}",
            "  ✅ High behavioral equivalence - safe to migrate".green()
        );
    } else if overall_similarity > 0.7 {
        println!(
            "{}",
            "  ⚠️  Moderate behavioral difference - review recommended".yellow()
        );
    } else {
        println!(
            "{}",
            "  ❌ Significant behavioral difference - migration risky".red()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_calculate_event_similarity_identical() {
        let baseline = BehavioralProfile {
            event_count: 100,
            categories: HashMap::new(),
        };
        let replacement = BehavioralProfile {
            event_count: 100,
            categories: HashMap::new(),
        };

        let similarity = calculate_event_similarity(&baseline, &replacement);
        assert!((similarity - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_calculate_event_similarity_different() {
        let baseline = BehavioralProfile {
            event_count: 100,
            categories: HashMap::new(),
        };
        let replacement = BehavioralProfile {
            event_count: 50,
            categories: HashMap::new(),
        };

        let similarity = calculate_event_similarity(&baseline, &replacement);
        assert!((similarity - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_calculate_category_similarity_identical() {
        let mut baseline = HashMap::new();
        baseline.insert(EventCategory::Filesystem, 50);
        baseline.insert(EventCategory::Network, 30);

        let mut replacement = HashMap::new();
        replacement.insert(EventCategory::Filesystem, 50);
        replacement.insert(EventCategory::Network, 30);

        let similarity = calculate_category_similarity(&baseline, &replacement);
        assert!((similarity - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_calculate_category_similarity_different() {
        let mut baseline = HashMap::new();
        baseline.insert(EventCategory::Filesystem, 50);
        baseline.insert(EventCategory::Network, 30);

        let mut replacement = HashMap::new();
        replacement.insert(EventCategory::Filesystem, 25);
        replacement.insert(EventCategory::Network, 15);

        let similarity = calculate_category_similarity(&baseline, &replacement);
        // Both categories are half the count, so similarity should be 0.5
        assert!((similarity - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_calculate_category_similarity_new_category() {
        let mut baseline = HashMap::new();
        baseline.insert(EventCategory::Filesystem, 50);

        let mut replacement = HashMap::new();
        replacement.insert(EventCategory::Filesystem, 50);
        replacement.insert(EventCategory::Network, 10); // New category

        let similarity = calculate_category_similarity(&baseline, &replacement);
        // Should be less than 1.0 due to new category
        assert!(similarity < 1.0);
        // The new category will count as a full difference for that category
        // Since filesystem is identical (50 vs 50), only network differs
        // The algorithm calculates this as: filesystem (0 diff) + network (1.0 diff) / 2 categories = 0.5
        assert!(similarity >= 0.4); // Should be around 0.5
    }

    #[test]
    fn test_compare_behaviors_detection() {
        let mut baseline_categories = HashMap::new();
        baseline_categories.insert(EventCategory::Filesystem, 50);

        let mut replacement_categories = HashMap::new();
        replacement_categories.insert(EventCategory::Filesystem, 50);
        replacement_categories.insert(EventCategory::Network, 10);

        let baseline = BehavioralProfile {
            event_count: 50,
            categories: baseline_categories,
        };

        let replacement = BehavioralProfile {
            event_count: 60,
            categories: replacement_categories,
        };

        let comparison = compare_behaviors(&baseline, &replacement);

        assert!(!comparison.new_categories.is_empty());
        assert!(comparison.new_categories.contains(&EventCategory::Network));
        assert!(comparison.missing_categories.is_empty());
    }

    #[test]
    fn extracts_syscalls_from_common_strace_formats() {
        assert_eq!(
            syscall_name("openat(AT_FDCWD, \"Cargo.toml\", O_RDONLY) = 3"),
            Some("openat")
        );
        assert_eq!(
            syscall_name("[pid 1234] 12:30:01.123456 connect(3, 0x0, 16) = 0"),
            Some("connect")
        );
        assert_eq!(syscall_name("+++ exited with 0 +++"), None);
    }
}
