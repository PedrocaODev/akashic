# 21 — Bounded Frontmatter Context Assembly and Indexed Gerrit Abandonment Lookup

**What to build:** Eliminate full-directory Markdown body parsing during **Investigation Context Packet** assembly and full-table workflow context deserialization during Gerrit `change-abandoned` handling. When Stage 3.5 gathers historical **Negative Knowledge** and prior **Resolution Nodes** from the **Vault**, inspect only the YAML frontmatter header block (or query the **Vault Index** when available) rather than parsing full Markdown bodies across every file in `incidents/`, `evidence/`, and `resolutions/`. Index workflow runs by **Incident** / trigger key in `rca_state.db` so `GerritEvent::ChangeAbandoned` locates the active run in $O(\log R)$ time without scanning and deserializing all historical workflow runs.

**Blocked by:** 20 — Curation Gate Contract, Forward-Only Supersession, and Single-Pass Lint-Gated Crystallization

**Status:** done

- [x] Stage 3.5 **Investigation Context Packet** assembly extracts frontmatter metadata (`scope`, `status`, `invalidated_hypotheses`, `failed_attempts`, `remediation_type`, and `abandoned_reason`) without parsing full Markdown document bodies across the **Vault**.
- [x] `rca_state.db` provides an indexed lookup to find the latest workflow run for an **Incident** or **Investigation Trigger** key in $O(\log R)$ time.
- [x] `GerritEvent::ChangeAbandoned` resolves the prior workflow run via the indexed SQLite lookup instead of calling `list_workflow_runs` and deserializing `context_json` across all historical rows.
- [x] Integration tests verify that Phase A context assembly and Gerrit `change-abandoned` auto-requeue produce identical **Negative Knowledge** and follow-up `AWAITING_REASONING` packets.
