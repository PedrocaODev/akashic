# 6. Non-Destructive Git Ingestion and Structural Significance Filtering

Date: 2026-10-19

## Status

Accepted

## Context

Indexing code repositories often involves switching branches or checking out worktrees, which disrupts active developer workspaces, triggers expensive build invalidations, and creates index lock collisions. In addition, large software projects produce thousands of Git commits that modify only local variable names, logging statements, or internal method implementations without altering architectural boundaries, type hierarchies, or public API signatures. Treating all commits identically leads to high index churn and spurious Vault drift notifications.

## Decision

Adopt non-destructive Git ingestion and structural significance filtering in `akashic-engine`:

1. **Non-Destructive Ingestion (`gix`)**:
   Use `gix` (gitoxide) to traverse Git commit trees and read file blobs directly from the bare `.git` object database into in-memory buffers. The engine never executes `git checkout`, never creates temporary worktrees, and never touches developer working tree state or index files.

2. **Structural Significance Filter**:
   Before updating `code_graph.db` or flagging potential architectural drift in the Vault, parse incoming commit diffs with Tree-sitter AST filters to classify changes into two tiers:
   - **Structurally Significant Changes**: Additions, deletions, or signature modifications to classes, interfaces, traits, public/protected methods, function signatures, inheritance hierarchies, and package exports. These trigger Code Graph reconciliation and potential Vault drift checks.
   - **Internal / Non-Significant Changes**: Purely internal method-body statement edits, comment revisions, whitespace formatting, or private variable renames. These are recorded with minimal symbol-body hash updates without triggering full community recalculation or Vault supersession alerts.

## Consequences

### Positive
- Developers and autonomous agents can run continuous background indexing and graph reconciliation concurrently with active coding without index locks or build interference.
- Drastic reduction in background CPU usage and database write churn during continuous Git ingestion.
- Eliminates noise: engineers and agents are only alerted when real architectural interfaces change.

### Negative
- AST diffing requires parsing before-and-after versions of modified files using Tree-sitter, introducing a small upfront CPU cost per commit.
