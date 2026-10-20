# 07 — Graph Communities, God Nodes, Blast Radius, and Report Slice

**What to build:** Implement advanced topological graph analysis algorithms and reporting in `akashic-engine`. Run Louvain/Leiden modularity clustering to identify `GraphCommunity` structures and intra-cluster cohesion scores, detect high-degree/betweenness `God Node` hubs and cross-community bridge edges (`surprising_connections`), compute patchset and symbol `Blast Radius` transitively, execute token-budgeted BFS/DFS and shortest-path graph traversals, and export `GRAPH_REPORT.md` / markdown wiki projections.

**Blocked by:** 06 — Multi-Language AST Code Graph, Gix, and Hybrid Search Slice

**Status:** done

- [x] Louvain/Leiden modularity algorithms detect cohesive `GraphCommunity` clusters and compute intra-cluster cohesion metrics ($0.0 \le C \le 1.0$).
- [x] Centrality scoring identifies architectural `God Nodes` and surfaces cross-community bridge edges (`surprising_connections`).
- [x] Transitive graph traversals calculate the `Blast Radius` of commits, patchsets, or modified symbols, returning impacted upstream callers and downstream dependents.
- [x] Shortest-path queries and token-budgeted BFS/DFS traversals trace call chains within bounded agent prompt token limits.
- [x] CLI commands `akashic graph communities`, `akashic graph god-nodes`, `akashic graph blast-radius`, and `akashic graph report` render structured JSON and human-readable `GRAPH_REPORT.md` wiki outputs.
