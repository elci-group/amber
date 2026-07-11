# Replacement Library

The optional `library` Cargo feature stores generated replacement modules in a
[Padagonia](https://github.com/elci-group/padagonia) graph database. When
enabled, amber consults the library before generating a new in-house
replacement and stores newly-generated modules for reuse.

## Build

```bash
cargo build --features library
```

## Configuration

Add a `[library]` section to `.amber.toml`:

```toml
[library]
enabled = true
path = "~/.amber/library.pad"
```

`path` defaults to `~/.amber/library.pad`. The `--library` and `--no-library`
flags override the config value for a single run.

## CLI

```bash
# Generate a replacement and store it in the library
amber --library replace anyhow --out-dir amber_out

# List stored modules
amber library list

# Import an external module
amber library import ./amber_custom.rs --crate-name custom

# Export a stored module
amber library export anyhow --out amber_anyhow.rs

# Show the resolved library path
amber library path
```

## Padagonia schema

Each module is stored as a node labeled `ReplacementModule` with these
properties:

| Property      | Type      | Meaning                              |
|---------------|-----------|--------------------------------------|
| `crate_name`  | `String`  | Crate that this module replaces      |
| `module_name` | `String`  | Name of the generated module         |
| `code`        | `String`  | Source code of the module            |
| `source`      | `String`  | `generated`, `imported`, or `forked` |
| `created_at`  | `Timestamp` | Unix timestamp of creation         |

## Reuse and forking

- When `--library` is active and a module for the target crate already exists,
  amber loads it instead of regenerating. The proposal prints `(library)` and
  the validation result shows `loaded from library`.
- To create a variant of an existing module, export it, edit it, and import it
  back under the same crate name. New entries are appended; `find` returns the
  most recent.
- Programmatic forking is available via `LibraryStore::fork`, which clones an
  existing entry with `source = forked` and new code.

## Notes

- The feature is gated so default builds do not pull in Padagonia.
- Padagonia requires Rust 1.85 or newer.
- Library file corruption is handled gracefully: load failures fall back to an
  empty store.
