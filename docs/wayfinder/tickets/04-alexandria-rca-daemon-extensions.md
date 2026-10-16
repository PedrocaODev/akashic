---
id: "04-alexandria-rca-daemon-extensions"
title: "Alexandria Daemon RCA Extensions: Vault Loop Filter, 5-Min Debounce, and Package Registry"
label: "wayfinder:prototype"
type: "prototype"
mode: "HITL"
parent: "wayfinder-map"
blocked_by: []
assignee: "pedrogoncalves"
status: closed
resolution:
  date: "2026-10-16"
  commit: "70277d80564d4e140f3c5d038209b1b399cb9f00"
  gist: "Implemented Gerrit self-trigger loop drop filter (akashic/*), 5-min sliding debounce with 3-stage AST filter, and SQLite package-to-repository autodiscovery index."
---

# Alexandria Daemon RCA Extensions: Vault Loop Filter, 5-Min Debounce, and Package Registry

## Question

How should `alexandria-daemon` and `alexandria-core` be extended to implement the remaining Spec 0001 and Spec 0003 contracts:
1. **Self-Trigger Drop Filter**: Immediately dropping Gerrit events where `project` matches `akashic/*` or `akashic-*` to prevent infinite synchronization loops.
2. **Per-Repository 5-Minute Sliding Debounce & Three-Stage Structural Significance Filter**: Implementing the `Idle -> Accumulating -> Diffing -> Notifying` state machine and AST diff filter (`Stage 1: File Exclusion`, `Stage 2: Tree-Sitter Declaration Diff`, `Stage 3: Visibility Gate`) before notifying the RCA pipeline.
3. **Clone-to-Onboard Package-to-Repository Index**: Persisting the auto-discovered mapping from Android `app_id` / package prefix to local Git path, Gerrit project, and SQLite Code Graph path, and exposing it via CLI/library API.

## Resolution

Successfully implemented in commit `70277d80564d4e140f3c5d038209b1b399cb9f00` in repository `alexandria`:
1. **Self-Trigger Drop Filter**: Added `is_akashic_self_trigger(project)` in `alexandria-daemon::watcher`, early-dropping events matching `akashic/*` and `akashic-*` across real-time ingestion, downtime catchup replay (`catchup.rs`), and pipeline ingestion (`pipeline.rs`).
2. **5-Minute Debounce & 3-Stage AST Significance Gate**: Added `alexandria-daemon::debounce` module implementing the per-repository `Idle -> Accumulating -> Diffing -> Notifying` sliding window controller (300s window), file exclusions, tree-sitter declaration diff, and visibility checks.
3. **Clone-to-Onboard Discovery Index**: Added `android_repositories` and `android_package_index` tables in `alexandria-core::store`, with queries for `resolve_app_id` and `resolve_stack_symbol`. Implemented static 5-pass zero-execution discovery scanner in `alexandria-daemon::discovery` supporting AGP 8+ namespace, nested settings.gradle, flavor suffixes, and source trees.
4. **Verification**: Stage 1 & Stage 2 verification (`cargo test --workspace`) passed all 134 tests across `alexandria-cli`, `alexandria-core`, and `alexandria-daemon`.
