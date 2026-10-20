# 10 — Legacy Contraction, E2E Seam, and Agent Skill MCP Overhaul

**What to build:** Complete the contract phase of the migration and deliver the unified CLI and MCP server in `akashic-core`. Delete all remaining pre-merge legacy binaries, scripts, and temporary adapters (`alexandria-cli`, `akashic-records`, `rca-cli`, `graphify`). Implement the unified `akashic mcp` server utilizing `rmcp` exposing `<domain>_<action>` tools with graceful capability degradation when optional sidecars are offline. Verify the primary top-level integration seam (`test_e2e_akashic_pipeline`) across CLI commands and MCP JSON-RPC endpoints against a comprehensive multi-language fixture repository, and update agent skill tool configs.

**Blocked by:** 05 — Knowledge Graph Linter and Custom Schema Rules Slice, 07 — Graph Communities, God Nodes, Blast Radius, and Report Slice, 08 — Host-Native RCA Conductor, Symbolication, and Crystallization Slice, 09 — Gerrit Stream Daemon, Debounce, and Auto-Discovery Slice

**Status:** done

- [x] Unified `akashic` CLI binary in `akashic-core` dispatches all domain subcommands (`vault`, `lint`, `graph`, `rca`, `daemon`, `embedding`, `sanitize`, `mcp`) from a single executable.
- [x] Unified `rmcp` MCP server exposes namespaced tools (`vault_*`, `graph_*`, `rca_*`) over standard I/O with structured JSON schemas and graceful fallback when embedding sidecars are offline.
- [x] Legacy binaries, obsolete helper scripts, and legacy crate adapters from `alexandria`, `akashic-records`, and `knowledge-base` are completely removed.
- [x] End-to-end integration test seam (`test_e2e_akashic_pipeline`) passes against a multi-language fixture repository, verifying non-destructive Git ingestion, sanitization, community clustering, blast radius, RCA deduplication, and lint-gated crystallization.
- [x] Persistence isolation is verified by deleting `code_graph.db` and `vault_index.db` and asserting full reconstruction while `rca_state.db` checkpoints remain intact.
- [x] Agent skill definitions and MCP configuration presets are updated to point to `akashic mcp`.
