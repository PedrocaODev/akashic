# 06 — Multi-Language AST Code Graph, Gix, and Hybrid Search Slice

**What to build:** Implement the core Code Graph extraction and storage engine in `akashic-engine`. Read commit trees directly from bare Git object databases via `gix` without mutating developer working directories, parse multi-language source trees using Tree-sitter (Java, Kotlin, C++, Rust, Python, Go, TypeScript), classify edges by `Edge Provenance` (`EXTRACTED`, `INFERRED`, `AMBIGUOUS`), apply the `Structural Significance Filter` on commit diffs, and store symbols, edges, FTS5 indexes, and `sqlite-vec` embeddings in `.akashic/code_graph.db` with hybrid RRF retrieval.

**Blocked by:** 02 — Profile-Driven Sanitization Slice, 03 — Local Embedding Sidecar and Provider Slice

**Status:** done

- [x] Non-destructive Git ingestion reads commit trees and file blobs directly via `gix` without modifying developer worktree state or acquiring index locks.
- [x] Tree-sitter AST extractors parse declarations, methods, classes, and calls across Java, Kotlin, C++, Rust, Python, Go, and TypeScript with deterministic unique symbol IDs.
- [x] Code edges record explicit `Edge Provenance` (`EXTRACTED` for AST relations, `INFERRED` for embedding similarity, `AMBIGUOUS` for unresolved calls).
- [x] Structural Significance Filter distinguishes public API and architectural changes from internal method-body statement revisions.
- [x] `.akashic/code_graph.db` persists symbols and edges alongside SQLite FTS5 BM25 text indices and `sqlite-vec` embeddings with Reciprocal Rank Fusion (RRF) hybrid search.
- [x] Code indexing executes with zero LLM calls during indexing, meeting sub-minute benchmarks for large codebases.
