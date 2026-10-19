# 08 — Host-Native RCA Conductor, Symbolication, and Crystallization Slice

**What to build:** Implement the complete Root Cause Analysis (RCA) pipeline in `akashic-rca`. Replace external Conductor orchestration with a host-native Rust state machine checkpointed durably into `.akashic/rca_state.db`, implement native/Java Symbolication (`addr2line`, R8 `retrace`) with an NVMe Symbol Store LRU cache, compute `CrashSignature` hashes to gate soak storms via the `Soak Storm Deduplication Gate`, run two-stage adversarial hypothesis generation and empirical falsification via headless local agent invocation, and crystallize verified findings into lint-validated OKF v0.2 `Incident`, `Evidence`, and `Resolution` nodes in `akashic-vault`.

**Blocked by:** 04 — OKF v0.2 Vault Graph, Custom Fields, and Vault Index Slice, 05 — Knowledge Graph Linter and Custom Schema Rules Slice, 06 — Multi-Language AST Code Graph, Gix, and Hybrid Search Slice

**Status:** ready-for-agent

- [ ] Local-Host Conductor state machine executes the 6-stage investigation workflow with durable checkpointing in `.akashic/rca_state.db`, resuming cleanly across interruptions.
- [ ] Symbolication engine resolves native crash frames via `addr2line` and Java/Kotlin obfuscated frames via R8 `retrace`, backed by a local Symbol Store LRU cache (`.akashic/symbols/`).
- [ ] Soak Storm Deduplication Gate computes deterministic `CrashSignature` hashes (`sha256(app_id + exception_signal + top_3_frames)`) and links duplicate crash tickets as `Evidence` on active incidents.
- [ ] Two-stage reasoning invokes the local headless agent CLI (`gemini`) in a sandboxed, read-only session to generate candidate hypotheses and falsify them against empirical evidence.
- [ ] Crystallization formats verified findings into canonical OKF v0.2 `Incident`, `Evidence`, and `Resolution` nodes (including `Remediation Type` and negative knowledge), validates them through `akashic-lint`, and writes them to the target Vault partition.
- [ ] CLI commands `akashic rca start <trigger_id>`, `akashic rca resume <id>`, `akashic rca status <id>`, and `akashic rca triage` control and monitor investigations.
