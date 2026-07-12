# Dependency Scorecard

Amber's own direct dependencies, scored by Amber itself. Regenerated at the
end of every release cycle (self-hosting roadmap, workstream 5).

**Generated:** 2026-07-11, amber v0.3.0, `amber --format json .`
**Direct dependencies:** 16 (14 normal, 1 optional, 1 dev)

| Crate | Score | Classification | Recommendation | Unique APIs | Call sites | Replacement status |
|-------|-------|----------------|----------------|-------------|------------|--------------------|
| cargo_metadata | 57 | Low Risk | propose | 5 | 7 | — |
| clap | 68 | Low Risk | propose | 2 | 3 | — |
| comfy-table | 60 | Low Risk | propose | 8 | 0 | — |
| criterion (dev) | 59 | Low Risk | propose | 4 | 0 | — |
| padagonia (optional) | 60 | Low Risk | propose | 6 | 0 | — |
| proc-macro2 | 68 | Low Risk | propose | 1 | 0 | keep — core AST parsing |
| rustsec | 68 | Low Risk | propose | 1 | 0 | — |
| semver | 68 | Low Risk | propose | 1 | 0 | — |
| serde | 20 | Security Critical | security_block | 2 | 42 | keep — security-sensitive |
| serde_json | 63 | Low Risk | propose | 1 | 14 | — |
| syn | 57 | Low Risk | propose | 16 | 42 | keep — core AST parsing |
| toml | 57 | Low Risk | propose | 0 | 4 | template ✅ |
| toml_edit (optional) | 68 | Low Risk | propose | 1 | 1 | — |
| tracing | 58 | Low Risk | propose | 6 | 3 | template ✅ |
| tracing-subscriber | 52 | Medium Risk | propose | 0 | 6 | template ✅ |
| ureq (optional) | 58 | Low Risk | propose | 0 | 3 | template ✅ |

## Notes

- **Replacement templates** (`src/replacement/template_data/`) exist for 20
  crates; the four Amber itself uses are marked ✅. Each template is validated
  with `cargo check` and covered by unit tests in `src/replacement/templates.rs`.
- **Kept by design:** `syn`/`proc-macro2` (AST parsing is the core value) and
  `serde` (classified security-sensitive; also foundational to the data model).
- **`toml_edit`** backs the experimental `migrate` feature; it may graduate or
  be removed with that feature.
- **Policy:** the self-analysis CI gate fails any PR that increases the direct
  dependency count. New dependencies require justification in the PR
  description (see `CONTRIBUTING.md`).
- Scores are from the offline provider (neutral maintenance metadata); run
  with `--features online` and `--online` for live crates.io data.
