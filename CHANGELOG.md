# Changelog

All notable changes to Amber are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Output paths for `replace`, `directives`, and `analyze -o` are validated
  against traversal outside the target project root.
- Fuzz target for `.amber.toml` parsing (`fuzz/`, PR smoke-test job).
- Dependency scorecard in `DEPENDENCIES.md`; self-analysis CI gate that fails
  PRs growing the direct dependency count.

### Changed

- **MSRV raised from 1.80 to 1.85** (enforced by the `msrv` CI job).
- `rustsec` updated to 0.31, fixing advisory-database load failures on
  CVSS 4.0 advisories (the whole DB previously failed to open, silently
  zeroing CVE counts).

### Fixed

- Optional `migrate` feature adding the `amber migrate <crate> --replace-with
  <FILE> [--dry-run]` subcommand. It applies a validated replacement module to
  the target project end to end: verifies the crate is a direct dependency,
  copies the module into `src/`, declares it in `lib.rs`/`main.rs`, rewrites
  `use <crate>::` imports and fully-qualified paths to `crate::<module>::`,
  removes the dependency from `[dependencies]`/`[dev-dependencies]` via
  `toml_edit`, and runs `cargo check`. Every change is rolled back from
  in-memory snapshots when the check fails; `--dry-run` previews the plan.
  Covered by the `tests/fixtures/migrate_project` fixture and
  `tests/migrate_tests.rs`.

## [0.3.0] - 2026-07-11

### Added

- Operator runbook covering CI integration, exit codes, SARIF consumption,
  RustSec DB caching, false positives, and rollback procedures.
- Cross-platform release job that builds Linux, macOS, and Windows binaries,
  computes SHA-256 checksums, optionally GPG-signs them, and attaches artifacts
  to GitHub releases.
- Internal std-only replacements for `colored` (`src/reporting/style.rs`),
  `walkdir` (`src/analysis/walker.rs`), and `tempfile` (`src/temp.rs`).
- Technical directives for the three removed dependencies under
  `docs/directives/`.
- Optional `library` feature that stores generated replacement modules in a
  Padagonia graph database. Adds the `library` CLI subcommand (`list`,
  `import`, `export`, `path`), `--library`/`--no-library` flags, a `[library]`
  config section, and automatic reuse of stored modules during `replace` and
  `analyze --propose`.
- `amber_library` Criterion benchmark suite covering library lookup, insert,
  list, save/load roundtrip, and generator integration with and without the
  library. Results recorded in `docs/benchmarks/library.md`.
- Replacement templates for `tracing`, `tracing-subscriber`, `toml`, and `ureq`
  in `src/replacement/template_data/` with validation tests.
- `library search` and `library remove` subcommands for querying and pruning the
  Padagonia replacement library.
- v0.3 self-hosting roadmap in `docs/roadmap/V0_3_SELF_HOSTING.md`.

### Changed

- Refactored the CLI from a 1,200-line `src/main.rs` into a `src/cli/`
  subcommand module hierarchy (`analyze`, `list`, `score`, `replace`,
  `directives`, `roadmap`, and `library`). `src/main.rs` is now a thin wrapper
  around `amber::cli::run_cli`.

- `CargoUsage.used_in_public_api` is now populated by tracking usages inside
  `pub` items and `impl` blocks for external traits/types.
- API-coverage estimation now uses `Dependency.public_api_count` when available
  and falls back to a LOC-based heuristic instead of a hard-coded value.
- Transitive dependency resolution now uses the full `cargo_metadata` resolve
  graph, supports renamed dependencies, and walks the complete transitive
  closure.
- Removed direct dependencies on `colored`, `walkdir`, and `tempfile` from
  `Cargo.toml`; functionality is now provided by internal std-only modules.

### Fixed

- Strengthened SARIF test assertions to verify rule metadata such as
  `fullDescription`, `defaultConfiguration`, and `help`.
- Usage analysis now detects hyphenated crates (e.g. `comfy-table` imported as
  `comfy_table`) and dependencies used only through fully-qualified call sites
  (e.g. `toml::from_str`), which were previously misreported as unused in all
  report formats, scores, and directives.

## [0.2.6] - 2026-07-09

### Added

- SARIF output format for integration with static-analysis tooling.
- JSON, PR/Markdown, and emoji report formatters.
- Replacement proposal generator with automated `cargo check` validation.
- Configurable safety-classifier weights via `.amber.toml`.
- RustSec advisory database enrichment for CVE counts.

### Changed

- Improved `cargo_metadata` integration for workspace and path-dependency
  projects.
- Refined usage analysis to detect type references, method-call heuristics,
  derive attributes, and macro invocations.

## [0.2.0] - 2026-06-10

### Added

- `amber analyze`, `list`, `score`, and `replace` subcommands.
- Dependency usage analysis based on `syn` AST traversal.
- Safety classifier with `SafeToReplace` through `SecurityCritical` classes.

### Changed

- Project restructured as a library crate with a thin CLI wrapper.

## [0.1.0] - 2026-05-20

### Added

- Initial release of the Amber dependency analyzer.
- Basic `Cargo.toml` dependency listing and unused-dependency detection.
