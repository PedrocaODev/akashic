---
id: "03-cargo-workspace-monorepo-layout"
title: "Cargo Workspace Monorepo and Path Crate Dependency Architecture"
label: "wayfinder:prototype"
type: "prototype"
mode: "HITL"
parent: "wayfinder-map"
blocked_by: []
assignee: "pedrogoncalves"
status: open
---

# Cargo Workspace Monorepo and Path Crate Dependency Architecture

## Question

How should the root `Cargo.toml` workspace in `/home/pedrogoncalves/projects/knowledge-base` structure its member crates (`rca-conductor`, `rca-sanitizer`, `rca-symbolicator`, `rca-crystallizer`, `rca-cli`) and wire `[workspace.dependencies]` path dependencies to `akashic` (`path = "../akashic-records"`), `alexandria-core` (`path = "../alexandria/crates/alexandria-core"`), and `alexandria-daemon` (`path = "../alexandria/crates/alexandria-daemon"`), ensuring Rust edition compatibility (`2021` vs `2024`), shared Tokio/rusqlite dependency unification, and clean library APIs?
