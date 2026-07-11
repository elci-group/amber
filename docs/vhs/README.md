# Reproducible demo recordings (VHS)

This directory is the single source of truth for the animated demos shown in
the top-level `README.md` and on the website. Recordings are produced with
[`vhs`](https://github.com/charmbracelet/vhs) from declarative `.tape` files, so
they are deterministic, reviewable in diffs, and trivial to re-render.

## Layout

```text
docs/vhs/
├── fixture/            # generic sample-app Cargo project the demos analyze
│   ├── Cargo.toml
│   ├── .amber.toml
│   └── src/main.rs
├── tapes/              # one .tape per demo (help, analyze, score, propose, emoji-*)
├── Containerfile       # amber + cargo + vhs + ttyd + ffmpeg in one image
├── build.sh            # render everything (container preferred, host fallback)
└── out/                # rendered GIFs (gitignored build output)
```

## Render

```bash
# preferred: fully containerised, nothing host-specific on screen
docs/vhs/build.sh

# force a path
docs/vhs/build.sh --docker
docs/vhs/build.sh --host
```

The script writes every GIF to `docs/vhs/out/` and copies the two
`amber-emoji-*.gif` demos into `website/assets/vhs/` so the site keeps working.

## Privacy model

A recording must never expose the developer's filesystem, username, shell
setup, or personal project paths. The pipeline enforces that:

- **Container path (default).** The image runs every command under `/work`,
  with the fixture at `/work/fixture`. The only host mount is the output
  directory (`-v "$OUT:/work/out"`); the recording never sees a host path.
- **Host fallback.** When no container runtime is available, the same tapes run
  on the host with `export PS1='> '` and relative commands (`cd fixture`,
  `amber .`), so the prompt shows no `$HOME`, no username, and no absolute path.
- **Tape contents.** Tapes contain no `source <host-file>`, no `~/...` paths,
  and no absolute host locations. They set `RUST_LOG=error` to keep frames clean
  and use the built-in `Catppuccin Mocha` theme (no local font dependency).

If you review a PR that touches `tapes/`, reject anything that reintroduces a
host-specific path, a `source` of a file outside the repo, or a dependency on a
particular developer's environment.

## Add a demo

1. Copy an existing tape in `tapes/` and change its `Output` name and command.
2. Keep the standard header (`Set Shell bash`, neutral prompt, `RUST_LOG=error`,
   `cd fixture`, relative `Output out/<name>.gif`).
3. Re-render with `docs/vhs/build.sh` and reference the new GIF from the docs.

The fixture is intentionally small and uses only widely cached crates so that
`amber` (via `cargo metadata`) resolves fully offline inside the container.
