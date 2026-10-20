# 01 — Three-Repo Merge and Eight-Crate Workspace Prefactor

**What to build:** Consolidate the codebase structure of `akashic-records`, `alexandria`, and `knowledge-base` into a unified Rust Cargo workspace rooted in this repository. Scaffold the 8 crates under `crates/` (`akashic-sanitize`, `akashic-embedding`, `akashic-vault`, `akashic-lint`, `akashic-engine`, `akashic-rca`, `akashic-daemon`, `akashic-core`) with their strict DAG dependency graph, unify root `Cargo.toml` workspace dependencies and shared toolchains, configure the clean-slate `akashic.toml` root configuration file, and verify clean workspace compilation.

**Blocked by:** None — can start immediately

**Status:** done

- [x] Root `Cargo.toml` configures a unified Cargo workspace containing all 8 crates under `crates/` with unified dependencies (Tokio, rusqlite, serde, gix, tree-sitter, rmcp).
- [x] The 8 crates (`akashic-sanitize`, `akashic-embedding`, `akashic-vault`, `akashic-lint`, `akashic-engine`, `akashic-rca`, `akashic-daemon`, `akashic-core`) compile cleanly with zero circular dependencies adhering to ADR 0001.
- [x] Root `akashic.toml` configuration template is established defining database paths under `.akashic/` (`code_graph.db`, `rca_state.db`, `vault_index.db`), sanitization profiles, lint rules, and daemon thresholds.
- [x] `cargo check --workspace` and `cargo test --workspace` execute without error on the baseline scaffold.
