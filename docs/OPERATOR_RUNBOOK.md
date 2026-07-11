# Amber Operator Runbook

This guide is for the engineer responsible for running Amber in CI pipelines,
consuming its outputs, and maintaining the advisory database cache.

## Running Amber in CI

Amber is a self-contained Rust binary. The recommended CI integration is:

```yaml
- name: Run Amber analysis
  run: |
    amber --threshold 60 --format sarif --output amber-results.sarif .
```

For the default console report, omit `--format sarif`. For JSON consumption use
`--format json`.

### Exit codes

Amber returns the following exit codes:

| Exit code | Meaning | Operator action |
|-----------|---------|-----------------|
| `0` | No actionable findings and no policy violations. | None required. |
| `1` | Actionable dependencies were found above the threshold, or a non-strict policy was violated. | Review the report and decide whether to remove/replace the flagged crates. |
| `2` | A strict policy violation was detected (only when `policy.strict = true` in `.amber.toml`). | Address the policy violation before merging. |

Treat exit code `1` as a review signal, not a build failure, unless your team
has decided to block merges on Amber findings.

## SARIF output consumption

Amber can emit a SARIF v2.1.0 report with two rule types:

- `unused-dependency` — declared in `Cargo.toml` but has no imports or call
  sites in source code.
- `replaceable-dependency` — scored high enough that Amber recommends
  replacement or removal.

The SARIF `tool.driver.rules` array contains rule metadata including
`shortDescription`, `fullDescription`, `defaultConfiguration`, and `help`.

### Example: upload to GitHub Code Scanning

```yaml
- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: amber-results.sarif
```

### Example: fail the job only on unused dependencies

```yaml
- name: Check for unused dependencies
  run: |
    if grep -q '"ruleId": "unused-dependency"' amber-results.sarif; then
      echo "Unused dependencies detected"
      exit 1
    fi
```

## Caching the RustSec advisory database

Amber enriches results with CVE counts from the RustSec advisory database. The
database is cached in the standard platform cache directory:

- Linux: `~/.cache/amber/rustsec-advisory-db`
- macOS: `~/Library/Caches/amber/rustsec-advisory-db`
- Windows: `%LOCALAPPDATA%\amber\rustsec-advisory-db`

If the standard cache cannot be resolved, Amber falls back to
`.amber/rustsec-advisory-db` in the working directory.

### CI cache recommendation

Cache the advisory database between runs to avoid cloning it on every build:

```yaml
- name: Cache RustSec DB
  uses: actions/cache@v4
  with:
    path: |
      ~/.cache/amber/rustsec-advisory-db
      .amber/rustsec-advisory-db
    key: amber-rustsec-db-${{ runner.os }}-${{ github.run_id }}
    restore-keys: amber-rustsec-db-${{ runner.os }}-
```

Amber refreshes stale entries automatically, so the cache only needs to be
valid enough to avoid a full clone each time.

## Handling false positives

Amber uses static heuristics to detect usage. It can be wrong when:

- A crate is only used through generated code, build scripts, or macro-expanded
  code that is not visible to the syntax tree.
- A crate is used indirectly through another dependency but does not appear in
  source files.
- A crate is used via dynamic dispatch or string-based APIs that Amber cannot
  trace.

### Mitigations

1. **Verify before removing.** Run `cargo check` and the full test suite after
   any removal.
2. **Raise the threshold.** Higher thresholds reduce low-confidence
   replacement suggestions.
3. **Use the policy file.** Add crates that should never be flagged to
   `.amber.toml`:

   ```toml
   [policy]
   forbidden = []
   required = ["sensitive_crate"]
   ```

4. **File an issue** if the same false positive repeats across projects.

## Rolling back replacements

If a replacement proposal is applied and causes a regression:

1. Revert the commit that removed the dependency and added the replacement
   module.
2. Restore the original `Cargo.toml` entry if it was removed.
3. Delete any generated modules under `amber_proposals/` or the configured
   output directory.
4. Re-run `cargo test` to confirm the regression is resolved.

For high-risk crates, generate the proposal with `--threshold 0 --propose` and
review the generated code before committing any changes.
