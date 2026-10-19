# 2. Isolate SQLite Persistence Into Domain-Specific Databases

Date: 2026-10-19

## Status

Accepted

## Context

`akashic` manages three distinct categories of state with vastly different concurrency patterns, write frequencies, and durability lifecycles:
1. Canonical human- and agent-authored Markdown documents in Git (Vault).
2. High-churn AST code symbols, graph community structures, and vector embeddings generated from Git commits (Code Graph).
3. Durable multi-stage RCA investigation checkpoints, symbol cache mappings, and streaming event watermarks (RCA & Daemon).

Storing all data in a single SQLite database would cause severe WAL write-lock contention between heavy background AST re-indexing and real-time RCA investigation step transitions. Furthermore, derived cache corruption would risk destroying durable RCA investigation audit trails.

## Decision

Isolate SQLite persistence into three domain-specific database files located by default under `.akashic/` (configurable in `akashic.toml`):

1. `.akashic/code_graph.db` (owned exclusively by `akashic-engine`):
   Stores `CodeSymbol` nodes, `CodeEdge` relationships, `GraphCommunity` cluster assignments, FTS5 BM25 search tables, and `sqlite-vec` code symbol vector embeddings.
2. `.akashic/rca_state.db` (owned by `akashic-rca` and `akashic-daemon`):
   Stores durable Local-Host Conductor investigation checkpoints, `CrashSignature` soak deduplication state, Symbol Store LRU cache index metadata, and Gerrit SSH stream event watermarks.
3. `.akashic/vault_index.db` (owned by `akashic-vault`):
   Acts as a disposable, reconstructible SQLite index and cache of parsed Vault Node frontmatter, forward Relations, FTS5 text indexes, and Vault Node embeddings. Canonical Markdown files tracked in Git remain the single source of truth at all times.

## Consequences

### Positive
- Zero SQLite WAL write-lock contention between background `akashic-engine` indexing and real-time `akashic-rca` workflow execution.
- Clear operational boundaries: `.akashic/code_graph.db` and `.akashic/vault_index.db` can be safely deleted and reconstructed from Git at any time without risking loss of durable RCA investigation records in `.akashic/rca_state.db`.
- Each database can configure SQLite PRAGMAs independently (e.g. aggressive cache sizes and memory mappings for `code_graph.db` versus high-durability fsync guarantees for `rca_state.db`).

### Negative
- Cross-domain queries between Code Graph symbols and Vault nodes cannot perform single-file SQL joins directly; joins must be coordinated in Rust memory or via read-only SQLite `ATTACH DATABASE`.
