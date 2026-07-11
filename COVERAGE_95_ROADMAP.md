# S-Grade Roadmap

Amber has reached **95.0% line coverage** and an overall enterprise grade of **A**.
This document tracks the remaining work to move the project from **A → S**.

## Achieved ✅

| Milestone | Status |
|-----------|--------|
| Line coverage ≥95% | **95.0%** |
| `cargo clippy` clean with pedantic + nursery | ✅ |
| `cargo audit` clean (0 vulns, 0 warnings) | ✅ |
| 169/169 tests passing | ✅ |
| Config weights wired into `SafetyClassifier` | ✅ |
| RustSec DB cached in standard cache directory | ✅ |
| MSRV declared in `Cargo.toml` (`rust-version = "1.80"`) | ✅ |
| MSRV CI job | ✅ |

## Remaining S-grade work

| # | Item | Impact | File(s) |
|---|------|--------|---------|
| 1 | Populate `used_in_public_api` in usage visitor | Medium | `src/analysis/usage.rs`, `src/analysis/types.rs` |
| 2 | Improve transitive dependency resolution with full `cargo_metadata` | Medium | `src/analysis/repo.rs` |
| 3 | Add SARIF tool component descriptor and rule metadata | Medium | `src/reporting/formatters.rs` |
| 4 | Signed release checksums + cross-platform CI builds | Medium | `.github/workflows/ci.yml` |
| 5 | Replace hard-coded API count (`50.0`) with crate-aware estimate | Low-Medium | `src/scoring/classifier.rs` |
| 6 | Operator runbook / CI integration docs | Low | `docs/` or `README.md` |
| 7 | Changelog and migration guide | Low | `CHANGELOG.md` |

## Quality guardrails

- Maintain `cargo clippy --all-targets --all-features -- -W clippy::pedantic -W clippy::nursery -D warnings`.
- Keep line coverage ≥95%.
- Keep `cargo audit` clean.
- Every new feature includes tests; every refactor preserves or improves coverage.
- All CI jobs must remain deterministic and offline-safe by default.
