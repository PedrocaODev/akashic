---
id: "04-alexandria-rca-daemon-extensions"
title: "Alexandria Daemon RCA Extensions: Vault Loop Filter, 5-Min Debounce, and Package Registry"
label: "wayfinder:prototype"
type: "prototype"
mode: "HITL"
parent: "wayfinder-map"
blocked_by: []
assignee: "pedrogoncalves"
status: open
---

# Alexandria Daemon RCA Extensions: Vault Loop Filter, 5-Min Debounce, and Package Registry

## Question

How should `alexandria-daemon` and `alexandria-core` be extended to implement the remaining Spec 0001 and Spec 0003 contracts:
1. **Self-Trigger Drop Filter**: Immediately dropping Gerrit events where `project` matches `akashic/*` or `akashic-*` to prevent infinite synchronization loops.
2. **Per-Repository 5-Minute Sliding Debounce & Three-Stage Structural Significance Filter**: Implementing the `Idle -> Accumulating -> Diffing -> Notifying` state machine and AST diff filter (`Stage 1: File Exclusion`, `Stage 2: Tree-Sitter Declaration Diff`, `Stage 3: Visibility Gate`) before notifying the RCA pipeline.
3. **Clone-to-Onboard Package-to-Repository Index**: Persisting the auto-discovered mapping from Android `app_id` / package prefix to local Git path, Gerrit project, and SQLite Code Graph path, and exposing it via CLI/library API.
