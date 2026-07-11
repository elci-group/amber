use amber::analysis::repo::RepositoryAnalyzer;
use amber::analysis::usage::UsageAnalyzer;
use amber::replacement::Generator;
use amber::scoring::classifier::SafetyClassifier;
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use std::path::PathBuf;

fn fixture_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample_project")
        .join("Cargo.toml")
}

fn bench_repo_analysis(c: &mut Criterion) {
    let manifest = fixture_manifest();
    c.bench_function("repo_analysis", |b| {
        b.iter(|| {
            let analyzer = RepositoryAnalyzer::new(&manifest).unwrap();
            let deps = analyzer.list_dependencies(false, true).unwrap();
            black_box(deps);
        });
    });
}

fn bench_usage_analysis(c: &mut Criterion) {
    let manifest = fixture_manifest();
    let analyzer = RepositoryAnalyzer::new(&manifest).unwrap();
    let deps = analyzer.list_dependencies(false, true).unwrap();

    c.bench_function("usage_analysis", |b| {
        b.iter(|| {
            let usage = UsageAnalyzer::new(&manifest).unwrap();
            let stats = usage.analyze_all_usage(&deps).unwrap();
            black_box(stats);
        });
    });
}

fn bench_scoring(c: &mut Criterion) {
    let manifest = fixture_manifest();
    let analyzer = RepositoryAnalyzer::new(&manifest).unwrap();
    let deps = analyzer.list_dependencies(false, true).unwrap();
    let usage = UsageAnalyzer::new(&manifest).unwrap();
    let stats = usage.analyze_all_usage(&deps).unwrap();
    let classifier = SafetyClassifier::new();

    c.bench_function("score_dependencies", |b| {
        b.iter(|| {
            let scores: Vec<_> = deps
                .iter()
                .map(|dep| {
                    let usage = stats.get(&dep.name).cloned().unwrap_or_default();
                    classifier.score_dependency(dep, &usage)
                })
                .collect();
            black_box(scores);
        });
    });
}

fn bench_replacement_generation(c: &mut Criterion) {
    let manifest = fixture_manifest();
    let analyzer = RepositoryAnalyzer::new(&manifest).unwrap();
    let dep = analyzer.get_dependency("anyhow").unwrap();
    let usage = UsageAnalyzer::new(&manifest).unwrap();
    let usage_stats = usage.analyze_crate_usage("anyhow").unwrap();
    let classifier = SafetyClassifier::new();
    let score = classifier.score_dependency(&dep, &usage_stats);
    let generator = Generator::new(PathBuf::from("benches_out"));

    c.bench_function("replacement_generation_anyhow", |b| {
        b.iter(|| {
            let proposal = generator
                .generate_replacement("anyhow", &usage_stats, &score)
                .unwrap();
            black_box(proposal);
        });
    });
}

criterion_group!(
    benches,
    bench_repo_analysis,
    bench_usage_analysis,
    bench_scoring,
    bench_replacement_generation
);
criterion_main!(benches);
