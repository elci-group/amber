# Amber

[![version](https://img.shields.io/badge/version-0.3.0-amber.svg)](Cargo.toml)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![msrv](https://img.shields.io/badge/MSRV-1.80-orange.svg)](Cargo.toml)

**Amber** is an autonomous dependency-reduction engine for Rust. It analyzes a
Cargo project, detects how each third-party crate is used, scores every
dependency for replaceability, and generates validated, drop-in replacement
modules for common utility crates.

<p align="center">
  <img src="docs/vhs/out/amber-analyze.gif" alt="amber analyzing a sample project and printing a scored dependency report" width="880" />
</p>

> The recording above is reproducible: it is rendered from a declarative
> [`vhs`](https://github.com/charmbracelet/vhs) tape against a generic fixture,
> with no host-specific paths. See [Reproduce the demos](#reproduce-the-demos).

## Why Amber

Rust projects quietly accumulate crates that are lightly used, trivially
replaced with the standard library, or vendored in a few lines. Amber finds them,
ranks them by how safely they can be removed, and hands you replacement code
that already compiles — so shrinking your dependency tree is a review, not a
research project.

## Features

- **Dependency inventory** — direct and transitive dependencies with source,
  kind, and metadata.
- **AST-based usage analysis** — imports, function and method calls, macro
  invocations, type usage, and trait bounds.
- **Replaceability scoring** — usage simplicity, transitive value, security
  safety, maintenance burden, testability, and API surface.
- **Validated replacements** — generated modules are checked with `cargo check`
  before they are reported; a proposal that does not compile never reaches you.
- **Multiple output formats** — console tables, JSON, PR-ready Markdown, SARIF,
  and a compact emoji summary.
- **Policy enforcement** — `.amber.toml` for required/forbidden crates and
  thresholds, with a strict mode for CI.
- **Optional live metadata** — crates.io integration via the `online` feature.
- **Optional replacement library** — a Padagonia-backed database of generated
  modules via the `library` feature.

## Installation

Requires Rust **1.80** or newer.

```bash
cargo install --path .
```

Build with optional features as needed:

```bash
cargo install --path . --features online    # live crates.io metadata
cargo install --path . --features library   # Padagonia replacement library
```

## Quick start

```bash
# Analyze the current project (console report)
amber

# Analyze a specific project and emit JSON
amber --format json path/to/project

# Generate validated replacement proposals for high-scoring crates
amber --propose --threshold 70

# Score a single crate
amber score anyhow

# Generate a validated replacement module
amber replace anyhow --out-dir amber_out
```

## Commands

| Command | Description |
| --- | --- |
| `amber [PATH]` | Analyze the project at `PATH` (default `.`) and print a report. |
| `amber analyze [-o FILE]` | Same as above, with an optional report output file. |
| `amber score <crate>` | Score one crate and print the per-dimension breakdown. |
| `amber list` | List all dependencies with usage statistics. |
| `amber replace <crate> [-o DIR]` | Generate a validated replacement module (default dir `amber_out`). |
| `amber directives <crate> [-o FILE]` | Emit a scoped, implementation-ready replacement directive. |
| `amber roadmap` | Show Amber's internal module roadmap. |
| `amber library …` | Manage the replacement library (`library` feature). |

Run `amber --help` for the full option list, examples, and exit codes, or read
the manual page in [`docs/man/amber.1`](docs/man/amber.1).

## Output formats

| Format | Use |
| --- | --- |
| `console` | Human-readable table (default). |
| `json` | Machine-readable report for tooling. |
| `pr` | Markdown suitable for a pull-request comment. |
| `sarif` | Static-analysis interchange format for CI ingestion. |
| `emoji` | Compact, friendly summary. |

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success; no policy violations. |
| `1` | Analysis found policy violations (strict mode). |
| `2` | A command, parse, or I/O error occurred. |

## Configuration

Amber reads `.amber.toml` from the target project root (or the path given to
`--config`):

```toml
threshold = 60

[policy]
required = ["serde"]
forbidden = ["deprecated_crate"]
strict = false
```

## Replacement modules

Generated replacements are written as **`amber_<crate>_redux`** modules (crate
names with hyphens are normalised to underscores). Each module is validated with
`cargo check` before it is reported, so the file you receive is a starting point
that already compiles against your project — review it, wire it in, and delete
the dependency.

> Note: `crate::amber_anyhow` is Amber's own internal error module, not a
> generated replacement; it is never renamed or emitted as a proposal.

## Reproduce the demos

Every animation in this README and on the website is rendered from the tapes in
[`docs/vhs/`](docs/vhs/README.md). The pipeline is designed so it never exposes
your filesystem, username, or shell setup: the container path runs everything
under `/work`, and the host fallback uses a neutral prompt and relative paths.

```bash
# preferred: fully containerised
docs/vhs/build.sh

# force the container, or force the host fallback
docs/vhs/build.sh --docker
docs/vhs/build.sh --host
```

GIFs are written to `docs/vhs/out/`; the two emoji demos are also copied to
`website/assets/vhs/`.

## Documentation

- [`docs/README.md`](docs/README.md) — documentation index.
- [`docs/man/amber.1`](docs/man/amber.1) — manual page (install to a manpath and
  run `man amber`).
- [`docs/vhs/README.md`](docs/vhs/README.md) — recording pipeline and privacy model.
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — internal design.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo audit
```

## License

MIT — see [LICENSE](LICENSE).
