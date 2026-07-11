use std::path::PathBuf;
use std::process::Command;

fn bin_path() -> PathBuf {
    option_env!("CARGO_BIN_EXE_amber")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.push("target/debug/amber");
            path
        })
}

fn fixture_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures");
    path.push(name);
    path
}

fn run(args: &[&str]) -> (std::process::ExitStatus, String, String) {
    let mut cmd = Command::new(bin_path());
    cmd.args(args).env("NO_COLOR", "1");
    let output = cmd.output().expect("failed to run amber binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status, stdout, stderr)
}

#[test]
fn help_shows_description() {
    let (status, stdout, _) = run(&["--help"]);
    assert!(status.success(), "help should exit successfully");
    assert!(stdout.contains("Dependency garbage collector"));
}

#[test]
fn roadmap_command_prints_roadmap() {
    let (status, stdout, _) = run(&["roadmap"]);
    assert!(status.success());
    assert!(stdout.contains("Amber Internal Module Roadmap"));
}

#[test]
fn list_command_shows_dependencies() {
    let path = fixture_path("sample_project");
    let (status, stdout, _) = run(&[path.to_str().unwrap(), "list"]);
    assert!(status.success());
    assert!(stdout.contains("anyhow"));
}

#[test]
fn analyze_command_outputs_report() {
    let path = fixture_path("sample_project");
    let (status, stdout, _) = run(&["--threshold", "100", path.to_str().unwrap(), "analyze"]);
    assert!(status.success());
    assert!(stdout.contains("Amber Dependency Analysis Report"));
}

#[test]
fn analyze_json_outputs_valid_report() {
    let path = fixture_path("sample_project");
    let (status, stdout, _) = run(&[
        "--format",
        "json",
        "--threshold",
        "100",
        path.to_str().unwrap(),
        "analyze",
    ]);
    assert!(status.success());
    assert!(stdout.contains("\"amber_version\""));
    assert!(stdout.contains("\"results\""));
}

#[test]
fn analyze_pr_outputs_markdown() {
    let path = fixture_path("sample_project");
    let (status, stdout, _) = run(&[
        "--format",
        "pr",
        "--threshold",
        "100",
        path.to_str().unwrap(),
        "analyze",
    ]);
    assert!(status.success());
    assert!(stdout.contains("# Amber Dependency Reduction Report"));
}

#[test]
fn score_command_outputs_score_card() {
    let path = fixture_path("sample_project");
    let (status, stdout, _) = run(&[path.to_str().unwrap(), "score", "anyhow"]);
    assert!(status.success());
    assert!(stdout.contains("Score Card for anyhow"));
}

#[test]
fn replace_command_outputs_proposal() {
    let path = fixture_path("sample_project");
    let (status, stdout, _) = run(&[
        "--threshold",
        "0",
        path.to_str().unwrap(),
        "replace",
        "anyhow",
    ]);
    assert!(status.success());
    assert!(stdout.contains("Replacement Proposal"));
}

#[test]
fn analyze_sarif_outputs_valid_report() {
    let path = fixture_path("sample_project");
    let (_status, stdout, _) = run(&[
        "--format",
        "sarif",
        "--threshold",
        "0",
        path.to_str().unwrap(),
        "analyze",
    ]);
    assert!(stdout.contains("\"version\": \"2.1.0\""));
}

#[test]
fn missing_manifest_errors() {
    let (status, _, stderr) = run(&["/definitely/not/a/project", "list"]);
    assert!(!status.success());
    assert!(stderr.contains("error:"));
}

#[test]
fn missing_crate_for_score_errors() {
    let path = fixture_path("sample_project");
    let (status, _, stderr) = run(&[path.to_str().unwrap(), "score", "definitely_missing_crate"]);
    assert!(!status.success());
    assert!(stderr.contains("error:"));
}

#[cfg(not(feature = "online"))]
#[test]
fn online_flag_without_feature_errors() {
    let path = fixture_path("sample_project");
    let (status, _, stderr) = run(&["--online", path.to_str().unwrap(), "list"]);
    assert!(!status.success());
    assert!(stderr.contains("online"));
}

#[test]
fn verbose_flag_is_accepted() {
    let path = fixture_path("sample_project");
    let (status, _, _) = run(&["-v", path.to_str().unwrap(), "list"]);
    assert!(status.success());
}

#[cfg(feature = "library")]
fn temp_project_with_library(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::fs;
    let stamp = std::process::id();
    let base = std::env::temp_dir().join(format!("amber-library-cli-{stamp}-{label}"));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(base.join("src")).expect("create src");
    fs::write(
        base.join("Cargo.toml"),
        "[package]\nname = \"libtest\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("write manifest");
    fs::write(base.join("src/main.rs"), "fn main() {}\n").expect("write source");
    let library_path = base.join("library.pad");
    let config = format!(
        "[library]\nenabled = true\npath = \"{}\"\n",
        library_path.display()
    );
    fs::write(base.join(".amber.toml"), config).expect("write config");
    (base, library_path)
}

#[cfg(feature = "library")]
#[test]
fn library_list_empty_shows_message() {
    let (base, _) = temp_project_with_library("list-empty");
    let (status, stdout, _) = run(&[base.to_str().unwrap(), "library", "list"]);
    assert!(status.success());
    assert!(stdout.contains("No replacement modules stored"));
}

#[cfg(feature = "library")]
#[test]
fn library_path_prints_configured_path() {
    let (base, library_path) = temp_project_with_library("path");
    let (status, stdout, _) = run(&[base.to_str().unwrap(), "library", "path"]);
    assert!(status.success());
    assert!(stdout.contains(&library_path.display().to_string()));
}

#[cfg(feature = "library")]
#[test]
fn library_import_and_export_roundtrip() {
    use std::fs;
    let (base, library_path) = temp_project_with_library("import-export");
    let module_path = base.join("amber_demo.rs");
    fs::write(&module_path, "pub fn demo() {}\n").expect("write module");

    let (status, stdout, stderr) = run(&[
        base.to_str().unwrap(),
        "library",
        "import",
        module_path.to_str().unwrap(),
        "--crate-name",
        "demo",
    ]);
    assert!(status.success(), "import failed: {stderr}");
    assert!(stdout.contains("Imported replacement"));
    assert!(library_path.exists(), "library file should be created");

    let (status, stdout, _) = run(&[base.to_str().unwrap(), "library", "list"]);
    assert!(status.success());
    assert!(stdout.contains("demo"));

    let export_path = base.join("exported.rs");
    let (status, _, stderr) = run(&[
        base.to_str().unwrap(),
        "library",
        "export",
        "demo",
        "--out",
        export_path.to_str().unwrap(),
    ]);
    assert!(status.success(), "export failed: {stderr}");
    let exported = fs::read_to_string(&export_path).expect("read exported");
    assert!(exported.contains("pub fn demo()"));
}

#[cfg(feature = "library")]
#[test]
fn library_search_finds_entries() {
    use std::fs;
    let (base, _) = temp_project_with_library("search");
    let module_path = base.join("amber_search.rs");
    fs::write(&module_path, "pub fn search_fn() {}\n").expect("write module");

    let (status, _, stderr) = run(&[
        base.to_str().unwrap(),
        "library",
        "import",
        module_path.to_str().unwrap(),
        "--crate-name",
        "searchcrate",
    ]);
    assert!(status.success(), "import failed: {stderr}");

    let (status, stdout, _) = run(&[base.to_str().unwrap(), "library", "search", "search_fn"]);
    assert!(status.success());
    assert!(stdout.contains("searchcrate"));
}

#[cfg(feature = "library")]
#[test]
fn library_remove_deletes_entry() {
    use std::fs;
    let (base, _) = temp_project_with_library("remove");
    let module_path = base.join("amber_remove.rs");
    fs::write(&module_path, "pub fn remove_fn() {}\n").expect("write module");

    run(&[
        base.to_str().unwrap(),
        "library",
        "import",
        module_path.to_str().unwrap(),
        "--crate-name",
        "removecrate",
    ]);

    let (status, stdout, _) = run(&[base.to_str().unwrap(), "library", "remove", "removecrate"]);
    assert!(status.success());
    assert!(stdout.contains("Removed replacement"));

    let (status, stdout, _) = run(&[base.to_str().unwrap(), "library", "list"]);
    assert!(status.success());
    assert!(!stdout.contains("removecrate"));
}
