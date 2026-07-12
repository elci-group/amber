# Versioning Policy

Amber follows [Semantic Versioning 2.0.0](https://semver.org), enforced as
follows.

## What is covered

- **The CLI surface** — subcommands, flags, output formats (`console`, `json`,
  `pr`, `sarif`, `emoji`), exit codes, and the `.amber.toml` configuration
  schema.
- **The library API** — the public modules of the `amber` crate
  (`analysis`, `config`, `metadata`, `replacement`, `reporting`, `scoring`).

## Compatibility commitments

- **Patch (0.3.x):** bug fixes only. No CLI flag removals, no changes to JSON
  or SARIF field names, no breaking library API changes.
- **Minor (0.x.0):** new subcommands, flags, output fields, and library APIs.
  Additions to JSON reports are always new fields, never renames or type
  changes. While the major version is 0, minor releases may make breaking
  changes, but only with a `### Changed` entry in `CHANGELOG.md` describing
  the migration.
- **Major (x.0.0):** breaking changes of any kind, with a migration guide in
  `MIGRATION.md`.

## MSRV policy

- The minimum supported Rust version is declared as `rust-version` in
  `Cargo.toml` (currently **1.85**) and enforced by the `msrv` CI job.
- MSRV bumps are treated as **minor** changes, never patch changes, and are
  noted in the changelog.
- Optional features may require a newer toolchain; this is documented per
  feature. All current features build at the base MSRV.

## Deprecations

A CLI flag, output field, or library API is deprecated for at least one full
minor release before removal. Deprecations are announced in `CHANGELOG.md`
under `### Deprecated` with the replacement.

## Feature flags

- `online` and `library` are opt-in and carry no stability commitments beyond
  the CLI surface they expose.
- `migrate` is experimental: its behavior may change in any release until it
  graduates to the default feature set, which will be announced in the
  changelog.
