use amber::analysis::repo::RepositoryAnalyzer;
use amber::analysis::usage::UsageAnalyzer;
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures");
    path.push(name);
    path.push("Cargo.toml");
    path
}

#[test]
fn lists_all_dependencies() {
    let manifest = fixture_path("sample_project");
    let analyzer = RepositoryAnalyzer::new(&manifest).expect("valid metadata");
    let deps = analyzer.list_dependencies(false, true).expect("list deps");

    let names: Vec<_> = deps.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"anyhow"));
    assert!(names.contains(&"serde"));
    assert!(names.contains(&"serde_json"));
    assert!(names.contains(&"unused_crate"));
}

#[test]
fn detects_usage_of_anyhow_and_serde() {
    let manifest = fixture_path("sample_project");
    let analyzer = RepositoryAnalyzer::new(&manifest).expect("valid metadata");
    let deps = analyzer.list_dependencies(false, true).expect("list deps");

    let usage_analyzer = UsageAnalyzer::new(&manifest).expect("valid analyzer");
    let usage = usage_analyzer
        .analyze_all_usage(&deps)
        .expect("analyze usage");

    let anyhow = usage.get("anyhow").expect("anyhow usage");
    assert!(!anyhow.imported_items.is_empty(), "anyhow imports detected");
    assert!(
        anyhow
            .call_sites
            .iter()
            .any(|c| c.kind == amber::analysis::types::UsageKind::MacroInvocation),
        "anyhow::bail! macro invocation detected"
    );

    let serde = usage.get("serde").expect("serde usage");
    assert!(
        serde
            .call_sites
            .iter()
            .any(|c| c.kind == amber::analysis::types::UsageKind::Attribute),
        "serde derive attribute detected"
    );

    let unused = usage.get("unused_crate").expect("unused crate usage");
    assert!(
        unused.imported_items.is_empty(),
        "unused crate has no imports"
    );
    assert!(
        unused.call_sites.is_empty(),
        "unused crate has no call sites"
    );
}

#[test]
fn locations_have_non_zero_lines() {
    let manifest = fixture_path("sample_project");
    let analyzer = RepositoryAnalyzer::new(&manifest).expect("valid metadata");
    let deps = analyzer.list_dependencies(false, true).expect("list deps");

    let usage_analyzer = UsageAnalyzer::new(&manifest).expect("valid analyzer");
    let usage = usage_analyzer
        .analyze_all_usage(&deps)
        .expect("analyze usage");

    let anyhow = usage.get("anyhow").expect("anyhow usage");
    let import = anyhow.imported_items.first().expect("an import");
    assert!(import.location.line > 0, "line number is populated");
}

#[test]
fn detects_renamed_import_fixture() {
    let manifest = fixture_path("renamed_import");
    let analyzer = RepositoryAnalyzer::new(&manifest).expect("valid metadata");
    let deps = analyzer.list_dependencies(false, true).expect("list deps");

    let usage_analyzer = UsageAnalyzer::new(&manifest).expect("valid analyzer");
    let usage = usage_analyzer
        .analyze_all_usage(&deps)
        .expect("analyze usage");

    let serde_json = usage.get("serde_json").expect("serde_json usage");
    assert!(
        serde_json
            .call_sites
            .iter()
            .any(|c| c.function_name == "Value"),
        "renamed import is attributed to serde_json"
    );
}

#[test]
fn detects_glob_import_fixture() {
    let manifest = fixture_path("glob_import");
    let analyzer = RepositoryAnalyzer::new(&manifest).expect("valid metadata");
    let deps = analyzer.list_dependencies(false, true).expect("list deps");

    let usage_analyzer = UsageAnalyzer::new(&manifest).expect("valid analyzer");
    let usage = usage_analyzer
        .analyze_all_usage(&deps)
        .expect("analyze usage");

    let log = usage.get("log").expect("log usage");
    assert!(
        log.imported_items.iter().any(|i| i.name == "*"),
        "glob import detected"
    );
}

#[test]
fn detects_derive_macro_fixture() {
    let manifest = fixture_path("derive_macro");
    let analyzer = RepositoryAnalyzer::new(&manifest).expect("valid metadata");
    let deps = analyzer.list_dependencies(false, true).expect("list deps");

    let usage_analyzer = UsageAnalyzer::new(&manifest).expect("valid analyzer");
    let usage = usage_analyzer
        .analyze_all_usage(&deps)
        .expect("analyze usage");

    let serde = usage.get("serde").expect("serde usage");
    assert!(
        serde
            .call_sites
            .iter()
            .any(|c| c.kind == amber::analysis::types::UsageKind::Attribute),
        "derive macro detected"
    );
}

#[test]
fn detects_renamed_cargo_dependency() {
    let manifest = fixture_path("renamed_cargo_dependency");
    let analyzer = RepositoryAnalyzer::new(&manifest).expect("valid metadata");
    let deps = analyzer.list_dependencies(false, true).expect("list deps");

    let names: Vec<_> = deps.iter().map(|d| d.name.as_str()).collect();
    assert!(
        names.contains(&"helper"),
        "expected the declared (renamed) dependency name"
    );
    assert!(
        !names.contains(&"path_helper"),
        "original package name should not appear in the dependency list"
    );
}
