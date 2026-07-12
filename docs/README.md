# Amber documentation

This directory holds the long-form documentation for Amber. The top-level
[`README.md`](../README.md) is the entry point; everything here goes deeper.

## Reference

- [`man/amber.1`](man/amber.1) — the `amber(1)` manual page. Install to a
  manpath (for example `/usr/local/share/man/man1/`) and run `man amber`. Its
  editable source is [`man/amber.1.md`](man/amber.1.md).
- [`library.md`](library.md) — the optional Padagonia replacement library:
  storing, importing, forking, and exporting generated modules.
- [`OPERATOR_RUNBOOK.md`](OPERATOR_RUNBOOK.md) — day-to-day operation, output
  formats, policy files, and troubleshooting.

## Replacements & directives

- [`directives/colored.md`](directives/colored.md)
- [`directives/tempfile.md`](directives/tempfile.md)
- [`directives/walkdir.md`](directives/walkdir.md)

Scoped, implementation-ready directives for replacing specific crates. Regenerate
with `amber directives <crate>`.

## Benchmarks

- [`benchmarks/library.md`](benchmarks/library.md) — benchmark methodology and
  results for generated replacements versus their original crates.

## Recordings

- [`vhs/README.md`](vhs/README.md) — how the README/website animations are
  produced reproducibly with [`vhs`](https://github.com/charmbracelet/vhs),
  including the container pipeline and its privacy model (no host paths, no
  usernames, no personal setup on screen).

## Roadmap

- [`roadmap/V0_3_SELF_HOSTING.md`](roadmap/V0_3_SELF_HOSTING.md) — the path to
  Amber analyzing its own dependency tree.
- [`roadmap/RELEASE_READINESS.md`](roadmap/RELEASE_READINESS.md) — the path from
  v0.3.0 to enterprise general availability (v1.0.0); see also the
  [enterprise assessment](../ENTERPRISE_ASSESSMENT.md).
