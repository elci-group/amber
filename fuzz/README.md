# Fuzz testing

Coverage-guided fuzz targets for Amber's input parsers, run with
[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) (requires nightly).

## Targets

- `config_parse` — arbitrary bytes interpreted as `.amber.toml`: TOML
  deserialization into `amber::config::Config`, `Config::validate`, and policy
  lookups. Guards against panics and pathological inputs in the one file
  format Amber accepts from untrusted projects.

## Running

```bash
cargo install cargo-fuzz --locked
cargo +nightly fuzz run config_parse            # run until interrupted
cargo +nightly fuzz run config_parse -- -max_total_time=60
```

CI runs every target for 60 seconds per pull request (`fuzz` job in
`.github/workflows/ci.yml`). Crash artifacts are written to `fuzz/artifacts/`
(gitignored); minimized reproducers belong in `fuzz/corpus/` when worth
keeping.
