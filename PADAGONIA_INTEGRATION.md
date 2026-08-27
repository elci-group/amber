# PADAGONIA Integration Roadmap

Follow `/home/sal/padagonia/docs/enterprise-integration-directives.md`.

## Modules

- `dependency_event_adapter`: map crates, versions, imports, licenses, and
  advisory observations.
- `finding_writer`: persist replaceable dependency findings and evidence.
- `proposal_reader`: correlate proposed replacements with later build,
  benchmark, and security outcomes.
- `remediation_lineage`: record accepted, rejected, and reverted changes.

## Acceptance gates

Dependency scans are reproducible, proposals are never auto-applied from graph
presence, and all remediation decisions have commit provenance.
