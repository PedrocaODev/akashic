# 18 — Betweenness Centrality God Nodes, File-Bucketed Graph Communities, and Bounded Top-K Analytics

**What to build:** Complete the Spec 0001 **Code Graph** analytics suite and eliminate quadratic and full-sort bottlenecks. Compute Brandes betweenness centrality alongside degree centrality when identifying **God Nodes**, group **Code Symbols** by `file_path` in $O(N)$ time via a hash map before constructing intra-file co-location edges for **Graph Community** detection, and replace full $O(N \log N)$ and $O(E \log E)$ sorts in `god_nodes` and `surprising_connections` with bounded $O(N + K \log K)$ top-$K$ selection.

**Blocked by:** None — can start immediately

**Status:** ready-for-agent

- [ ] **God Node** analysis computes and exposes both node degree centrality (`in_degree`, `out_degree`, `total_degree`) and unweighted Brandes betweenness centrality across the **Code Graph**.
- [ ] **Graph Community** initialization groups **Code Symbols** into per-file buckets in $O(N)$ time rather than scanning all $N(N-1)/2$ symbol pairs for `file_path` equality.
- [ ] `god_nodes` and `surprising_connections` use bounded top-$K$ selection (`select_nth_unstable_by` or a capacity-$K$ min-heap) instead of sorting the entire symbol or edge vector before truncation.
- [ ] Unit and integration tests verify betweenness centrality scores, community cohesion output, and top-$K$ ordering.
