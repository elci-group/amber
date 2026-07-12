# Amber Release-Readiness Roadmap

**Target:** enterprise general availability (v1.0.0)
**Baseline:** v0.3.0 — see [`ENTERPRISE_ASSESSMENT.md`](../../ENTERPRISE_ASSESSMENT.md)
  (overall grade A-; engineering core S-grade)
**Date:** 2026-07-11

## Release-readiness definition

Amber is release-ready when **all** of the following hold:

1. A fresh clone builds and tests green on Linux, macOS, and Windows with
   default and `--all-features` — no machine-specific path dependencies.
2. Every release artifact carries a SHA-256 checksum, a GPG signature from a
   published key, and a CycloneDX SBOM.
3. `cargo audit` and Dependabot run on a schedule, not just per-PR.
4. Governance docs exist: `SECURITY.md`, `CONTRIBUTING.md`,
   `CODE_OF_CONDUCT.md`, and a documented support/MSRV policy.
5. Coverage stays ≥95% and is enforced as a failing CI gate.
6. Public-facing material (website, releases feed, README) describes only what
   actually exists and ships.
7. The tool is installable via `cargo install amber` from crates.io.

---

## Phase 0 — Unblock the build (v0.3.1, days)

| # | Work item | Acceptance |
|---|-----------|------------|
| 0.1 | Replace `padagonia = { path = "../padagonia" }` in `Cargo.toml` with a buildable source: publish padagonia to crates.io and depend by version, or vendor it into the workspace, or make it a `git` dependency. | `git clone` → `cargo build --all-features` passes on a clean machine |
| 0.2 | Push the repository to `github.com/elci-group/amber`; confirm every job in `.github/workflows/ci.yml` passes on the real runner (including `msrv` and `coverage`). | CI green on `main` |
| 0.3 | Re-baseline coverage after the 0.3.0 fixes (`cargo tarpaulin --all-targets --all-features`); regenerate `lcov.info`. | Coverage ≥95% recorded in CI |
| 0.4 | Retire `COVERAGE_95_ROADMAP.md` (goal met); point readers at this roadmap. | File removed or replaced with a pointer |

## Phase 1 — Enterprise hardening (v0.3.x, 1–2 weeks)

| # | Work item | Acceptance |
|---|-----------|------------|
| 1.1 | Validate `--output` / `out_dir` paths: reject traversal outside the target project (canonicalize + prefix check) in `replace` and `directives`. | Unit tests for `../` rejection |
| 1.2 | Add `SECURITY.md` (vulnerability reporting, supported versions), `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`. | Files at repo root, linked from README |
| 1.3 | Add Dependabot for Cargo and GitHub Actions; schedule weekly `cargo audit`. | `.github/dependabot.yml` + scheduled workflow |
| 1.4 | Enforce coverage: fail CI below 95% (`tarpaulin --fail-under 95`, config in `tarpaulin.toml`). | CI red on regression |
| 1.5 | Self-analysis gate (from `V0_3_SELF_HOSTING.md`): run `amber --format json .` in CI; fail if direct dependency count rises vs. `main` or a removed dep reappears. | Gate live on PRs |
| 1.6 | Purge fictional website content: the `0.3.0-beta.1` entry ("Groq LPU", "Amber Pro") in `website/data/releases.json` and the pricing page, or label them explicitly as demo content. | Site describes only real artifacts |
| 1.7 | Document known analysis limitations (assessment Appendix B) in `docs/` and the man page. | Limitations section published |

## Phase 2 — General availability (v1.0.0, 2–4 weeks)

| # | Work item | Acceptance |
|---|-----------|------------|
| 2.1 | Publish to crates.io (verify name availability; `cargo publish --dry-run` gate in CI on tags). | `cargo install amber` works |
| 2.2 | SBOM: generate CycloneDX (`cargo cyclonedx`) per release artifact; attach to GitHub release. | SBOM present on v1.0.0 assets |
| 2.3 | Provenance: GitHub artifact attestations (`actions/attest-build-provenance`) for each binary. | `gh attestation verify` passes |
| 2.4 | Arm GPG signing: publish the public key, document `gpg --verify` in the operator runbook. | Signed `checksums.txt.asc` on the release |
| 2.5 | Fuzz the parsers (`.amber.toml`, target `Cargo.toml` handling) with `cargo-fuzz`; PR smoke gate. | Fuzz targets in `fuzz/`, CI job |
| 2.6 | Library API stability: semver policy for `amber` as a crate, MSRV policy doc, deprecation window. | Policy in `docs/` |
| 2.7 | `DEPENDENCIES.md` scorecard (self-hosting roadmap item 5). | File maintained per release |
| 2.8 | Decide `amber migrate`: implement behind a `migrate` feature flag with fixture tests and rollback, or explicitly descope from v1.0. | Decision recorded in CHANGELOG |

## Carry-over from the v0.3 self-hosting roadmap

- ✅ Replacement templates for `tracing`, `tracing-subscriber`, `toml`, `ureq`.
- ➡️ Self-analysis CI gate → item 1.5.
- ➡️ `amber migrate` → item 2.8.
- ➡️ Dependency scorecard → item 2.7.
- ➡️ Library-backed proposal reuse → shipped in 0.3.0 as the `library`
  feature; `library publish` and default seeding remain optional Phase 2
  stretch work.

## Definition of done (v1.0.0)

- All Phase 0 and Phase 1 items closed; Phase 2 items 2.1–2.4 closed.
- Release-readiness criteria 1–7 above verified end-to-end on the v1.0.0 tag.
- `ENTERPRISE_ASSESSMENT.md` re-graded: no axis below B.
