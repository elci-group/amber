# Amber Enterprise-Grade Assessment

**Version assessed:** 0.3.0
**Date:** 2026-07-11
**Assessor:** Automated code-quality review
**Scope:** Source code, tests, CI, documentation, security posture, distribution, and operational readiness.
**Previous assessment:** [0.2.6, 2026-07-08](#appendix-a-traceability-from-the-026-assessment)

---

## Executive Summary

Amber 0.3.0 is a well-engineered, library-first Rust CLI: 214 passing tests,
95% line coverage, zero clippy warnings under pedantic + nursery, no `unsafe`,
and a mature local toolchain (fmt/clippy/test/audit/coverage in CI, MSRV job,
cross-platform release builds with checksums and optional GPG signing).
Most of the previous assessment's A→S items are closed (see Appendix A).

**Overall grade: A-** — the engineering core is S-grade, but three
non-code issues block enterprise *release* readiness:

1. **The repository does not build standalone.** `Cargo.toml` declares
   `padagonia = { path = "../padagonia", optional = true }`. Cargo resolves
   path dependencies at manifest time regardless of features, so a fresh clone
   fails *every* build — including the default one — and every CI job on a
   clean GitHub Actions runner. This is the single release-blocking defect.
2. **Governance and supply-chain docs are missing.** No `SECURITY.md`,
   `CONTRIBUTING.md`, or code of conduct; no Dependabot; the git history was
   started 2026-07-11 and has no remote yet.
3. **The website publishes fictional releases.** `website/data/releases.json`
   lists a `0.3.0-beta.1` with "Groq LPU-powered migration hints for Amber Pro
   users" and there is a pricing page for a product that does not exist. For an
   enterprise audience this is a credibility liability.

None of these require code redesign; all are closable in days. The detailed
plan is in [`docs/roadmap/RELEASE_READINESS.md`](docs/roadmap/RELEASE_READINESS.md).

---

## Methodology

Grading scale: **S** exceptional, **A** production-ready, **B** solid with
gaps, **C** significant gaps, **D** below standard, **F** critical.

Evidence:

- `cargo test --all-targets` — 214 passed, 0 failed (194 lib + 13 integration + 7 CLI)
- `cargo clippy --all-targets --all-features -- -W clippy::pedantic -W clippy::nursery -D warnings` — clean
- `cargo fmt --check` — clean
- `lcov.info` — 95.0% line coverage (generated 2026-07-08, pre-dates the 0.3.0
  unused-detection fix; re-baseline due)
- Standalone-build reproduction of the path-dependency failure
- Manual review of source, CI workflows, website data, and documentation

---

## Cross-Cutting Metrics

| Metric | Value | Target | Grade |
|--------|-------|--------|-------|
| Lines of source (src/) | ~10,600 | — | — |
| Test pass rate | 214/214 | 100% | S |
| Clippy (pedantic + nursery) | Clean | Clean | S |
| rustfmt | Clean | Clean | S |
| `cargo audit` | 0 vulns (CI gate) | 0 | S |
| Line coverage | 95.0% (stale by 3 days) | ≥95% | A |
| `unsafe` code | None | None | S |
| MSRV | 1.80 declared + CI job | Declared + enforced | S |
| Standalone build | **Fails** (path dep `../padagonia`) | Must pass | F |
| Feature flags | `online`, `library` | Present | A |
| Release signing | GPG, optional/unarmed | Armed + documented | B |
| SBOM / provenance | None | Required for GA | D |

---

## Area Assessments

### Engineering core (unchanged strengths)

| Area | Grade | Notes |
|------|-------|-------|
| Code quality | S | Strict lints, no unsafe, `#![deny(clippy::unwrap_used)]`. |
| Test coverage | S | 214 tests; fixtures cover renamed imports, glob imports, derive macros, path deps. |
| Architecture | A | CLI refactored from a 1,200-line `main.rs` into `src/cli/` subcommand modules. |
| Performance | A | Synchronous, no runtime; Criterion benches for hot paths and the library feature. |
| Observability | B+ | Structured `tracing` logging; no metrics (acceptable for a CLI). |
| Documentation | A- | README, man page, ARCHITECTURE, MIGRATION, OPERATOR_RUNBOOK, directives, VHS demos. |

### Correctness of analysis — B+ (up from B)

- **Fixed today (0.3.0):** hyphenated crates (`comfy-table` seen as
  `comfy_table`) and call-site-only dependencies (`toml::from_str`) were
  reported as unused in every output format. Self-analysis now shows real usage
  for all 14 direct dependencies.
- **Remaining by-design limitation:** syntax-only method-call attribution can
  misattribute `x.foo()` without type inference. Documented and bounded; not a
  blocker for a recommendation tool, but must be disclosed in enterprise docs.

### Security — A-

- RustSec advisory integration with XDG-compliant cache (fixed since 0.2.6).
- CVE count overrides classification to `SecurityCritical`; never-replace list
  for crypto/TLS/runtime crates.
- **Open:** `--output`/`out_dir` paths are not validated against traversal;
  `git`/`cargo` subprocesses inherit the user's environment; `Validator` runs
  `cargo check` unsandboxed. All acceptable for a local dev tool, all must be
  documented or hardened before GA.

### CI / Build / Release — C (regressed from A)

- **Blocker:** standalone build failure (see Executive Summary). Every job in
  `.github/workflows/ci.yml` (`build --all-features`, MSRV, coverage) fails on
  a clean runner because `../padagonia` is absent.
- Present and good: fmt/clippy/tests/audit gate, MSRV job read from
  `Cargo.toml`, Linux/macOS/Windows release matrix, SHA-256 checksums,
  upload-before-sign ordering, GPG signing that no-ops cleanly without a key.
- **Missing:** Dependabot, SBOM generation, provenance attestation, scheduled
  (not just PR-triggered) `cargo audit`, coverage gate that fails under 95%,
  self-analysis regression gate (roadmap item).

### Distribution & governance — D

- Not published to crates.io; install is `cargo install --path .`.
- No `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`.
- Git history initialized 2026-07-11; no remote configured; previously the tree
  lived untracked inside another repository.
- Website contains fabricated release notes and a pricing page (see Executive
  Summary, item 3).
- `Cargo.toml` metadata otherwise complete (license, repository, description).

### Configuration — A (up from B+)

- Weights are now wired: `SafetyClassifier::with_weights`, validated
  (non-negative, finite, sum ≈ 1.0) in `src/config/mod.rs`, with a test proving
  custom weights change scores.
- Remaining minor gap: no published JSON/TOML schema for `.amber.toml`.

### Replacement generation — B+ (up from B)

- Templates added for `tracing`, `tracing-subscriber`, `toml`, `ureq`;
  validation via `cargo check` before reporting; `used_in_public_api` now
  populated and consumed by validation strategy.
- Still compile-only validation (no behavioral equivalence) and hard-coded
  compile-time/binary-size estimates — acceptable, must stay documented.

---

## Aggregate Scorecard

| Area | 0.2.6 | 0.3.0 | Delta |
|------|-------|-------|-------|
| Code quality | A | S | ⬆ |
| Test coverage | S | S | — |
| Architecture | A | A | — |
| Security | A- | A- | — |
| Performance | A | A | — |
| Observability | B+ | B+ | — |
| Configurability | B+ | A | ⬆ |
| Operational readiness | A | C | ⬇ (standalone build) |
| Documentation | A- | A- | — |
| Correctness / heuristics | B+ | B+ | ⬆ within grade (unused-detection fix) |
| Distribution & governance | — | D | new axis |

**Overall: A-** — S-grade engineering core, release blocked by buildability,
governance, and distribution gaps.

---

## Appendix A: Traceability from the 0.2.6 assessment

| 0.2.6 A→S item | Status at 0.3.0 |
|----------------|-----------------|
| Wire config weights into scoring | ✅ Done (`with_weights` + validation + test) |
| Standard cache directory for RustSec DB | ✅ Done (XDG_CACHE_HOME, `src/metadata/rustsec.rs:26`) |
| Populate `used_in_public_api` | ✅ Done (visitor tracks `pub` items/impls) |
| MSRV policy and CI job | ✅ Done (`rust-version = "1.80"`, `msrv` job) |
| Robust transitive resolution | ✅ Done (full `cargo_metadata` resolve graph, renamed deps) |
| SARIF rule metadata | ✅ Done (`fullDescription`, `defaultConfiguration`, `help`) |
| Signed checksums + cross-platform builds | ✅ Done (matrix builds, SHA-256, optional GPG) — signing unarmed |
| Crate-aware API-count estimate | ✅ Done (`Dependency.public_api_count` with LOC fallback) |
| Refresh stale `COVERAGE_95_ROADMAP.md` | ⚠️ Still present; superseded by this document |
| Unused-dependency false positives (found after 0.2.6) | ✅ Done (0.3.0, hyphenated + call-site-only) |

## Appendix B: Known limitations to disclose to enterprise users

1. Syntax-only analysis: method calls without imports may be misattributed.
2. Replacement validation is compile-only, not behavioral.
3. Compile-time/binary-size estimates are heuristic lookup tables.
4. `online` and `library` features are off by default; offline scoring uses
   neutral metadata defaults.
