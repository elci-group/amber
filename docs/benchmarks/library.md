# Padagonia Library Integration Benchmarks

Measured with Criterion on the `library` feature:

```bash
cargo bench --features library --bench amber_library
```

Environment: Rust 1.96, release build, plotters backend. Each measurement uses
10 samples over a 1s window unless noted.

## Library operations

| Benchmark | Median time | Notes |
|-----------|-------------|-------|
| `library_open_empty` | ~5.9 µs | Create an empty store on disk |
| `library_insert_single` | ~81 µs | Insert one entry and persist |
| `library_find/hit/10` | ~1.6 µs | Lookup in 10 entries |
| `library_find/hit/100` | ~13 µs | Lookup in 100 entries |
| `library_find/hit/1000` | ~459 µs | Lookup in 1000 entries |
| `library_find/miss/1000` | ~280 µs | Lookup miss in 1000 entries |
| `library_list/100` | ~20 µs | List 100 entries |
| `library_list/1000` | ~406 µs | List 1000 entries |
| `library_save_load_roundtrip/10` | ~570 µs | Save then reload 10 entries |
| `library_save_load_roundtrip/100` | ~1.64 ms | Save then reload 100 entries |
| `library_save_load_roundtrip/1000` | ~9.34 ms | Save then reload 1000 entries |

Lookup (`find`) and `list` scale roughly linearly with the number of stored
entries because the current implementation iterates all `ReplacementModule`
nodes. For typical library sizes (tens of modules), lookups stay in the
single-digit microsecond range.

## Generator integration

| Benchmark | Median time | Notes |
|-----------|-------------|-------|
| `generator_without_library` | ~187 ms | Generate + `cargo check` validation |
| `generator_with_library_miss` | ~442 ms | Generate, validate, and store |
| `generator_with_library_hit` | ~3.7 µs | Load from library, no validation |

A library hit is roughly **50,000× faster** than a full generation because it
skips template rendering and `cargo check` validation entirely. A library miss
is slower than the no-library path because it also writes the module to the
library, but the cost is paid once per crate.

## Takeaways

- The library pays off when the same crate is replaced more than once; the
  first generation amortizes the store cost across subsequent hits.
- `find` is O(n); if libraries grow beyond a few thousand modules, an index
  keyed by `crate_name` would restore constant-time lookups.
- Validation (`cargo check`) dominates generation cost and is the primary
  reason library hits are so much cheaper.
