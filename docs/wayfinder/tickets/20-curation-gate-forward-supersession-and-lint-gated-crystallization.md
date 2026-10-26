# 20 — Curation Gate Contract, Forward-Only Supersession, and Single-Pass Lint-Gated Crystallization

**What to build:** Enforce ADR 0003 and ADR 0004 across the **Curation Gate** and Stage 5 **Crystallization**. When a new **Resolution Node** supersedes a prior resolution, assert a forward `supersedes` **Relation** on the new node without mutating the prior **Resolution Node** file in place. Validate proposed **Vault Node** changes (`INC-*`, `EVD-*`, and `RES-*`) once in memory using `akashic-lint` and configured custom field rules before applying the **Plan** to disk, and eliminate the duplicate in-memory preview pass during commit. Route Gerrit `change-merged` updates through a lint-gated **Plan**, remove uncurated direct-write `RES-*` fallbacks from the CLI and MCP surfaces, checkpoint `HypothesisFalsified` when a **Candidate Finding** passes the **Curation Gate**, record dynamic ISO-8601 `valid_from` timestamps on new **Vault Nodes**, and deduplicate resolved **Code Symbol** identifiers in $O(F)$ time.

**Blocked by:** None — can start immediately

**Status:** ready-for-agent

- [ ] **Supersession** in the **Curation Gate** adds a forward `supersedes` **Relation** on the new **Resolution Node** and does not modify or overwrite the superseded **Resolution Node** file on disk.
- [ ] Invariant `C004` runs `akashic-lint` (including configured custom field schema rules when a policy path is provided) and applies the validated **Plan** without rebuilding the in-memory **Vault Graph** a second time or passing a no-op validator closure.
- [ ] `GerritEvent::ChangeMerged` updates the target **Resolution Node** through a validated **Plan** rather than writing directly to the filesystem before linting.
- [ ] `rca_crystallize` and `akashic rca submit-finding` require every **Candidate Finding** to pass through the **Curation Gate** and remove legacy direct-write `RES-*` fallback paths.
- [ ] Approved **Candidate Findings** record the `HypothesisFalsified` checkpoint in the **Local-Host Conductor** before transitioning to `Crystallized` or `Inconclusive`.
- [ ] Newly crystallized `Incident`, `Evidence`, and `Resolution` **Vault Nodes** record the current UTC ISO-8601 date in `valid_from` rather than a hardcoded static date string.
- [ ] `CurationIssue` uses a typed severity enum (`error` / `warning`), `C001` shares a single **Negative Knowledge** comparison helper across invalidated hypotheses and failed attempts, CLI `run` and `prepare` subcommands share one input builder, and `InvestigationContextPacket::resolved_symbol_ids` deduplicates stack frame symbols in $O(F)$ expected time using a hash set.
