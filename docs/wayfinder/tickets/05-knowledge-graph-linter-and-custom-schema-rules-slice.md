# 05 — Knowledge Graph Linter and Custom Schema Rules Slice

**What to build:** Implement the dedicated `akashic-lint` crate providing standard OKF v0.2 validation rules (`L001`–`L011`) and configurable project-specific schema rules declared under `[lint.custom_fields]` in `akashic.toml`. Expose CLI execution via `akashic lint` and programmatic validation interfaces used by `akashic vault apply` and RCA crystallization gates to prevent invalid nodes or relations from landing on disk or Git review branches.

**Blocked by:** 04 — OKF v0.2 Vault Graph, Custom Fields, and Vault Index Slice

**Status:** done

- [x] Linter implements standard rules `L001` through `L011`, catching broken target references, missing required fields, unnormalized node IDs, invalid enum types, and untyped links.
- [x] Linter parses and enforces project-specific custom field rules from `akashic.toml` (e.g. required regex patterns, required scopes, allowed values).
- [x] Detailed diagnostic reporting surfaces file paths, line numbers, error severities, and suggested auto-fixes.
- [x] Programmatic lint runner gates `akashic vault apply` and RCA crystallization, rejecting invalid mutations before disk commits.
- [x] CLI command `akashic lint [--path <vault_path>]` runs fast workspace validation and returns standard exit codes (`0` clean, `1` lint errors).
