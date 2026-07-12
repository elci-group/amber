# S-Grade Roadmap — Retired

**Retired 2026-07-11.** Every milestone and every remaining item in this
document has been completed (see `CHANGELOG.md` for 0.2.6 and 0.3.0, and the
traceability appendix in [`ENTERPRISE_ASSESSMENT.md`](ENTERPRISE_ASSESSMENT.md)).

The guardrails below remain in force and are now enforced by CI:

- `cargo clippy --all-targets -- -W clippy::pedantic -W clippy::nursery -D warnings` clean.
- Line coverage ≥95% (enforced via `cargo tarpaulin --fail-under 95`).
- `cargo audit` clean, run per-PR and weekly.
- Every new feature includes tests; refactors preserve or improve coverage.
- CI jobs stay deterministic and offline-safe by default.

Forward-looking work is tracked in
[`docs/roadmap/RELEASE_READINESS.md`](docs/roadmap/RELEASE_READINESS.md).
