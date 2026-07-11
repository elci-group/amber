# Migration Guide

This guide explains how to migrate between major and minor versions of Amber.

## Migrating from v0.1.x to v0.2.x

### CLI changes

v0.2.x introduces subcommands. The old positional-only invocation is no longer
supported.

| v0.1.x command | v0.2.x equivalent |
|----------------|-------------------|
| `amber /path/to/project` | `amber /path/to/project analyze` |
| `amber /path/to/project --list` | `amber /path/to/project list` |
| `amber /path/to/project --crate foo` | `amber /path/to/project score foo` |

New output-format flags are available:

- `--format json` — machine-readable JSON report.
- `--format sarif` — SARIF v2.1.0 for static-analysis integrations.
- `--format pr` — Markdown report suitable for pull-request bodies.
- `--format emoji` — friendlier terminal output.

### Config format changes

v0.2.x adds `.amber.toml` for persistent configuration.

```toml
# v0.2.x example configuration
threshold = 60

[weights]
usage_simplicity = 0.2
transitive_value = 0.2
security_safety = 0.2
maintenance_burden = 0.15
testability = 0.15
api_surface = 0.1

[policy]
required = []
forbidden = ["unwanted_crate"]
strict = false
```

In v0.1.x these values were either absent or controlled only through CLI flags.
Move any flags you previously repeated into `.amber.toml`.

### Report format changes

The default v0.1.x plain-text report has been replaced by the structured
console report. If you parsed the old output, switch to one of the new
machine-readable formats:

- **JSON** is the best replacement for custom scripts.
- **SARIF** is recommended for GitHub Code Scanning and similar platforms.

The JSON schema is documented by example in the output of
`amber /path/to/project analyze --format json`.

### RustSec database caching

v0.2.x fetches the RustSec advisory database on first run and caches it in the
platform cache directory (see `docs/OPERATOR_RUNBOOK.md`). If you previously ran
Amber in an offline-only environment, ensure the CI cache covers the directory
listed in the runbook.

### Replacement workflow changes

The `replace` subcommand now generates a module under `amber_proposals/` and
runs `cargo check` against it. If you previously removed dependencies manually,
review the generated proposal before committing.

### Exit-code behavior

v0.2.x formalizes exit codes:

- `0` — no actionable findings.
- `1` — actionable findings or non-strict policy violation.
- `2` — strict policy violation.

Update CI pipelines that treated any non-zero exit as a hard failure.

## General migration checklist

1. Read the [CHANGELOG](CHANGELOG.md) for the target version.
2. Update `.amber.toml` if new configuration fields were added.
3. Run `amber --format json` and compare outputs against your existing
   automation.
4. Validate the RustSec database cache path in CI.
5. Run the full test suite after applying any replacement proposals.
