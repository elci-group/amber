//! Basic library usage example.
//!
//! Run with:
//!
//! ```bash
//! cargo run --example basic -- /path/to/rust/project
//! ```

use amber::analysis::repo::RepositoryAnalyzer;
use amber::analysis::usage::UsageAnalyzer;
use amber::scoring::classifier::SafetyClassifier;
use std::env;
use std::path::PathBuf;

fn main() -> amber::amber_anyhow::Result<()> {
    let path: PathBuf = env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("."), PathBuf::from);

    let manifest = if path.join("Cargo.toml").exists() {
        path.join("Cargo.toml")
    } else {
        path
    };

    let analyzer = RepositoryAnalyzer::new(&manifest)?;
    let deps = analyzer.list_dependencies(false, true)?;
    println!("Found {} dependencies", deps.len());

    let usage_analyzer = UsageAnalyzer::new(&manifest)?;
    let usage = usage_analyzer.analyze_all_usage(&deps)?;

    let classifier = SafetyClassifier::new();
    for dep in &deps {
        let dep_usage = usage.get(&dep.name).cloned().unwrap_or_default();
        let score = classifier.score_dependency(dep, &dep_usage);
        println!(
            "{}: score={}/100 confidence={}/100 {:?}",
            dep.name, score.overall, score.confidence, score.recommendation
        );
    }

    Ok(())
}
