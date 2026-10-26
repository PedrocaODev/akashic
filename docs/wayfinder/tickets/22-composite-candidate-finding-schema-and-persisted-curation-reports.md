# 22 — Composite Candidate Finding Schema, Checkpointed `rca_status`, and Persisted Curation Reports

**What to build:** Align the `candidate_findings` persistence schema and `rca_status` inspection output with Spec 0002. Key the `candidate_findings` table in `rca_state.db` by composite `(candidate_id, run_id)` and persist the submitting agent identifier (`agent_id`) as a dedicated column. Store the latest `CurationReport` JSON on the workflow run record whenever the **Curation Gate** evaluates or adjudicates a **Candidate Finding**, and return workflow stage `checkpoints` and the stored `CurationReport` from the `rca_status` MCP tool.

**Blocked by:** 20 — Curation Gate Contract, Forward-Only Supersession, and Single-Pass Lint-Gated Crystallization

**Status:** ready-for-agent

- [ ] The `candidate_findings` table in `rca_state.db` uses composite `PRIMARY KEY (candidate_id, run_id)` and stores `agent_id` in a dedicated column populated from the submitted **Candidate Finding**.
- [ ] Evaluating or adjudicating a **Candidate Finding** persists the resulting `CurationReport` JSON on the workflow run in `rca_state.db`.
- [ ] Calling `rca_status` returns the workflow run status, recorded stage `checkpoints`, stored **Investigation Context Packet**, pending `candidate_findings` (including `agent_id`), and latest `CurationReport`.
- [ ] Pipeline and MCP integration tests verify multi-agent candidate staging with composite keys, `agent_id` persistence, and `checkpoints` presence in `rca_status`.
