# 16 — Strict Plan-Then-Apply Vault Mutation and Pre-Apply Lint Gate

**What to build:** Enforce ADR 0004 across `akashic vault apply` and the Vault migrator. Require an explicit persisted **Plan** file (`--plan <path>`) when running `akashic vault apply` so the CLI never generates and applies mutations in a single unreviewed step. Validate all proposed **Vault Node** mutations against `akashic-lint` (`L001`–`L012`) before committing changes to disk, and abort atomically if any lint error occurs.

**Blocked by:** 15 — Unified CLI Context Prefactor and Vault Node Terminology Alignment

**Status:** done

- [x] `akashic vault apply` requires `--plan <path>` and rejects execution when no **Plan** file is provided, directing the operator to run `akashic vault plan` first.
- [x] Applying a **Plan** validates the resulting **Vault Graph** against `akashic-lint` rules (`L001`–`L012`) before writing modified **Vault Nodes** to disk.
- [x] If any mutated **Vault Node** fails `akashic-lint` validation, `akashic vault apply` aborts without modifying files on disk and reports the offending lint diagnostics.
- [x] End-to-end tests verify both successful two-step **Plan** application and atomic rejection when a **Plan** introduces a lint violation.
