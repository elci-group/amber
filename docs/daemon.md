# Amber daemon

The optional `daemon` feature adds a lightweight monitor for local Cargo
projects. It is designed to be cheap when nothing changed: each scan discovers
Cargo manifests, fingerprints dependency tables, and only runs full Amber
analysis when the project is new or the dependency fingerprint changed.

## Build

```bash
cargo install --path . --features daemon
```

The daemon feature implies the `library` feature because analysis snapshots are
stored in a Padagonia-backed cache.

## Commands

Run one bounded scan:

```bash
amber daemon --once
```

Run continuously, polling every 30 seconds:

```bash
amber daemon
```

Monitor a specific tree:

```bash
amber daemon --root ~/work --interval-secs 60
```

Use a specific cache:

```bash
amber daemon --cache ~/.amber/analysis-cache.pad
```

## Defaults

| Setting | Default |
|---------|---------|
| Monitor root | `$HOME` |
| Poll interval | 30 seconds |
| Projects per scan | 32 |
| Cache path | `~/.amber/analysis-cache.pad` |
| Cache entries retained | 512 |
| Cache max age | 90 days |

The scanner skips common heavy directories including `.git`, `target`,
`node_modules`, `.cache`, `.cargo`, `.rustup`, `dist`, and `build`.

## Project markers

Each compatible project receives a lightweight marker at:

```text
.amber/daemon.toml
```

The marker records the Amber version, cache path, threshold, current dependency
fingerprint, and update timestamp. It is metadata only; it is not a policy file.

## Cache locking

The daemon creates an exclusive lock next to the Padagonia cache, for example:

```text
~/.amber/analysis-cache.pad.lock
```

A second daemon using the same cache exits with a lock error instead of writing
concurrently. The lock file is removed when the daemon exits normally. If a
process is killed abruptly, remove the stale lock only after confirming no Amber
daemon is still running.

## Cache retention

After every new analysis snapshot, the daemon compacts the cache:

- keeps only the newest snapshot per project;
- evicts entries whose manifest path no longer exists;
- evicts entries older than `--cache-max-age-days`;
- caps total retained snapshots at `--max-cache-entries`.

Tune retention for larger workstations:

```bash
amber daemon --max-cache-entries 2048 --cache-max-age-days 180
```

## Source-only changes

By default, source-only changes do not trigger full analysis when dependency
tables are unchanged. This keeps steady-state monitoring low-cost and lets the
daemon act as an upstream dependency-change trigger.

Use this when you want fresh usage data after source edits:

```bash
amber daemon --analyze-source-changes
```

## systemd user service

Example unit:

```ini
[Unit]
Description=Amber dependency analysis daemon

[Service]
ExecStart=%h/.cargo/bin/amber daemon --root %h --interval-secs 60
Restart=on-failure
RestartSec=10

[Install]
WantedBy=default.target
```

Save it as `~/.config/systemd/user/amber-daemon.service`, then run:

```bash
systemctl --user daemon-reload
systemctl --user enable --now amber-daemon.service
systemctl --user status amber-daemon.service
```

## Troubleshooting

- If `amber daemon` is missing, check `which amber`. A stale binary earlier in
  `PATH` may shadow a newer install in `~/.cargo/bin`.
- If the daemon reports a cache lock, check for another running daemon before
  removing the `.lock` file.
- If analysis is unexpectedly slow, lower `--max-projects`, increase
  `--interval-secs`, or monitor a narrower `--root`.
- If RustSec updates contend in parallel test runs, use serialized CI jobs or a
  pre-warmed advisory cache.
