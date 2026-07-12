# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.3.x   | ✅        |
| < 0.3   | ❌        |

Only the latest minor release receives security fixes. Amber follows semantic
versioning; see [`VERSION`](VERSION) for the current release.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Report privately via GitHub Security Advisories at
<https://github.com/elci-group/amber/security/advisories/new>,
or email **svch@seriousaboutsolutions.co.uk** with:

- a description of the vulnerability and its impact,
- steps to reproduce or a proof of concept,
- affected versions, if known.

You will receive an acknowledgement within **5 business days** and a triage
decision within **15 business days**. Fixes are released as patch versions and
credited in the changelog unless you prefer to remain anonymous.

## Scope and security model

Amber is a local developer tool that:

- reads the target project's `Cargo.toml` and source files,
- shells out to `cargo` (metadata, check) and `git` (RustSec advisory DB),
- writes generated replacement modules to a user-specified output directory,
- optionally fetches crates.io metadata (the `online` feature, off by default),
- optionally clones the RustSec advisory database into the user's platform
  cache directory (`XDG_CACHE_HOME` or equivalent).

Amber never executes project code and performs no network access by default.
Output paths are validated to stay within the target project.

In scope for reports: path-traversal bypasses in output validation, unsafe
handling of untrusted `Cargo.toml`/`.amber.toml` input, supply-chain issues in
release artifacts, and credential leakage. Out of scope: issues requiring an
already-compromised toolchain or `cargo`/`git` binaries.

## Verifying releases

Release artifacts ship with SHA-256 checksums, a GPG signature, a CycloneDX
SBOM, and a GitHub build-provenance attestation. Verification steps are in
[`docs/OPERATOR_RUNBOOK.md`](docs/OPERATOR_RUNBOOK.md).
