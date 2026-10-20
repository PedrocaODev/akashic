# 13 — `akashic-cli` and `graphify` Skills Clean-Slate Overhaul

**What to build:** Rewrite `/home/pedrogoncalves/.gemini/config/skills/akashic-cli/SKILL.md` and `/home/pedrogoncalves/.gemini/config/skills/graphify/SKILL.md` from scratch so that no legacy flat `akashic` commands (`akashic inspect`, `akashic query`, `akashic plan`, `akashic apply`, `akashic export`) or Python `graphifyy` / LLM chunk-indexing workflows remain. Document the unified `akashic` domain subcommands, custom frontmatter lint rules (`[lint.custom_fields]`), deterministic Tree-sitter AST + `akashic-embedding` sidecar indexing, `.akashic/` SQLite persistence (`code_graph.db`, `vault_index.db`, `rca_state.db`), and unified `akashic` MCP tools.

**Blocked by:** 11 — Unified `akashic` Binary Installation and Legacy Binary Retirement, 12 — Unified `akashic mcp` Server Registration and Tool Schema Cutover

**Status:** ready-for-agent

- [ ] `/home/pedrogoncalves/.gemini/config/skills/akashic-cli/SKILL.md` is rewritten to document `akashic vault {init,query,inspect,plan,apply,reindex}`, `akashic lint [--vault <dir>] [--strict] [--json]`, `[lint.custom_fields]` (`L012`), `akashic rca {run,resume,status,symbolicate,crystallize,supersede,start,triage}`, `akashic daemon {start,stop,status,catchup,discover,run}`, `akashic embedding {start,stop,status,benchmark}`, `akashic sanitize`, and the `vault_*` / `rca_*` MCP tools.
- [ ] `/home/pedrogoncalves/.gemini/config/skills/graphify/SKILL.md` (and any backup copy in `/home/pedrogoncalves/.gemini/config/skills.bak/graphify/SKILL.md`) is rewritten to replace all Python `uv tool run --from graphifyy`, `graphify-out/graph.json`, and LLM subagent extraction steps with deterministic Rust `akashic graph {index,reconcile,query,path,explain,communities,god-nodes,surprising,blast-radius,report,wiki,discover,symbol}` commands backed by `.akashic/code_graph.db`, `akashic-embedding`, and the `graph_*` MCP tools under the `akashic` MCP server.
- [ ] Zero references to legacy flat CLI commands (`akashic inspect`, `akashic query`, `akashic plan`, `akashic apply`, `akashic export`) or `graphifyy` Python package remain in either skill file.
