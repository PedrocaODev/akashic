---
id: "03-cargo-workspace-monorepo-layout"
title: "Cargo Workspace Monorepo and Path Crate Dependency Architecture"
label: "wayfinder:prototype"
type: "prototype"
mode: "HITL"
parent: "wayfinder-map"
blocked_by: []
assignee: "pedrogoncalves"
status: closed
resolution:
  date: "2026-10-16"
  commit: "0845817cb59c4bd1d98e3f602465c03379de9ee6"
  gist: "Scaffolded Cargo workspace monorepo (edition 2024, resolver '2') with 5 member crates wiring local path dependencies to akashic-records, alexandria-core, and alexandria-daemon."
---

# Cargo Workspace Monorepo and Path Crate Dependency Architecture

## Question

How should the root `Cargo.toml` workspace in `/home/pedrogoncalves/projects/knowledge-base` structure its member crates (`rca-conductor`, `rca-sanitizer`, `rca-symbolicator`, `rca-crystallizer`, `rca-cli`) and wire `[workspace.dependencies]` path dependencies to `akashic` (`path = "../akashic-records"`), `alexandria-core` (`path = "../alexandria/crates/alexandria-core"`), and `alexandria-daemon` (`path = "../alexandria/crates/alexandria-daemon"`), ensuring Rust edition compatibility (`2021` vs `2024`), shared Tokio/rusqlite dependency unification, and clean library APIs?

## Resolution

Successfully implemented in commit `0845817cb59c4bd1d98e3f602465c03379de9ee6`:
1. **Root Workspace Definition**: Configured `Cargo.toml` (edition 2024, resolver 2) managing 5 member crates:
   - `crates/rca-conductor`: SQLite WAL-backed durable orchestration state machine (`OrchestrationState`, `AnalysisSession`, `RunStatus`).
   - `crates/rca-sanitizer`: Extensible redaction pipeline (`Sanitizer`, `BasicSanitizer`, `SanitizedOutput`).
   - `crates/rca-symbolicator`: Android stack trace parsing and symbol resolution (`StackFrame`, `SymbolicationRequest`).
   - `crates/rca-crystallizer`: Vault crystallization and OKF node/relation bridge to `akashic-records` (`Crystallizer`, `CrystallizedArtifact`).
   - `crates/rca-cli`: Host CLI executable dispatching RCA runs.
2. **Path Dependency Unification**: Wired path dependencies to `akashic` (`/home/pedrogoncalves/projects/akashic-records`), `alexandria-core`, and `alexandria-daemon`, unifying `tokio 1.40`, `rusqlite 0.32` (`bundled`), `serde`, `serde_json`, `thiserror`, and `tracing`.
3. **Verification**: Stage 1 & Stage 2 `cargo check --workspace` compiled cleanly with 0 errors.
