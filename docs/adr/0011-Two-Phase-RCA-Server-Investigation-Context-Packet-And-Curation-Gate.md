# 11. Two-Phase RCA Server, Investigation Context Packet, and Curation Gate

Date: 2026-10-23

## Status

Accepted (Supersedes Stage 4 headless subprocess execution in [ADR 0008](0008-Local-Host-RCA-Conductor-And-Soak-Storm-Deduplication.md))

## Context

In [ADR 0008](0008-Local-Host-RCA-Conductor-And-Soak-Storm-Deduplication.md), Stage 4 of the Local-Host Conductor pipeline (`akashic-rca`) invoked an external coding agent CLI (`gemini --headless`) as a child process via `HeadlessGeminiExecutor`. In practice, this inverted the architectural boundary between Akashic and reasoning agents:

1. **Inverted Control Flow & Recursive Nesting**: When an interactive coding agent (`agy`) called the `rca_trigger` MCP tool, Akashic attempted to spawn a second headless `gemini` CLI subprocess inside the MCP tool handler—wasting tokens, failing when the binary was absent, and hiding reasoning inside an opaque child process.
2. **Missing Context Assembly**: Stage 4 passed only a minimal prompt string rather than assembling a deterministic bundle combining sanitized stack frames, Code Graph **Blast Radius** neighborhoods, and historical Vault **Negative Knowledge**.
3. **Uncurated Direct-to-Disk Writes**: Stage 5 wrote `INC-*`, `EVD-*`, and `RES-*` Markdown files directly to disk before running `akashic-lint`, bypassing the `MigrationPlan` / `apply_plan_with_validator` atomic guarantee established in [ADR 0004](0004-Strict-Conflict-Abstention-And-Plan-Then-Apply-Vault-Mutations.md) and allowing unverified, regressive, or conflicting agent claims into the canonical **Vault**.

## Decision

Re-architect `akashic-rca`, `akashic-daemon`, and `akashic-core` into a strictly deterministic, zero-LLM **Two-Phase RCA Server and Context Engine** governed by five pillars:

1. **Elimination of `HeadlessGeminiExecutor` & Two-Phase State Machine**:
   Remove subprocess LLM invocation from `akashic-rca`. Split the Local-Host Conductor into two deterministic phases around an explicit **`AWAITING_REASONING`** checkpoint in `.akashic/rca_state.db`:
   - **Phase A (Stages 1–3.5: Ingestion & Investigation Context Packet Assembly)**: Executes trigger validation (Stage 1), `LogProfile` sanitization (Stage 2), crash symbolication and **Soak Storm Deduplication Gate** (Stage 3), and **Investigation Context Packet** assembly (Stage 3.5), then checkpoints as `AWAITING_REASONING`.
   - **Phase B (Stages 4.5–6: Curation Gate & Crystallization)**: Accepts a structured **Candidate Finding** (`CandidateFinding`) from an external reasoning agent, validates it through the deterministic **Curation Gate** (Stage 4.5), atomically crystallizes surviving nodes via `MigrationPlan` (Stage 5), and tracks Gerrit remediation lifecycle events (Stage 6).

2. **Deterministic Investigation Context Packet (Stage 3.5)**:
   Join across `.akashic/rca_state.db`, `.akashic/code_graph.db`, and `.akashic/vault_index.db` in Rust memory to construct an `InvestigationContextPacket` containing:
   - Sanitized crash slice and redaction counts (`LogProfile`).
   - Symbolicated stack frames, `CrashSignature`, and resolved `faulting_symbol`.
   - Code Graph **Blast Radius** around `faulting_symbol` (`upstream_callers`, `downstream_dependencies`, `impacted_communities`).
   - Prior Vault **Negative Knowledge** (`invalidated_hypotheses`, `failed_attempts`, and prior `RES-*` resolutions for the same **Scope** or `prior_resolution_id`).

3. **Two-Mode Transport (MCP/CLI Interactive & A2A v1.0 Autonomous Daemon)**:
   - **Agent-Driven Mode (`akashic-core` MCP & CLI)**: Interactive agents (`agy`) invoke `rca_trigger` to run Phase A and receive the `InvestigationContextPacket`, reason in their own host session, and invoke `rca_crystallize` with a `CandidateFinding` to run Phase B.
   - **Daemon-Driven Mode (`akashic-daemon` A2A v1.0 Server)**: `akashic-daemon` exposes an HTTP/JSON-RPC 2.0 Agent-to-Agent (A2A v1.0) server (`GET /.well-known/agent.json`, `POST /a2a`). Suspended runs in `AWAITING_REASONING` map to A2A task state `input-required` with the `InvestigationContextPacket` attached as an A2A Artifact; external orchestrators or fleets resume tasks by sending a `CandidateFinding` DataPart payload.

4. **Deterministic Pre-Crystallization Curation Gate (Stage 4.5)**:
   Every **Candidate Finding** submitted via MCP, CLI, or A2A must pass four deterministic curation invariants before touching the Vault:
   - *Invariant 1 — Negative-Knowledge Non-Regression (Strict)*: Reject any finding whose `primary_hypothesis` or `remediation_proposal` repeats an entry in the packet's `historical_invalidated_hypotheses` or `historical_failed_attempts`.
   - *Invariant 2 — Falsification Trace Requirement (Strict)*: Require non-empty `falsification_attempts` whenever a verified resolution is claimed (`outcome == Resolved`).
   - *Invariant 3 — Empirical Grounding Check (Advisory)*: Emit an advisory warning if `faulting_symbol` is present in the packet but unreferenced in the finding.
   - *Invariant 4 — In-Memory `MigrationPlan` Preview, Lint & Scope Conflict Gate (Strict)*: Compile proposed `INC-*`, `EVD-*`, and `RES-*` mutations into an `akashic_vault::MigrationPlan`, preview them in memory via `VaultGraph::from_dir_with_plan`, and enforce `akashic_lint::lint_vault` (0 errors) and `query_current` (no un-superseded **Unresolved Conflict** in the same **Scope**) before atomic disk commit via `apply_plan_with_validator`.

5. **Partial Crystallization on Inconclusive Findings, Multi-Agent Adjudication (`AWAITING_CURATION`), and Gerrit Abandonment Re-Queuing**:
   - **Inconclusive Abstention**: When an agent falsifies all candidate hypotheses (`outcome == Inconclusive`), Stage 5 writes `INC-*` (`status: active`) and `EVD-*` (recording the newly falsified `invalidated_hypotheses` as **Negative Knowledge**), abstains from writing a `RES-*` node, and transitions the run to `INCONCLUSIVE`.
   - **Multi-Agent Competing Findings (`AWAITING_CURATION`)**: When multiple agents submit competing `CandidateFinding` payloads for the same Incident (or when a finding is held for review), Akashic records each candidate in `.akashic/rca_state.db` and holds the run at `AWAITING_CURATION`. Upon curation (`curate_investigation`), the winning candidate is crystallized into `RES-*` while all rejected candidate hypotheses are automatically folded into `invalidated_hypotheses` (**Negative Knowledge**).
   - **Gerrit `change-abandoned` Auto-Requeue**: When `GerritEvent::ChangeAbandoned` fires for an active Resolution CL, Akashic marks the `RES-*` node `abandoned`, appends the abandonment reason to `failed_attempts`, automatically queues a follow-up workflow run in `AWAITING_REASONING` pre-linked with `prior_resolution_id` and a refreshed `InvestigationContextPacket`, and updates `storm_signatures.active_run_id` to the new run.

## Consequences

### Positive
- Zero LLM execution inside Akashic: `akashic-rca` and `akashic-daemon` remain 100% deterministic Rust engines.
- Eliminates recursive CLI spawning and provides full **Blast Radius** and **Negative Knowledge** context to reasoning agents.
- Enforces [ADR 0004](0004-Strict-Conflict-Abstention-And-Plan-Then-Apply-Vault-Mutations.md) plan-then-apply atomicity and prevents regressive or un-falsified findings from polluting the canonical Vault.
- Supports both interactive single-agent debugging (MCP/CLI) and autonomous fleet triage (A2A v1.0 HTTP/JSON-RPC) over the same SQLite state machine.

### Negative
- Two-phase execution requires external callers (or test harnesses) to either drive the two-step `prepare_investigation` + `submit_finding` flow or explicitly opt into an in-process `Stage4Executor` convenience wrapper for single-shot tests.
