# Contributing to Amber

Thanks for helping out. The workflow is intentionally plain.

## Getting started

```bash
git clone https://github.com/elci-group/amber
cd amber
cargo build
cargo test --all-targets
```

Requires Rust 1.80 or newer (MSRV; see `Cargo.toml`). The optional `library`
feature requires Rust 1.85+.

## Before opening a PR

All of these must pass — they are enforced in CI:

```bash
cargo fmt --check
cargo clippy --all-targets -- -W clippy::pedantic -W clippy::nursery -D warnings
cargo test --all-targets
cargo tarpaulin --all-targets --fail-under 95   # coverage floor
cargo audit
```

## Conventions

- **Tests with every change.** New features ship with unit tests and, where
  the CLI surface changes, integration coverage under `tests/`. Line coverage
  must stay at or above 95%.
- **No new dependencies without a reason.** Amber is a dependency-reduction
  tool; its own tree is held to the same standard. Run `cargo run -- .` on the
  repo and check the report before adding a crate. The self-analysis CI gate
  fails PRs that grow the direct dependency count.
- **Deterministic and offline by default.** Network access stays behind the
  `online` feature; tests must not require network or a specific host
  environment.
- **No `unwrap`/`expect` outside tests.** `#![deny(clippy::unwrap_used)]`
  applies to release code; return the project's `amber_anyhow` error with
  context instead.
- Keep edits scoped. A tidy, reviewable diff beats an opportunistic cleanup.

## Commit and PR style

- Conventional-ish prefixes (`fix:`, `feat:`, `docs:`, `ci:`, `release:`) as
  used in the existing history.
- User-facing changes get an entry under `## [Unreleased]` in `CHANGELOG.md`.
- Version bumps happen only in release commits (`release: vX.Y.Z`) that update
  `Cargo.toml`, `VERSION`, the README badge, `docs/man/amber.1`, and
  `website/data/releases.json` together.

## Reporting security issues

See [`SECURITY.md`](SECURITY.md) — please do not file public issues for
vulnerabilities.
