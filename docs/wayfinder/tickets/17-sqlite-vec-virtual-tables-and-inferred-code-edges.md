# 17 — `sqlite-vec` Virtual Table Persistence and Threshold-Faithful Inferred Code Edges

**What to build:** Replace JSON-text embedding storage and brute-force in-memory cosine similarity loops in the **Code Graph** (`code_graph.db`) and **Vault Index** (`vault_index.db`) with `sqlite-vec` virtual tables and indexed vector queries as mandated by Spec 0001. Normalize unordered **Code Symbol** pairs to eliminate duplicate tuple allocations and redundant distance checks, remove the unrequested top-5 truncation cap so all `INFERRED` **Code Edges** meeting the configured similarity threshold (`>= threshold`) are persisted, and commit the batched upsert and secondary index improvements.

**Blocked by:** None — can start immediately

**Status:** done

- [x] `code_graph.db` and `vault_index.db` register and store embeddings in `sqlite-vec` virtual tables (`vec0`) rather than `vector_json TEXT` columns.
- [x] Semantic search and `INFERRED` **Code Edge** generation query `sqlite-vec` indexed vectors instead of executing an in-memory $O(N^2)$ nested loop across all vectors.
- [x] All candidate pairs with cosine similarity `>= inferred_similarity_threshold` (default `0.82`) are persisted as `INFERRED` **Code Edges** without an arbitrary `.truncate(5)` cap.
- [x] Unordered **Code Symbol** pair deduplication uses a canonical key ordering (`min(a, b), max(a, b)`) without duplicate reverse-tuple clones.
- [x] Relational indexes and batched SQLite transactions (`upsert_symbols_batch`, `upsert_edges_batch`) are committed and verified by integration tests.
