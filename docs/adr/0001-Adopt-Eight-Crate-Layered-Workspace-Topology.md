# 1. Adopt Eight-Crate Layered Workspace Topology

Date: 2026-10-19

## Status

Accepted

## Context

Consolidating `akashic-records`, `alexandria`, and `knowledge-base` into a single unified `akashic` system requires strict architectural separation. Combining Vault document graph mechanics, lint validation rules, AST/semantic Code Graph analysis, root cause analysis (RCA) state orchestration, background daemon synchronization, sanitization, and embedding generation into a monolithic crate would lead to bloated build times, circular dependencies, and tangled responsibilities.

## Decision

Organize the `akashic` Cargo workspace into 8 focused crates under `crates/` structured as a strict Directed Acyclic Graph (DAG):

1. `akashic-sanitize`: Format-preserving semantic pseudonymization and code/log sanitization profiles without domain dependencies.
2. `akashic-embedding`: `EmbeddingProvider` trait, HTTP embedding client, and local process/container sidecar lifecycle management.
3. `akashic-vault`: Markdown parser, Open Knowledge Format (OKF) v0.2 data model with round-trip custom fields, Vault graph traversal across four query modes (`current`, `lineage`, `impact`, `historical`), mutation planner/applier, crystallizer primitives, and SQLite `vault_index.db` cache.
4. `akashic-lint`: Dedicated Vault lint rule engine (`L001`–`L011` and `.akashic.toml` custom field validation rules) depending strictly on `akashic-vault`.
5. `akashic-engine`: High-performance Code Graph engine (Rust rewrite of `graphify` and `alexandria-core`) featuring Tree-sitter AST extraction, non-destructive `gix` Git ingestion, Louvain/Leiden community detection, God Node detection, blast radius analysis, hybrid BM25/vector search, and `code_graph.db`.
6. `akashic-rca`: Host-native Local-Host Conductor state machine (`rca_state.db`), native/Java Symbolication (`addr2line`, R8 `retrace`, Symbol Store LRU cache), `CrashSignature` deduplication, and RCA Crystallization into `akashic-vault`.
7. `akashic-daemon`: Long-running background runtime managing Gerrit SSH event stream monitoring, adaptive debounce, persistent watermark catch-up, Clone-to-Onboard repository discovery, and dispatch into `akashic-engine` and `akashic-rca`.
8. `akashic-core`: Unified CLI entry point (`akashic`) and `rmcp`-based Model Context Protocol server (`akashic mcp`).

## Consequences

### Positive
- Clear single-responsibility boundaries prevent circular dependencies across domain boundaries.
- `akashic-rca` can crystallize Vault Nodes directly via `akashic-vault` without dragging in `akashic-lint`.
- Foundational crates (`akashic-sanitize` and `akashic-embedding`) are cleanly shared between Vault and Code Graph workflows.
- Modular architecture allows testing and benchmarking individual subsystems in isolation.

### Negative
- Increased initial workspace boilerplate across 8 individual `Cargo.toml` manifests.
