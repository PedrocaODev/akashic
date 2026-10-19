# 10. Clean-Slate Domain-Subcommand CLI and Unified MCP Server

Date: 2026-10-19

## Status

Accepted

## Context

Prior to consolidation, functionality was fragmented across disparate commands and tools: `akashic-records` (Vault management), `alexandria-cli` (Code Graph AST), `rca-cli` (incident analysis), and external scripts from `graphify`. This fragmentation forced users and AI agents to juggle distinct configuration files, tool parameters, and divergent MCP server interfaces, causing confusion and maintenance burden.

## Decision

Unify the CLI entry point and agent interface cleanly in `akashic-core`:

1. **Single Unified Binary (`akashic`)**:
   Drop all legacy binary names in favor of a single, clean-slate `akashic` executable with top-level domain subcommands:
   - `akashic vault`: Query modes (`current`, `lineage`, `impact`, `historical`), mutation planner (`plan`), applier (`apply`), and crystallization.
   - `akashic lint`: Vault schema and custom field rule verification.
   - `akashic graph`: AST extraction, projection export, community detection, God Node analysis, and blast radius calculation.
   - `akashic rca`: Local-Host Conductor investigation workflows, symbolication, and triage.
   - `akashic daemon`: Gerrit event streaming, background reconciliation, and repository auto-discovery.
   - `akashic embedding`: Sidecar lifecycle management and health diagnostics.
   - `akashic sanitize`: Code and log sanitization filter testing.
   - `akashic mcp`: Unified Model Context Protocol (MCP) server over standard I/O.

2. **Single Unified Configuration (`akashic.toml`)**:
   Consolidate all system configurations—including database paths, sanitization profiles, lint rules, and daemon debounce thresholds—into a single workspace-level `akashic.toml` file.

3. **Unified `rmcp` Server**:
   Implement a unified MCP server utilizing the `rmcp` crate. Expose tools with standard `<domain>_<action>` naming (e.g., `vault_query`, `vault_plan`, `graph_query_symbol`, `graph_blast_radius`, `rca_investigate`). The server implements graceful capability degradation: if an optional sidecar (such as vector embeddings) is unavailable, structural and lexical tools continue operating cleanly.

## Consequences

### Positive
- One binary, one configuration file, and one unified MCP server provide a coherent developer and autonomous agent experience.
- Clean namespacing (`<domain>_<action>`) eliminates naming collisions and clarifies tool intent for LLM function calling.
- Graceful degradation ensures agent tooling remains functional even when local AI embedding sidecars are offline.

### Negative
- Deprecates legacy script invocations and aliases (`alexandria-cli`, `akashic-records`), requiring updates to existing external workflow scripts.
