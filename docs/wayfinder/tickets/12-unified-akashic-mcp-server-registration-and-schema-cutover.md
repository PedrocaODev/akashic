# 12 — Unified `akashic mcp` Server Registration and Tool Schema Cutover

**What to build:** Cut over Antigravity's MCP configuration (`/home/pedrogoncalves/.gemini/config/mcp_config.json`) and cached MCP tool schemas (`/home/pedrogoncalves/.gemini/antigravity-cli/mcp/akashic`) to the unified `akashic mcp` stdio server (`crates/akashic-core/src/mcp.rs`), and remove the standalone `graphify` (`graphify-mcp`) MCP server registration and schema directory.

**Blocked by:** 11 — Unified `akashic` Binary Installation and Legacy Binary Retirement

**Status:** done

- [x] `/home/pedrogoncalves/.gemini/config/mcp_config.json` configures the `akashic` server with `command: "/home/pedrogoncalves/.cargo/bin/akashic"` and `args: ["mcp", "--vault", "/home/pedrogoncalves/second-brain", "--workspace", "/home/pedrogoncalves/projects/knowledge-base"]`.
- [x] The standalone `"graphify"` server entry (`graphify-mcp`) is removed from `/home/pedrogoncalves/.gemini/config/mcp_config.json`, and `/home/pedrogoncalves/.gemini/antigravity-cli/mcp/graphify` is removed.
- [x] `/home/pedrogoncalves/.gemini/antigravity-cli/mcp/akashic` contains updated JSON tool schema definitions and `instructions.md` matching all 17 unified tools (`vault_query`, `vault_inspect`, `vault_lint`, `vault_read`, `vault_write_canonical`, `graph_query`, `graph_get_node`, `graph_get_neighbors`, `graph_get_community`, `graph_god_nodes`, `graph_stats`, `graph_shortest_path`, `graph_blast_radius`, `rca_status`, `rca_trigger`, `rca_symbolicate`, `rca_crystallize`) and legacy `akashic_inspect.json`, `akashic_lint.json`, and `akashic_query.json` schemas are removed.
- [x] A live JSON-RPC `tools/list` and `tools/call` smoke check against `/home/pedrogoncalves/.cargo/bin/akashic mcp` returns all 17 tools and valid JSON responses.
