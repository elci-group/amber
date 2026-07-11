# amber(1) — editable source

This file is the editable source for the roff manual at [`amber.1`](amber.1).
Keep the two in sync: edit here first, then reflect the same options and
examples in `amber.1`. The authoritative list of flags is `amber --help`.

If you have `pandoc`, you can regenerate a starting point for the roff with:

```bash
pandoc -s -t man amber.1.md -o amber.1
```

Review the output by hand afterwards — hand-tuned roff in `amber.1` is the
canonical file shipped to users.

---

## NAME

**amber** — dependency garbage collector for Rust

## SYNOPSIS

**amber** \[*OPTIONS*] \[*PATH*] \[*COMMAND*]

## DESCRIPTION

**amber** analyzes a Cargo project, detects how each third-party crate is used,
scores every dependency for replaceability, and generates validated, drop-in
replacement modules for common utility crates.

Generated replacements are emitted as `amber_<crate>_redux` modules and checked
with `cargo check` before they are reported. Crate names containing hyphens are
normalised to underscores in the module name.

## OPTIONS

- `-f`, `--format <FORMAT>` — `console` (default), `json`, `pr`, `sarif`, `emoji`.
- `--emoji` — shorthand for `--format emoji`.
- `-t`, `--threshold <N>` — minimum replaceability score 0–100 (default 50).
- `-T`, `--transitive` — include transitive dependencies.
- `-p`, `--propose` — generate validated replacements for crates ≥ threshold.
- `--no-dev` — exclude dev-dependencies.
- `-c`, `--crates <CRATES>` — comma-separated or repeated crate filter.
- `--config <PATH>` — config file (default `.amber.toml`).
- `-v`, `--verbose…` — increase verbosity (`-v` debug, `-vv` trace).
- `-h`, `--help` — print help; `-V`, `--version` — print version.

Feature-gated: `--library` / `--no-library` (`library` feature), `--online`
(`online` feature).

## COMMANDS

- `analyze [-o file]` — full pipeline; report in the selected `--format`.
- `score <crate>` — score one crate and print the breakdown.
- `list` — list dependencies with usage statistics.
- `replace <crate> [-o dir]` — generate a validated `amber_<crate>_redux` module.
- `roadmap` — show Amber's internal module roadmap.
- `directives <crate> [-o file]` — emit a scoped replacement directive.
- `library` — (`library` feature) manage the Padagonia replacement library.

## EXIT STATUS

- `0` — success, no policy violations.
- `1` — analysis found policy violations (strict mode).
- `2` — a command, parse, or I/O error occurred.

## ENVIRONMENT

- `RUST_LOG` — controls tracing output (demos set `RUST_LOG=error`).

## EXAMPLES

```bash
amber
amber --format json /path/to/project
amber --propose --threshold 70
amber score anyhow
amber replace serde --out-dir out
amber directives colored -o colored.md
```

## SEE ALSO

`cargo(1)`, reproducible demos in `docs/vhs/README.md`, documentation index in
`docs/README.md`.
