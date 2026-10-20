# 15 — Unified CLI Context Prefactor and Vault Node Terminology Alignment

**What to build:** Load the platform configuration file (`config/akashic.toml` or `--config`) once during CLI startup into a shared runtime context that resolves the isolated SQLite database paths (`vault_index.db`, `code_graph.db`, and `rca_state.db`) and default repository names for all subcommands. Rename `note_id` and `doc` identifiers and user-facing error messages in `akashic vault query` and `akashic vault inspect` to use the canonical **Vault Node** (`node_id`) terminology from `CONTEXT.md`.

**Blocked by:** None — can start immediately

**Status:** ready-for-agent

- [ ] `akashic` loads `--config` (defaulting to `config/akashic.toml` when present) once before subcommand dispatch and supplies resolved paths for `vault_index.db`, `code_graph.db`, and `rca_state.db` to all domain subcommands.
- [ ] `akashic vault query` and `akashic vault inspect` accept `--node-id` (with `node_id` field names) and emit `"Vault Node '<id>' not found"` instead of using avoided `Note` or `Document` terms.
- [ ] `akashic graph` read subcommands (`god-nodes`, `communities`, `path`, `surprising`, `blast-radius`, `report`, `wiki`) resolve the configured or default repository name when `--repo-name` is omitted instead of querying an empty string `""`.
- [ ] CLI unit and integration tests verify shared configuration loading and **Vault Node** error output.
