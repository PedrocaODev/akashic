# 3. Enforce OKF v0.2 Schema With Custom Fields and Forward-Only Relations

Date: 2026-10-19

## Status

Accepted

## Context

Managing architectural decisions, OEM Android bug investigations, and system knowledge requires a standardized schema that preserves Git immutability, supports rich causal graph structures, and accommodates project-specific domain metadata without data loss. Ad-hoc frontmatter schemas or bidirectional backlink mutation pollute Git history, create merge conflicts across branches, and lead to fragile graph integrity.

## Decision

Adopt the Google Open Knowledge Format (OKF) v0.2 specification as the baseline schema in `akashic-vault` and enforce it via `akashic-lint`, with the following core architectural rules:

1. **Whole-Note Granularity**:
   Every discrete concept, incident, resolution, decision, or architectural note is stored as a standalone Markdown document (`.md`) with YAML frontmatter. Fine-grained block-level IDs are avoided.

2. **Canonical Forward-Only Relations**:
   Graph edges are strictly declared in source document frontmatter pointing forward to targets using typed Relations: `supersedes`, `resolves`, `depends_on`, `supported_by`, `caused_by`, `relates_to`, and `contradicts`. Target nodes are never retroactively edited to add inverse backlinks, preserving Git history and node immutability.

3. **First-Class Incident and Resolution Types**:
   Define first-class node types extending OKF v0.2:
   - `type: Incident`: authoritatively captures failure events, symptoms, impacted build/device scopes, and evidence links.
   - `type: Resolution`: documents verified remedies classified by `Remediation Type` (`workaround`, `hotfix`, `permanent_fix`, or `revert`) with dedicated negative knowledge fields (`invalidated_hypotheses`, `failed_attempts`).

4. **Lossless Custom Field Preservation**:
   All unknown or project-specific YAML frontmatter attributes are preserved during serialization and deserialization via Rust Serde flattening:
   ```rust
   #[serde(flatten)]
   pub custom: BTreeMap<String, serde_yaml::Value>,
   ```

5. **Custom Field Validation**:
   `akashic-lint` validates standard OKF v0.2 fields (`L001`–`L011`) while dynamically enforcing optional custom field constraints and types declared in `akashic.toml`.

## Consequences

### Positive
- Strict Git immutability: adding an edge or recording a resolution modifies only the new or successor node.
- Lossless round-tripping guarantees external automation and project-specific frontmatter attributes are never dropped during crystallization or plan application.
- Negative knowledge prevents recurring engineering mistakes by documenting failed fixes and disproven hypotheses as first-class graph metadata.

### Negative
- Reverse relation lookups (e.g. finding all nodes superseded by a document) require index queries through `vault_index.db` rather than reading raw target node frontmatter.
