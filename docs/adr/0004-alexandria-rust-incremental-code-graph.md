# 0004: Standalone Alexandria Code Graph Engine Integration

We externalize Code Graph extraction, AST diffing, symbol embeddings, and call-chain graph maintenance to a standalone Rust engine and toolchain, **Alexandria** (`~/projects/alexandria`), rather than running in-process Python extractors or polluting the RCA knowledge vault with ephemeral AST changes.

Conductor investigation workflows and RCA agents interact with Alexandria across a decoupled interface:
1. **Model Context Protocol (MCP)**: Agents query symbols, call chains (`callers`, `callees`), semantic embeddings, and graph status over standard I/O via `alexandria-cli mcp`.
2. **On-Demand Projections**: When an investigation requires a full repository visualization, Alexandria renders an atomic `graph.json` projection on-demand (`alexandria-cli project`).
3. **Zero Working-Tree Mutation**: Alexandria diffs bare Git commit objects directly, preventing branch switching or git index lock conflicts in developer workspaces.

Authoritative internal technical decisions, AST grammars, SQLite WAL schemas, embedding sidecar runbooks, and test benchmarks are maintained in Alexandria's dedicated Akashic Vault at `~/projects/alexandria/docs/index.md`.
