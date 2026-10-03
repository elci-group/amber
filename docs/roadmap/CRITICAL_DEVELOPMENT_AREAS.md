# Five Critical Development Areas for Amber

> Derived from direct source-code analysis of `src/` at commit `7bbafba`.
> Each area cites specific files and line numbers where the gap exists today.

---

## 1. Replace Hardcoded Templates with a Scalable Generation System

**Where the gap lives:** `src/replacement/templates.rs` (lines 34–65)

`generate_code()` is a giant `match` statement over crate names. For anything not on the list, it returns:

```rust
_ => TemplateOutcome::Unsupported {
    reason: format!("no dedicated replacement template for `{}`", self.crate_name),
}
```

`src/replacement/template_data/` contains hand-written `.rs` files per crate. This does not scale to Amber's stated goal of covering the long tail of dependencies.

**What needs to change:**
- Build an inference pipeline that auto-generates replacement stubs from actual usage patterns.
- The `UsageDriven` variant already exists but is only wired for one crate; generalise it.
- Move from a name-based whitelist to a capability-based system that asks "what traits/functions does this project actually call?"

---

## 2. Deepen Usage Analysis Beyond Surface-Level AST Walking

**Where the gap lives:** `src/analysis/usage.rs`

The current visitor records `ItemUse` and `ExprMethodCall`, but misses:
- Trait bounds (`impl SomeTrait for T` where `SomeTrait` is external)
- Derive macros (`#[derive(serde::Serialize)]` are invisible to the current pass)
- `build.rs` and proc-macro hooks (not scanned at all)

This means Amber under-counts API surface, leading to overconfident replacement scores.

**What needs to change:**
- Integrate deeper semantic analysis (e.g. `rust-analyzer`'s `hir` or macro expansion via `cargo expand`).
- Every token attributable to a third-party crate must be captured with full context.
- Add a `build.rs` visitor that detects `Cargo.toml` target-table hooks.

---

## 3. Evolve Scoring from Static Heuristics to a Data-Driven Model

**Where the gap lives:** `src/scoring/rules.rs` (lines 176–233) and `src/scoring/classifier.rs` (lines 187–196)

`categorize_crate()` uses substring matching:

```rust
if name.contains("crypto") || name.contains("aes") || name.contains("sha") {
    return "cryptography";
}
```

`category_risk_penalty()` checks membership in static arrays (`NEVER_REPLACE`, `FREQUENTLY_REPLACEABLE`, `HEAVY_TRANSITIVE_CRATES`).

API surface coverage is bucketed into crude thresholds:

```rust
let api_surface = if usage.api_coverage_percent < 10.0 { 90u8 }
    else if usage.api_coverage_percent < 30.0 { 70u8 }
    else if usage.api_coverage_percent < 60.0 { 40u8 }
    else { 10u8 };
```

There is **no feedback loop**: Amber never learns whether a past replacement actually succeeded in production.

**What needs to change:**
- Replace static tables with a self-improving model that ingests real replacement outcomes.
- Track: compile success, test pass, performance regression, human review verdict.
- Adjust confidence intervals per crate based on observed history.

---

## 4. Validate Replacements in the Context of the Actual Target Project

**Where the gap lives:** `src/replacement/validator.rs` (lines 98–148)

The validator creates a **blank temporary Cargo project**, writes the replacement module, and runs `cargo check`. It ignores:
- The target project's dependency graph
- Feature flags
- Edition settings
- Existing type constraints

A module that compiles standalone can still fail when dropped into a real project.

**What needs to change:**
- Validate by cloning the target project, swapping the dependency, and running `cargo check --all-features`.
- Cache results keyed by `(crate_version, replacement_version, feature_set)`.
- Add semantic diffs of public API signatures using `cargo-public-api`.

---

## 5. Harden the End-to-End Migration Pipeline

**Where the gap lives:** `src/commands/migrate.rs` (behind `migrate` / `toml_edit` feature)

The `migrate` subcommand exists in the CLI tree but is thinly implemented. It is supposed to:
- Rewrite `Cargo.toml` to remove the replaced dependency
- Add the replacement module to the source tree
- Rewrite imports (`use anyhow::...` → `use amber_anyhow::...`)
- Run `cargo check` and roll back on failure

In practice, path rewriting is brittle, feature-flag propagation is unhandled, and rollback logic is shallow. The command is gated behind an optional feature because it is not yet production-safe.

**What needs to change:**
- Implement robust AST-based import rewriting (not regex) using `syn`.
- Add transactional rollback: snapshot `Cargo.toml` and `src/` before mutation; restore on any `cargo check` failure.
- Propagate feature flags from the original dependency to the replacement module where applicable.
- Expand integration fixture tests to cover multi-crate workspaces, path dependencies, and edition-gated syntax.
- Only remove the `migrate` feature gate once rollback has been battle-tested on real projects.

---

## Suggested Priority Order

1. **Area 2** (deeper analysis) unlocks everything else — without accurate usage data, scores and templates are guesses.
2. **Area 3** (data-driven scoring) builds on Area 2 and makes user-facing output credible.
3. **Area 1** (template generation) becomes feasible once Areas 2 and 3 provide the signal to auto-generate stubs.
4. **Area 4** (contextual validation) closes the gap between "compiles in a vacuum" and "works in production".
5. **Area 5** (migration pipeline) is the user-visible payoff — keep it behind the feature gate until Areas 1–4 are solid.
