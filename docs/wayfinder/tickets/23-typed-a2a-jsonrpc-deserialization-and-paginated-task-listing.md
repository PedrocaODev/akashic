# 23 — Typed A2A JSON-RPC Request Deserialization, Persisted Report Artifacts, and Paginated Task Listing

**What to build:** Harden and bound the A2A v1.0 HTTP/JSON-RPC 2.0 server in `akashic-daemon`. Push `status` filtering and `LIMIT` pagination down into `rca_state.db` and batch-load candidate findings for matched runs so `tasks/list` executes in $O(L)$ time and memory instead of loading all historical runs and issuing $N+1$ SQLite queries. Attach the persisted `CurationReport` artifact on `tasks/get` responses alongside the **Investigation Context Packet**, and replace cascading untyped JSON key lookups and manual parameter unpacking with typed `serde` request structures.

**Blocked by:** 22 — Composite Candidate Finding Schema, Checkpointed `rca_status`, and Persisted Curation Reports

**Status:** ready-for-agent

- [ ] `tasks/list` pushes `LIMIT` and optional state/status filters to SQLite and batch-loads `candidate_findings` for the returned runs without scanning the full `workflow_runs` table or issuing per-run queries.
- [ ] `tasks/get` includes both the **Investigation Context Packet** artifact and the persisted `CurationReport` artifact when a **Candidate Finding** has been evaluated for that task.
- [ ] `tasks/get`, `tasks/cancel`, and `tasks/send` (`message/send`) deserialize JSON-RPC `params` into typed `serde` request structs (supporting `snake_case` and `camelCase` field aliases) rather than chaining manual `serde_json::Value::get` lookups.
- [ ] Daemon A2A integration tests verify bounded `tasks/list` pagination, `CurationReport` artifact retrieval on `tasks/get`, and `camelCase` / `snake_case` request compatibility.
