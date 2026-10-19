# 5. Use Deterministic AST and Embedding Sidecar Instead of LLM Indexing

Date: 2026-10-19

## Status

Accepted

## Context

Reference tools such as Python `graphify` rely on LLM subagent prompts during codebase indexing to infer entity relationships, summarize code blocks, and construct dependency graphs. At Android OEM repository scale (millions of lines of Java, Kotlin, C++, and Rust), executing LLM calls during indexing is prohibitively slow, expensive, and non-deterministic. Conversely, purely lexical search or AST extraction misses semantic similarities across decoupled modules and documentation.

## Decision

Prohibit LLM calls during codebase and Vault indexing, delegating structural analysis to deterministic Tree-sitter parsers and semantic analysis to local embedding sidecars:

1. **Deterministic AST Extraction (`akashic-engine`)**:
   Implement a high-performance native Rust engine (consolidating `graphify` and `alexandria-core` concepts) utilizing Tree-sitter grammars (Java, Kotlin, C++, Rust, Python, Go, TypeScript). All structural relationships (calls, inheritance, type references) are marked with `EXTRACTED` edge provenance.

2. **Graph Topology Analysis**:
   Graph algorithms operate deterministically over in-memory petgraph structures:
   - **Community Detection**: Partition the Code Graph into cohesive `GraphCommunity` clusters using Louvain/Leiden modularity optimization.
   - **God Node Detection**: Identify high-degree, high-betweenness centrality hubs that bridge multiple communities.
   - **Blast Radius & Shortest Path**: Calculate the transitive impact radius of commit changes and patchsets deterministically.

3. **Embedding Sidecar (`akashic-embedding`)**:
   Define an `EmbeddingProvider` trait and manage the local process lifecycle for a lightweight `llama.cpp` / ONNX sidecar. When online, the engine generates dense vector embeddings stored in `code_graph.db` via `sqlite-vec`.

4. **Hybrid Retrieval and Inferred Relations**:
   - Combine dense vector similarity and sparse SQLite FTS5 BM25 lexical scores using Reciprocal Rank Fusion (RRF) for code and vault queries.
   - Assert `INFERRED` edge provenance for relationships derived from high-confidence cosine similarity thresholds, clearly separating deterministic code links from heuristic associations.

## Consequences

### Positive
- Sub-minute, deterministic indexing of large OEM codebases without LLM token cost or latency.
- Clear edge provenance distinguishing compiler/AST verified links (`EXTRACTED`) from semantic associations (`INFERRED`).
- Graceful degradation: if the embedding sidecar is unavailable, indexing and structural graph queries continue uninterrupted using AST and lexical FTS5 search.

### Negative
- Requires a local embedding runtime sidecar (`llama.cpp` / ONNX) and dedicated memory footprint when semantic vector search is enabled.
