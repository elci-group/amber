# Amber Architecture

Amber is a library-first tool. The `amber` binary is a thin CLI wrapper around
the library in `src/lib.rs`.

## Crate layout

```text
src/
├── lib.rs              # Public module re-exports
├── main.rs             # CLI entry point
├── analysis/           # Understanding the project
│   ├── repo.rs         # Cargo metadata → Dependency list
│   ├── usage.rs        # AST visitor → CrateUsage
│   └── types.rs        # Shared data types
├── metadata/           # Enriching dependencies with external data
│   ├── mod.rs          # MetadataProvider trait
│   ├── offline.rs      # Static, network-free metadata
│   ├── online.rs       # crates.io integration (feature `online`)
│   └── rustsec.rs      # RustSec advisory DB integration
├── scoring/            # Replaceability decisions
│   ├── classifier.rs   # SafetyClassifier and ReplacementScore
│   └── rules.rs        # Category lists and dimension helpers
├── replacement/        # Code generation
│   ├── generator.rs    # Build ReplacementProposal objects
│   ├── templates.rs    # Crate-specific replacement stubs
│   └── validator.rs    # cargo check validation of generated code
├── reporting/          # Output formats
│   └── formatters.rs   # Console, JSON, PR, SARIF reporters
└── config/             # User configuration
    └── mod.rs          # .amber.toml parsing and policy checks
```

## Data flow

```text
Cargo.toml ──► RepositoryAnalyzer ──► Vec<Dependency>
                                         │
                                         ▼
                            UsageAnalyzer ──► HashMap<String, CrateUsage>
                                         │
                                         ▼
                         SafetyClassifier ──► Vec<ReplacementScore>
                                         │
                    ┌────────────────────┼────────────────────┐
                    ▼                    ▼                    ▼
              ConsoleReporter      JsonReporter/PrReporter   Generator
```

1. **Repository analysis** reads `Cargo.toml` via `cargo_metadata`, resolves
   direct and transitive dependencies, and fetches metadata.
2. **Usage analysis** walks the project's `src/` with `syn`, recording imports,
   function calls, method calls, macro invocations, type references, and
   attributes.
3. **Scoring** combines usage data and metadata into a multi-dimensional score.
   Security-sensitive crates and crates with known advisories are blocked.
4. **Reporting** prints human-readable tables, machine-readable JSON/SARIF, or
   PR descriptions.
5. **Replacement generation** writes `amber_<crate>.rs` modules and validates
   them with `cargo check`.

## Key design decisions

- **Library-first**: All core logic lives in `src/lib.rs` so it can be tested
  and embedded in other tools.
- **Offline by default**: Amber does not require network access. The `online`
  feature enables crates.io downloads.
- **Security-aware**: RustSec advisory data enriches CVE counts. Any crate
  with a known advisory is classified as `SecurityCritical`.
- **Heuristic attribution**: Method calls are attributed to crates when the
  receiver path or method name unambiguously maps to a tracked dependency.
- **Confidence scoring**: Each score carries a separate confidence value based
  on usage visibility, independent of the overall replaceability score.

## Adding a new replacement template

1. Add a match arm in `ReplacementTemplate::generate_code`.
2. Implement a private `replace_<crate>` method returning a `String`.
3. Ensure the generated code compiles standalone (it is checked by
   `Validator::validate`).
4. Add an integration test that runs `amber replace <crate>` against a fixture.
