# 8. Local-Host RCA Conductor and Soak Storm Deduplication

Date: 2026-10-19

## Status

Accepted

## Context

Previous iterations of automated Root Cause Analysis (RCA) relied on heavyweight external Netflix/Orkes Conductor clusters backed by PostgreSQL/Redis and distributed Python worker processes. This architecture incurred significant operational complexity, Docker dependency overhead, and network latency. Furthermore, during OEM automated soak testing and nightly fleet runs, single regressions frequently trigger hundreds of duplicate crash reports ("soak storms"), which overwhelmed LLM reasoning pipelines and produced duplicate incident notes.

## Decision

Re-architect RCA into an in-process, host-native Rust crate (`akashic-rca`) governed by four pillars:

1. **Local-Host Conductor State Machine**:
   Replace distributed orchestrators with an embedded, pure-Rust finite state machine. Workflow execution checkpoints are persisted durably into `.akashic/rca_state.db`, allowing interrupted investigations to resume seamlessly across crashes or reboots without cloud services.

2. **Native & Java Symbolication Engine**:
   Execute crash symbolication directly in Rust via `addr2line` for native ELF binaries and Proguard/R8 `retrace` for Java/Kotlin stack traces. Maintain a local NVMe-backed Symbol Store LRU cache under `.akashic/symbols/` mapping OEM build fingerprints to unstripped symbols and mapping files.

3. **Deterministic Soak Storm Deduplication Gate**:
   Compute a cryptographic `CrashSignature` for each incoming diagnostic trigger:
   `CrashSignature = sha256(app_id + exception_or_signal + top_3_symbolicated_app_frames)`
   When an incoming trigger matches an active Incident with the same `CrashSignature`, the deduplication gate skips new LLM investigation runs and attaches the trigger as an immutable `Evidence` node to the existing `Incident`.

4. **Two-Stage Adversarial Reasoning and Crystallization**:
   Execute two distinct reasoning phases:
   - *Phase 1 (Hypothesis Generation)*: Generates structured candidate root cause hypotheses from symbolicated stacks, Code Graph call paths, and Git history.
   - *Phase 2 (Adversarial Falsification)*: Stress-tests each hypothesis against empirical logcat sequences and CPU register states, discarding invalid hypotheses into negative knowledge.
   Surviving verified findings undergo Crystallization into canonical OKF v0.2 `Incident`, `Evidence`, and `Resolution` nodes in `akashic-vault`.

## Consequences

### Positive
- Zero external orchestration dependencies: no Docker containers, Postgres databases, or Python worker runtimes required.
- Robust protection against fleet soak storms, eliminating redundant LLM reasoning runs and preventing duplicate Vault nodes.
- High-throughput symbolication via local native libraries and LRU caching.

### Negative
- Requires local storage for the Symbol Store LRU cache (`.akashic/symbols/`) to hold build mapping files and symbol tables.
