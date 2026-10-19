---
id: "05-localhost-rca-pipeline-orchestrator"
title: "Local-Host Conductor 6-Stage Pipeline and SQLite Durable State Machine"
label: "wayfinder:grilling"
type: "grilling"
mode: "HITL"
parent: "wayfinder-map"
blocked_by: []
assignee: "pedrogoncalves"
status: closed
resolution:
  date: "2026-10-18"
  commits:
    - "06799c218226bfe0f7eb042a19d0a43a528aaeca"
    - "92765cf988564774bf305b35505d38db73124bcd"
  gist: "Implemented 6-stage local-host RCA pipeline, SQLite WAL durable state machine, format-preserving sanitizer, stack symbolicator, Akashic crystallizer with forward-only lint gate, and rca-cli commands."
---

# Local-Host Conductor 6-Stage Pipeline and SQLite Durable State Machine

## Question

How should the Rust **Local-Host Conductor** (`rca-conductor`) orchestrate the 6-stage RCA pipeline (`Stage 1: Triage & Storm Gate`, `Stage 2: Log Sanitization`, `Stage 3: Native & Java Symbolication`, `Stage 4: Hypothesis Generation & Adversarial Falsification via gemini-cli`, `Stage 5: Akashic Crystallization & Lint Gate`, `Stage 6: Resolution Tracking & Supersession`) on the developer workstation using SQLite WAL durable state checkpointing, replacing the heavy Dockerized Orkes Conductor (Java/Postgres/Redis) stack while preserving deterministic retries, Crash Signature deduplication (`Soak Storm Deduplication Gate`), and local credential isolation?

## Resolution

Successfully implemented across `lane/wayfinder-ticket05-stage-crates` (`06799c2`) and `lane/wayfinder-ticket05-conductor-engine` (`92765cf`):
1. **Leaf Domain Stage Libraries**:
   - `crates/rca-sanitizer`: `FormatPreservingSanitizer` redacting phone numbers, IMEIs, emails, tokens/credentials, and OEM secrets while preserving formatting.
   - `crates/rca-symbolicator`: `StackFrame` parser for Java/Kotlin exceptions and native tombstones, deterministic `compute_crash_signature`, and `AlexandriaSymbolResolver` querying `alexandria-core` SQLite CodeGraph store.
   - `crates/rca-crystallizer`: OKF v0.2 `IncidentNode`, `EvidenceNode`, and `ResolutionNode` serialization with forward-only relation validation and `akashic::linter::lint_vault` integration gate.
2. **Local-Host Conductor Engine (`crates/rca-conductor`)**:
   - **SQLite WAL State Machine**: Backed by `rusqlite`, enforcing WAL mode, busy timeout, and tables `rca_sessions`, `rca_checkpoints`, and `rca_active_incidents`.
   - **6-Stage Pipeline Orchestrator**:
     - *Stage 1 (Triage & Storm Gate)*: Crash signature calculation, 24h storm gate deduplication with early exit.
     - *Stage 2 (Log Sanitization)*: Scrubbing sensitive tokens and PII from raw logs.
     - *Stage 3 (Symbolication)*: Resolving frames via `SymbolResolver` trait.
     - *Stage 4 (Hypothesis Generation & Falsification)*: `Stage4Runner` trait supporting headless sandboxed `gemini-cli` subprocess runner (`stage4-readonly.toml`) with mock runner fallback.
     - *Stage 5 (Akashic Crystallization & Lint Gate)*: Writing OKF nodes and validating vault cleanliness (0 errors).
     - *Stage 6 (Resolution Tracking & Supersession)*: Writing resolution node with `resolves` and forward `supersedes` relations.
   - **Durable Resumption**: `resume_session` reads checkpoints from SQLite WAL and resumes from the exact pending stage.
3. **CLI Interface (`crates/rca-cli`)**:
   - Subcommands `start`, `resume`, `status`, and `list`.
4. **Verification**: All 25 unit and integration tests passed cleanly in Stage 1 and Stage 2 verification gates.
