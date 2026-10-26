---
id: wayfinder-map
title: "OEM Android Knowledge & RCA Infrastructure"
label: "wayfinder:map"
status: completed
---

# Wayfinder Map: OEM Android Knowledge & RCA Infrastructure

## Destination

Deliver an end-to-end, local-host deployable unified **Akashic** platform in `/home/pedrogoncalves/projects/knowledge-base` as specified in [0001-unified-akashic-architecture.md](../specs/0001-unified-akashic-architecture.md). Evolve the workspace into a cohesive 8-crate Cargo workspace (`akashic-sanitize`, `akashic-embedding`, `akashic-vault`, `akashic-lint`, `akashic-engine`, `akashic-rca`, `akashic-daemon`, `akashic-core`) providing a clean-slate domain-subcommand CLI (`akashic`) and unified MCP server (`akashic mcp`), physically isolated SQLite databases (`code_graph.db`, `rca_state.db`, `vault_index.db`), deterministic multi-language Tree-sitter AST and local embedding sidecar indexing, profile-driven sanitization, zero-configuration Clone-to-Onboard Discovery across Android PoC repositories, and host-native Local-Host Conductor RCA with lint-gated OKF v0.2 crystallization into partitioned Vaults (`akashic/<app_id>/`).

## Notes

- **Unified 8-Crate Layered Topology**: Consolidates `akashic-records`, `alexandria`, and `knowledge-base` into an 8-crate DAG (`akashic-sanitize`, `akashic-embedding`, `akashic-vault`, `akashic-lint`, `akashic-engine`, `akashic-rca`, `akashic-daemon`, `akashic-core`) eliminating duplicated code and circular dependencies.
- **Physical SQLite Persistence Isolation**: Isolates WAL persistence across three domain-specific files under `.akashic/` (`code_graph.db`, `rca_state.db`, `vault_index.db`) while keeping Markdown files in Git as the canonical Vault source of truth.
- **Deterministic Zero-LLM Code Indexing**: `akashic-engine` uses multi-language Tree-sitter grammars (Java, Kotlin, C++, Rust, Python, Go, TypeScript) for AST structural extraction (`EXTRACTED` edges) and local embedding sidecars (`akashic-embedding`) for hybrid RRF retrieval and `INFERRED` similarity links without making LLM calls during indexing.
- **Host-Native Execution & Credential Isolation**: All tools that touch developer credentials (`gemini`, `motojira-mcp`, `gerrit-mcp`, SSH keys, `.netrc`) execute natively on the host workstation. No developer credentials or API keys are mounted into or exposed to Docker containers.
- **Local-Host Conductor**: Replaces the heavy Dockerized Orkes Conductor stack with a lightweight Rust state machine (`akashic-rca`) backed by durable checkpointing in `rca_state.db`, deterministic retries, native/Java Symbolication with Symbol Store LRU cache, and the **Soak Storm Deduplication Gate**.
- **Clone-to-Onboard Discovery**: Any Motorola Android repository cloned into the workspace is automatically discovered by `akashic-daemon`, indexed into the **Package-to-Repository Index** (`applicationId`, `namespace`, and manifest `package` attributes mapped to Gerrit project, local Git worktree, SQLite Code Graph, and `akashic/<app_id>/` Vault partition) without hardcoded project lists.
- **Unified CLI & MCP Server**: Single `akashic` binary in `akashic-core` exposing domain subcommands (`vault`, `lint`, `graph`, `rca`, `daemon`, `embedding`, `sanitize`, `mcp`) and unified `<domain>_<action>` MCP tools with graceful capability degradation.

## Decisions so far

- Android Package-to-Repository Auto-Discovery across Multi-Module Gradle and Manifest Trees: 5-pass static extractor backed by SQLite tables with role-priority and `crash_app_id_hint` disambiguation.
- Gemini CLI Headless Invocation and Local-Host Conductor Integration Contract: Headless subprocess invocation with zero paid API keys.
- Cargo Workspace Monorepo and Path Crate Dependency Architecture: Workspace layout and path dependency consolidation.
- Alexandria Daemon RCA Extensions: Vault Loop Filter, 5-Min Debounce, and Package Registry.
- Local-Host Conductor 6-Stage Pipeline and SQLite Durable State Machine: Host-native RCA pipeline and durable state checkpointing.
- [ADRs 0001–0010: Clean Consolidated Baseline](../adr/0001-Adopt-Eight-Crate-Layered-Workspace-Topology.md): Established canonical architectural decision records covering the 8-crate workspace, isolated SQLite databases, OKF v0.2 forward-only relations, conflict abstention, deterministic AST indexing, non-destructive Git ingestion, profile-driven sanitization, Local-Host RCA Conductor, Gerrit stream adaptive debounce, and clean-slate domain-subcommand CLI.
- [Spec 0001: Unified Akashic Architecture](../specs/0001-unified-akashic-architecture.md): Authoritative architecture specification (`spec-0001-unified-akashic-architecture`).
- [ADR 0011: Two-Phase RCA Server, Investigation Context Packet, and Curation Gate](../adr/0011-Two-Phase-RCA-Server-Investigation-Context-Packet-And-Curation-Gate.md): Eliminates subprocess LLM invocation in favor of a two-phase deterministic RCA server, Investigation Context Packet, and 4-layer Curation Gate.
- [Spec 0002: Two-Phase Deterministic RCA Server, Investigation Context Packet, and Curation Gate](../specs/0002-two-phase-rca-server-and-curation-gate.md): Authoritative specification for Phase A context assembly, Phase B curation gate, partial crystallization, multi-agent adjudication, and A2A v1.0 server.

## Frontier (Unblocked)

*(None)*

## Blocked

*(None)*

## Completed

- [01 — Three-Repo Merge and Eight-Crate Workspace Prefactor](tickets/01-three-repo-merge-and-eight-crate-workspace-prefactor.md) (`done`)
- [02 — Profile-Driven Sanitization Slice](tickets/02-profile-driven-sanitization-slice.md) (`done`)
- [03 — Local Embedding Sidecar and Provider Slice](tickets/03-local-embedding-sidecar-and-provider-slice.md) (`done`)
- [04 — OKF v0.2 Vault Graph, Custom Fields, and Vault Index Slice](tickets/04-okf-v02-vault-graph-custom-fields-and-vault-index-slice.md) (`done`)
- [05 — Knowledge Graph Linter and Custom Schema Rules Slice](tickets/05-knowledge-graph-linter-and-custom-schema-rules-slice.md) (`done`)
- [06 — Multi-Language AST Code Graph, Gix, and Hybrid Search Slice](tickets/06-multi-language-ast-code-graph-gix-and-hybrid-search-slice.md) (`done`)
- [07 — Graph Communities, God Nodes, Blast Radius, and Report Slice](tickets/07-graph-communities-god-nodes-blast-radius-and-report-slice.md) (`done`)
- [08 — Host-Native RCA Conductor, Symbolication, and Crystallization Slice](tickets/08-host-native-rca-conductor-symbolication-and-crystallization-slice.md) (`done`)
- [09 — Gerrit Stream Daemon, Debounce, and Auto-Discovery Slice](tickets/09-gerrit-stream-daemon-debounce-and-auto-discovery-slice.md) (`done`)
- [10 — Legacy Contraction, E2E Seam, and Agent Skill MCP Overhaul](tickets/10-legacy-contraction-e2e-seam-and-agent-skill-mcp-overhaul.md) (`done`)
- [11 — Unified `akashic` Binary Installation and Legacy Binary Retirement](tickets/11-unified-akashic-binary-installation-and-legacy-retirement.md) (`done`)
- [12 — Unified `akashic mcp` Server Registration and Tool Schema Cutover](tickets/12-unified-akashic-mcp-server-registration-and-schema-cutover.md) (`done`)
- [13 — `akashic-cli` and `graphify` Skills Clean-Slate Overhaul](tickets/13-akashic-cli-and-graphify-skills-clean-slate-overhaul.md) (`done`)
- [14 — Global `GEMINI.md` and Specialist Agent Rules Alignment](tickets/14-global-gemini-md-and-specialist-agent-rules-alignment.md) (`done`)
- [15 — Unified CLI Context Prefactor and Vault Node Terminology Alignment](tickets/15-unified-cli-context-prefactor-and-vault-node-terminology.md) (`done`)
- [16 — Strict Plan-Then-Apply Vault Mutation and Pre-Apply Lint Gate](tickets/16-strict-plan-then-apply-vault-mutation-and-lint-gate.md) (`done`)
- [17 — `sqlite-vec` Virtual Table Persistence and Threshold-Faithful Inferred Code Edges](tickets/17-sqlite-vec-virtual-tables-and-inferred-code-edges.md) (`done`)
- [18 — Betweenness Centrality God Nodes, File-Bucketed Graph Communities, and Bounded Top-K Analytics](tickets/18-betweenness-centrality-god-nodes-and-bounded-graph-analytics.md) (`done`)
- [19 — $O(\log N)$ Symbol Store LRU Cache Eviction and Min-Heap Adaptive Debouncer](tickets/19-logarithmic-symbol-cache-lru-and-min-heap-debouncer.md) (`done`)
- [20 — Curation Gate Contract, Forward-Only Supersession, and Single-Pass Lint-Gated Crystallization](tickets/20-curation-gate-forward-supersession-and-lint-gated-crystallization.md) (`done`)
- [21 — Bounded Frontmatter Context Assembly and Indexed Gerrit Abandonment Lookup](tickets/21-bounded-frontmatter-context-assembly-and-indexed-abandonment-lookup.md) (`done`)
- [22 — Composite Candidate Finding Schema, Checkpointed `rca_status`, and Persisted Curation Reports](tickets/22-composite-candidate-finding-schema-and-persisted-curation-reports.md) (`done`)
- [23 — Typed A2A JSON-RPC Request Deserialization, Persisted Report Artifacts, and Paginated Task Listing](tickets/23-typed-a2a-jsonrpc-deserialization-and-paginated-task-listing.md) (`done`)

*(Note: Stage 1, 2, 3, & 4 tickets 01–23 are complete.)*
