# 04 — OKF v0.2 Vault Graph, Custom Fields, and Vault Index Slice

**What to build:** Implement `akashic-vault` providing the complete Markdown parsing engine with Open Knowledge Format (OKF) v0.2 frontmatter, lossless custom field round-tripping (`#[serde(flatten)] pub custom: BTreeMap<String, Value>`), forward-only relation graph traversal across 4 query modes (`current`, `lineage`, `impact`, `historical`), strict conflict abstention on cycles/competing successors, two-phase mutation planning (`akashic vault plan` / `akashic vault apply`), and the disposable SQLite `vault_index.db` cache reconstructible from Git Markdown files.

**Blocked by:** 02 — Profile-Driven Sanitization Slice, 03 — Local Embedding Sidecar and Provider Slice

**Status:** done

- [x] Parser reads OKF v0.2 Markdown files and preserves arbitrary custom YAML frontmatter attributes via `#[serde(flatten)]` without data loss on serialization.
- [x] Forward-only relation graph traversals power four query modes: `current` (resolves active guidance), `lineage` (traces historical supersession chains), `impact` (identifies dependent nodes), and `historical` (evaluates point-in-time state).
- [x] Graph engine detects supersession cycles and competing active successors, strictly abstains from guessing winners, and marks branches as `Unresolved Conflict`.
- [x] Mutation planner implements `akashic vault plan` (emits reviewable diff) and `akashic vault apply` (atomically commits valid updates).
- [x] SQLite `vault_index.db` caches parsed node metadata, relations, and FTS5 search indexes under `.akashic/`, reconstructible from scratch via `akashic vault reindex`.
