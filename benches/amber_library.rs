//! Benchmarks for the Padagonia-backed replacement library.

use amber::analysis::repo::RepositoryAnalyzer;
use amber::analysis::usage::UsageAnalyzer;
use amber::library::{EntrySource, LibraryEntry, LibraryStore};
use amber::replacement::Generator;
use amber::scoring::classifier::SafetyClassifier;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_library_path() -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("amber-bench-lib-{}-{id}.pad", std::process::id()))
}

fn make_entry(i: usize) -> LibraryEntry {
    LibraryEntry::new(
        format!("crate_{i}"),
        format!("amber_crate_{i}"),
        format!("pub fn generated_{i}() {{}}\n"),
        EntrySource::Generated,
    )
}

fn populated_store(size: usize) -> (LibraryStore, PathBuf) {
    let path = temp_library_path();
    let mut store = LibraryStore::open(&path).expect("open library");
    let entries: Vec<_> = (0..size).map(make_entry).collect();
    store.insert_many(&entries);
    (store, path)
}

fn bench_open_empty(c: &mut Criterion) {
    c.bench_function("library_open_empty", |b| {
        b.iter(|| {
            let path = temp_library_path();
            let store = LibraryStore::open(&path).expect("open");
            black_box(store);
        });
    });
}

fn bench_insert_single(c: &mut Criterion) {
    c.bench_function("library_insert_single", |b| {
        b.iter(|| {
            let path = temp_library_path();
            let mut store = LibraryStore::open(&path).expect("open");
            store.insert(&make_entry(0)).expect("insert");
            black_box(());
        });
    });
}

fn bench_find(c: &mut Criterion) {
    let mut group = c.benchmark_group("library_find");
    for size in [10usize, 100, 1_000] {
        let (store, _path) = populated_store(size);
        let target = format!("crate_{}", size / 2);

        group.bench_with_input(BenchmarkId::new("hit", size), &size, |b, _| {
            b.iter(|| {
                let entry = store.find(&target);
                black_box(entry);
            });
        });

        group.bench_with_input(BenchmarkId::new("miss", size), &size, |b, _| {
            b.iter(|| {
                let entry = store.find("not_present");
                black_box(entry);
            });
        });
    }
    group.finish();
}

fn bench_list(c: &mut Criterion) {
    let mut group = c.benchmark_group("library_list");
    for size in [10usize, 100, 1_000] {
        let (store, _path) = populated_store(size);
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                let entries = store.list();
                black_box(entries);
            });
        });
    }
    group.finish();
}

fn bench_save_load_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("library_save_load_roundtrip");
    for size in [10usize, 100, 1_000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            b.iter(|| {
                let path = temp_library_path();
                {
                    let mut store = LibraryStore::open(&path).expect("open");
                    let entries: Vec<_> = (0..size).map(make_entry).collect();
                    store.insert_many(&entries);
                    store.save().expect("save");
                }
                let reloaded = LibraryStore::open(&path).expect("reload");
                black_box(reloaded.list().len());
            });
        });
    }
    group.finish();
}

fn fixture_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample_project")
        .join("Cargo.toml")
}

fn bench_generator_without_library(c: &mut Criterion) {
    let manifest = fixture_manifest();
    let analyzer = RepositoryAnalyzer::new(&manifest).unwrap();
    let dep = analyzer.get_dependency("anyhow").unwrap();
    let usage = UsageAnalyzer::new(&manifest).unwrap();
    let usage_stats = usage.analyze_crate_usage("anyhow").unwrap();
    let classifier = SafetyClassifier::new();
    let score = classifier.score_dependency(&dep, &usage_stats);
    let generator = Generator::new(PathBuf::from("benches_out"));

    c.bench_function("generator_without_library", |b| {
        b.iter(|| {
            let proposal = generator
                .generate_replacement("anyhow", &usage_stats, &score)
                .unwrap();
            black_box(proposal);
        });
    });
}

fn bench_generator_with_library_miss(c: &mut Criterion) {
    let manifest = fixture_manifest();
    let analyzer = RepositoryAnalyzer::new(&manifest).unwrap();
    let dep = analyzer.get_dependency("anyhow").unwrap();
    let usage = UsageAnalyzer::new(&manifest).unwrap();
    let usage_stats = usage.analyze_crate_usage("anyhow").unwrap();
    let classifier = SafetyClassifier::new();
    let score = classifier.score_dependency(&dep, &usage_stats);
    let generator = Generator::new(PathBuf::from("benches_out"));

    c.bench_function("generator_with_library_miss", |b| {
        b.iter(|| {
            let path = temp_library_path();
            let mut lib_store = LibraryStore::open(&path).expect("open");
            let proposal = generator
                .generate_replacement_with_library("anyhow", &usage_stats, &score, &mut lib_store)
                .unwrap();
            black_box(proposal);
        });
    });
}

fn bench_generator_with_library_hit(c: &mut Criterion) {
    let manifest = fixture_manifest();
    let analyzer = RepositoryAnalyzer::new(&manifest).unwrap();
    let dep = analyzer.get_dependency("anyhow").unwrap();
    let usage = UsageAnalyzer::new(&manifest).unwrap();
    let usage_stats = usage.analyze_crate_usage("anyhow").unwrap();
    let classifier = SafetyClassifier::new();
    let score = classifier.score_dependency(&dep, &usage_stats);
    let generator = Generator::new(PathBuf::from("benches_out"));

    // Pre-populate the library with a stored module so every iteration is a hit.
    let path = temp_library_path();
    let mut lib_store = LibraryStore::open(&path).expect("open");
    let seed = LibraryEntry::new(
        "anyhow",
        "amber_anyhow",
        "pub fn anyhow() {}",
        EntrySource::Generated,
    );
    lib_store.insert(&seed).expect("seed");

    c.bench_function("generator_with_library_hit", |b| {
        b.iter(|| {
            let proposal = generator
                .generate_replacement_with_library("anyhow", &usage_stats, &score, &mut lib_store)
                .unwrap();
            black_box(proposal);
        });
    });
}

fn criterion_config() -> Criterion {
    Criterion::default()
        .measurement_time(Duration::from_secs(1))
        .warm_up_time(Duration::from_millis(500))
        .sample_size(10)
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets =
        bench_open_empty,
        bench_insert_single,
        bench_find,
        bench_list,
        bench_save_load_roundtrip,
        bench_generator_without_library,
        bench_generator_with_library_miss,
        bench_generator_with_library_hit,
}
criterion_main!(benches);
