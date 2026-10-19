# 4. Strict Conflict Abstention and Plan-Then-Apply Vault Mutations

Date: 2026-10-19

## Status

Accepted

## Context

Automated knowledge graph engines often encounter topology anomalies, such as cyclic supersession edges (e.g., node A superseding node B which supersedes node A) or competing active successors claiming authority over the same Scope. Naive heuristics (such as picking the newest timestamp or random tie-breaking) silently corrupt graph semantics, surface outdated guidance, and degrade trust in automated agent decisions. Similarly, unverified, direct file writes by autonomous agents easily introduce syntax errors or broken relations into Vault Git repositories.

## Decision

Enforce strict mathematical conflict abstention and a two-phase mutation lifecycle across `akashic-vault` and `akashic-lint`:

1. **Strict Conflict Abstention**:
   When the engine detects an invalid graph topology (such as a supersession cycle or multiple active un-superseded successor nodes within identical or overlapping Scopes), it strictly abstains from choosing a winner. Instead, it marks the nodes as an `Unresolved Conflict`, surfaces an explicit warning/diagnostic, and excludes conflicting branches from `Current Guidance` queries until an engineer or explicit resolution plan resolves the ambiguity.

2. **Plan-Then-Apply Mutation Workflow**:
   Direct batch mutation of Markdown documents by CLI commands is prohibited. Vault migrations, metadata normalization, and inferred edge assertions must follow a two-step pattern:
   - `akashic vault plan`: Generates a machine-readable, deterministic JSON/YAML diff (`Plan`) listing prospective frontmatter updates and supporting evidence without touching files on disk.
   - `akashic vault apply`: Applies an inspected and approved `Plan` atomically, validating each file against `akashic-lint` rules before committing changes.

3. **Lint-Gated Crystallization**:
   Crystallizing investigation findings into new `Incident`, `Evidence`, or `Resolution` Vault Nodes requires pre-validation through `akashic-lint`. Nodes failing linting are rejected before reaching disk or Gerrit review branches.

## Consequences

### Positive
- Eradicates silent data corruption and heuristic guesswork in knowledge graph resolution.
- Engineering teams retain authoritative control over architectural and decision disputes through explicit conflict reporting.
- The two-phase `plan` / `apply` workflow provides reviewable change sets that can be audited before mutating files in Git.

### Negative
- Unresolved conflicts require explicit manual or planned intervention to restore complete guidance for the affected scope.
