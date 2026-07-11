# Amber v0.3 Roadmap: Self-Hosting Dependency Reduction

The S-grade quality milestone is complete. The next strategic goal for Amber
is to **eat its own dog food**: reduce Amber's own third-party dependency tree
by generating, validating, and adopting in-house replacements.

## Theme

> Make Amber its own best-case study for dependency reduction.

## Goals

| Goal | Metric |
|------|--------|
| Self-analysis in CI | Every PR runs `amber` on itself and fails on regressions |
| Validated replacements for all direct deps | Every direct dependency has a passing replacement proposal |
| Adopt std-only replacements where safe | Remove at least 4 more direct dependencies |
| Migration automation | `amber migrate` applies a replacement to the project's source |
| Dependency reduction target | ≥50% reduction in direct dependency count |

## Workstreams

### 1. Replacement template completeness

Amber currently ships validated templates for common utility crates. v0.3
extends coverage to the crates Amber itself depends on:

- `tracing` / `tracing-subscriber` — ✅ initial templates added in this cycle
- `toml` — ✅ initial template added in this cycle
- `ureq` — ✅ initial template added in this cycle
- `serde` / `serde_json` — design minimal derive-less serialization helpers
- `semver` — version requirement parsing and comparison
- `cargo_metadata` — thin wrapper around `cargo metadata --format-version 1`
- `syn` / `proc-macro2` — keep; these are too large to replace safely
- `clap` — evaluate lightweight arg parsing fallback
- `rustsec` — evaluate optional advisory enrichment vs. offline CVE lists

Each new template must:

1. Pass `Validator::validate` (i.e. `cargo check` clean).
2. Have a unit test in `src/replacement/templates.rs`.
3. Be exercised by `amber replace <crate>` in CI or an integration test.

### 2. `amber migrate` subcommand

Add a `migrate` subcommand that goes beyond proposal generation:

```bash
amber migrate anyhow --replace-with amber_out/amber_anyhow.rs
```

Responsibilities:

- Rewrite `Cargo.toml` to remove the replaced dependency.
- Add the replacement module to the target project's source tree.
- Rewrite `use anyhow::...` imports to `use amber_anyhow::...`.
- Run `cargo check` and report the result.
- Roll back on failure.

This is a high-impact, high-risk feature and should be implemented behind a
`migrate` feature flag with extensive fixture tests.

### 3. Self-analysis CI job

Add a GitHub Actions job that runs:

```bash
cargo run -- analyze --threshold 60 --propose
```

and fails if:

- The number of direct dependencies increases relative to `main`.
- Any existing validated replacement starts failing `cargo check`.
- A previously removed dependency reappears.

### 4. Library-backed proposal reuse

Leverage the Padagonia-backed replacement library so that once a replacement
for a crate is validated, it is reused across projects instead of regenerated.

Deliverables:

- Seed the default library with the validated std-only replacements.
- Add `amber library publish` to share replacements between teams.
- Document library workflows in `docs/library.md`.

### 5. Dependency reduction scorecard

Maintain a `DEPENDENCIES.md` scorecard that lists:

- Current direct dependencies
- Replaceability score
- Replacement status
- Owner workstream

Update it at the end of every release cycle.

## Definition of Done

- `amber migrate` exists, is feature-gated, and has integration tests.
- CI runs self-analysis on every PR.
- At least 4 additional direct dependencies are removed from `Cargo.toml`.
- A v0.3.0 changelog entry is written.
- Line coverage remains ≥95% and clippy remains clean.

## Out of Scope

- Replacing `syn` / `proc-macro2` (AST parsing is core value).
- Replacing `clap` unless a fully compatible derive-less parser is validated.
- Network-free crates.io metadata (already supported by the `online` feature).
