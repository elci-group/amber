# Amber Enterprise-Grade Assessment

**Version assessed:** 0.3.0 + daemon, migrate, and post-release readiness work
**Date:** 2026-07-20
**Assessor:** Automated code-quality and operability review
**Scope:** Feature coverage, source code, tests, CI, documentation, security
posture, distribution, daemon operation, and human operability.

---

## Executive summary

Amber has a strong engineering core and is suitable for serious internal use as
a Rust dependency-reduction assistant. The CLI is library-first, offline by
default, strictly linted, well-tested, and produces human and machine-readable
outputs. The optional daemon now adds workstation monitoring, `.amber` marker
creation, dependency-diff triggering, Padagonia-backed analysis caching,
exclusive cache locking, and bounded cache retention.

**Current grade:** A- engineering quality, B+ enterprise readiness, B+ human
operability.

The remaining enterprise GA blockers are distribution and operational polish:
publish or vendor `padagonia` so `cargo publish` can pass, arm and publish GPG
release signing material, keep the optional Padagonia dependency chain clear of
unmaintained advisories, and run the daemon under real long-lived workstation
loads before treating it as mature infrastructure.

---

## Evidence

- `cargo fmt --check` — clean.
- `cargo clippy --all-targets --all-features -- -D warnings` — clean.
- `cargo test --all-targets --all-features` — clean:
  - 237 library/unit tests;
  - 20 CLI tests;
  - 7 integration tests;
  - 5 migrate integration tests;
  - benches and examples build.
- `cargo audit --deny warnings --ignore RUSTSEC-2025-0141` — clean aside from
  the explicitly ignored optional Padagonia chain advisory.
- Self-analysis (`amber --format json --threshold 101 . analyze`) completed
  with exit code `1`, which is expected because Amber flags review candidates.
- Self-analysis found 16 direct dependencies and active usage for all 16.

Self-analysis classification summary:

| Classification | Count |
|----------------|-------|
| Low Risk | 11 |
| Medium Risk | 1 |
| Security Critical | 4 |

---

## Feature inventory

| Capability | Status | Notes |
|------------|--------|-------|
| Dependency inventory | Production-ready | Uses `cargo_metadata`; supports direct/transitive and workspace manifests. |
| Usage analysis | Strong, bounded | Syntax-tree analysis via `syn`; no type inference. |
| Replaceability scoring | Strong | Configurable weights and policy enforcement. |
| Reports | Strong | Console, JSON, PR Markdown, SARIF, emoji. |
| Replacement generation | Useful with review | Generated modules are compile-checked before reporting. |
| Technical directives | Useful | Produces scoped implementation guidance for crate replacement. |
| Migration | Promising | Feature-gated, rewrites imports, removes dependencies, validates, and rolls back on failure. |
| Padagonia library | Useful but distribution-gated | Git dependency blocks crates.io publishing until Padagonia is published or vendored. |
| Daemon | New, functional | Polling monitor, dependency fingerprints, `.amber/daemon.toml`, cache lock, retention controls. |
| Online metadata | Optional | crates.io fetches behind the `online` feature; default remains offline. |

Feature flags:

```text
default = []
online = ["dep:ureq"]
library = ["dep:padagonia"]
migrate = ["dep:toml_edit"]
daemon = ["library"]
```

---

## Enterprise readiness

| Area | Grade | Assessment |
|------|-------|------------|
| Code quality | S | Strict lints, no production `unsafe`, clear module boundaries. |
| Test posture | S | Broad unit/CLI/integration coverage; all-feature suite passes. |
| Security posture | A- | RustSec integration, audit gate, no unsafe; subprocess and generated-code validation remain local-trust operations. |
| CI | A | fmt, clippy, builds, tests, audit, MSRV, coverage, fuzz smoke, self-analysis, release artifact flow. |
| Release supply chain | B | Checksums, SBOM, provenance, optional GPG; signing keys and crates.io path still incomplete. |
| Distribution | C+ | Source installs work; crates.io is blocked by git `padagonia`. |
| Operability | B+ | Runbook, SARIF, exit codes, daemon docs, cache locking; daemon still needs long-running field data. |
| Observability | B | Structured tracing exists; no metrics endpoint, health command, or daemon status command. |

---

## Daemon readiness

The daemon is intentionally lightweight:

- defaults to monitoring `$HOME`;
- discovers Amber-compatible `Cargo.toml` files;
- skips heavy directories such as `.git`, `target`, `node_modules`, `.cache`,
  `.cargo`, `.rustup`, `dist`, and `build`;
- writes `.amber/daemon.toml` markers;
- hashes dependency tables for near-instant change detection;
- avoids full analysis for source-only edits unless `--analyze-source-changes`
  is enabled;
- stores JSON analysis snapshots in a Padagonia cache;
- holds an exclusive `.lock` file next to the cache;
- compacts to the latest entry per project and evicts missing/stale entries.

Remaining daemon gaps:

1. No native `status`, `stop`, or health-check subcommand.
2. No first-party systemd installer; docs provide a unit template only.
3. No resource telemetry beyond logs and scan summaries.
4. No soak-test data on very large home directories.
5. RustSec advisory cache lock contention can still appear during parallel test
   or analysis runs.

---

## Human operability

Strengths:

- Clear CLI help and examples.
- `--once` enables safe daemon trials and CI smoke tests.
- Exit codes are documented and CI-friendly.
- SARIF works for security/code-scanning workflows.
- JSON output supports agent and automation consumption.
- `.amber.toml` policy lets teams suppress or enforce known decisions.
- Path validation prevents report/proposal writes outside the project root.

Recently fixed:

- Local PATH shadowing was corrected on this workstation by replacing the stale
  `/home/sal/.local/bin/amber` with the daemon-capable binary.

Remaining UX gaps:

1. Feature-gated commands are invisible unless users install the right feature
   set; docs must keep showing exact install commands.
2. `amber daemon` needs a status-oriented operator surface.
3. Replacement proposals still require human review and behavioral tests.
4. The manual page should be regenerated to include `daemon` and `migrate`.

---

## GA blockers

1. Publish `padagonia` to crates.io or vendor the required storage layer.
2. Make `cargo publish --dry-run` pass for tags.
3. Publish and document the GPG signing key; require signed checksums for GA.
4. Add daemon soak testing against a large synthetic tree.
5. Add daemon status/health output.
6. Regenerate the man page for feature-gated commands.
7. Re-baseline coverage after daemon and migrate additions.

---

## Known limitations to disclose

1. Syntax-only analysis can miss macro-expanded, generated, or type-inferred
   usage.
2. Replacement validation is compile-only, not behavioral equivalence.
3. Compile-time and binary-size estimates are heuristic.
4. Online metadata requires the `online` feature and network access.
5. The daemon is a polling monitor, not an OS-native filesystem event watcher.
6. Daemon cache entries contain full JSON analysis snapshots; treat the cache as
   local developer metadata.
