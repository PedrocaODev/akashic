# 14 — Global `GEMINI.md` and Specialist Agent Rules Alignment

**What to build:** Update `/home/pedrogoncalves/.gemini/GEMINI.md` and specialist agent templates under `/home/pedrogoncalves/.gemini/agents/` to replace legacy `graphify-out/graph.json`, `graphify query/path/explain/update`, and standalone `Graphify MCP` references with `.akashic/code_graph.db`, `akashic graph` subcommands, and the unified `akashic` MCP server.

**Blocked by:** 12 — Unified `akashic mcp` Server Registration and Tool Schema Cutover, 13 — `akashic-cli` and `graphify` Skills Clean-Slate Overhaul

**Status:** ready-for-agent

- [ ] In `/home/pedrogoncalves/.gemini/GEMINI.md`, the `oracle` row in the `## Agents and delegation` table references `Akashic MCP` instead of `Graphify MCP`.
- [ ] In `/home/pedrogoncalves/.gemini/GEMINI.md`, the `## graphify` section is replaced with `## akashic (Code Graph & Vault)`, instructing agents to check `.akashic/code_graph.db` and run `akashic graph query "<question>"`, `akashic graph path "<A>" "<B>"`, `akashic graph explain "<symbol>"`, `akashic graph blast-radius "<symbol>"`, and `akashic graph index` / `akashic graph reconcile` after code changes.
- [ ] Specialist agent definitions in `/home/pedrogoncalves/.gemini/agents/*.md` contain zero stale references to `graphify-out/graph.json` or standalone `graphify` MCP server names.
