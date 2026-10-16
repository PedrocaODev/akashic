---
id: wayfinder-map
title: "OEM Android Knowledge & RCA Infrastructure"
label: "wayfinder:map"
status: in_progress
---

# Wayfinder Map: OEM Android Knowledge & RCA Infrastructure

## Destination

Deliver an end-to-end, local-host deployable OEM Android Knowledge & Root Cause Analysis (RCA) system in `/home/pedrogoncalves/projects/knowledge-base` that integrates `akashic-records` and `alexandria` via Rust workspace path dependencies, implements zero-configuration **Clone-to-Onboard Discovery** across the 4 Motorola Android PoC repositories (`AudioRecorder`, `FMRadio`, `MotoAssistPremium`, `motoDolbyUI`), executes the 6-stage RCA investigation pipeline via the Rust **Local-Host Conductor** and headless `gemini-cli`, and crystallizes lint-clean OKF v0.2 `INC-`, `EVD-`, and `RES-` nodes into partitioned Vaults (`akashic/<app_id>/`).

## Notes

- **Host-Native Execution & Credential Isolation**: All tools that touch developer credentials (`gemini-cli`, `motojira-mcp`, `gerrit-mcp`, SSH keys, `.netrc`) execute natively on the host workstation. No developer credentials or API keys are mounted into or exposed to Docker containers.
- **LLM Reasoning Engine**: Stage 4 Hypothesis Generation & Adversarial Falsification invokes the local authenticated `gemini-cli` (`gemini`) session headlessly as a subprocess (`--output-format json`) with zero paid API keys.
- **Local-Host Conductor**: Replaces the heavy Dockerized Orkes Conductor (Java/Postgres/Redis) stack with a lightweight Rust state machine (`rca-conductor`) backed by SQLite WAL durable checkpointing, deterministic retries, and the **Soak Storm Deduplication Gate**.
- **Cargo Workspace Monorepo**: `/home/pedrogoncalves/projects/knowledge-base` serves as a Rust Cargo workspace wiring `akashic` (`../akashic-records`), `alexandria-core` (`../alexandria/crates/alexandria-core`), and `alexandria-daemon` (`../alexandria/crates/alexandria-daemon`) via `[workspace.dependencies]` path dependencies.
- **Clone-to-Onboard Discovery**: Any Motorola Android repository cloned into `~/projects/` is automatically discovered by `alexandria-daemon`, indexed into the **Package-to-Repository Index** (`applicationId`, `namespace`, and `AndroidManifest.xml` `package` attributes mapped to Gerrit project, local Git worktree, SQLite Code Graph, and `akashic/<app_id>/` Vault partition) without hardcoded project lists.
- **Embedding Sidecar**: Uses the existing local embedding sidecar endpoint on `http://127.0.0.1:8080/embed` (`768` dimensions).

## Decisions so far

- [Android Package-to-Repository Auto-Discovery across Multi-Module Gradle and Manifest Trees](tickets/01-android-package-autodiscovery.md): 5-pass static extractor (`settings.gradle` at root or `application/`, root `ext`/properties, module `build.gradle` with AGP 8+ `namespace` fallback & flavor overrides/suffixes, `AndroidManifest.xml`, and source package roots) backed by SQLite `android_repositories` and `android_package_index` tables with role-priority and `crash_app_id_hint` disambiguation.
- [Gemini CLI Headless Invocation and Local-Host Conductor Integration Contract](tickets/02-gemini-cli-headless-contract.md): Invoke `gemini` v0.64.0 via `tokio::process::Command` with `stdin` payload + `-p` directive, `--output-format json --approval-mode default --skip-trust -e none --allowed-mcp-server-names ... --policy stage4-readonly.toml`, and env `GEMINI_CLI_TRUST_WORKSPACE=true`, `GEMINI_CLI_NO_RELAUNCH=true`, `NO_BROWSER=true`; parse `stdout` on exit `0` and `stderr` on fatal exit codes.

## Frontier (Unblocked)

- [Cargo Workspace Monorepo and Path Crate Dependency Architecture](tickets/03-cargo-workspace-monorepo-layout.md) (`open`, `prototype`, `HITL`)
- [Alexandria Daemon RCA Extensions: Vault Loop Filter, 5-Min Debounce, and Package Registry](tickets/04-alexandria-rca-daemon-extensions.md) (`open`, `prototype`, `HITL`)

## Blocked

- [Local-Host Conductor 6-Stage Pipeline and SQLite Durable State Machine](tickets/05-localhost-rca-pipeline-orchestrator.md) (`open`, `grilling`, `HITL`) — blocked by [Cargo Workspace Monorepo and Path Crate Dependency Architecture](tickets/03-cargo-workspace-monorepo-layout.md)
- [End-to-End 4-Project PoC Deployment and Seam Verification](tickets/06-four-project-poc-deployment.md) (`open`, `task`, `HITL`) — blocked by [Cargo Workspace Monorepo and Path Crate Dependency Architecture](tickets/03-cargo-workspace-monorepo-layout.md), [Alexandria Daemon RCA Extensions: Vault Loop Filter, 5-Min Debounce, and Package Registry](tickets/04-alexandria-rca-daemon-extensions.md), [Local-Host Conductor 6-Stage Pipeline and SQLite Durable State Machine](tickets/05-localhost-rca-pipeline-orchestrator.md)
