---
id: "05-localhost-rca-pipeline-orchestrator"
title: "Local-Host Conductor 6-Stage Pipeline and SQLite Durable State Machine"
label: "wayfinder:grilling"
type: "grilling"
mode: "HITL"
parent: "wayfinder-map"
blocked_by: []
assignee: "pedrogoncalves"
status: open
---

# Local-Host Conductor 6-Stage Pipeline and SQLite Durable State Machine

## Question

How should the Rust **Local-Host Conductor** (`rca-conductor`) orchestrate the 6-stage RCA pipeline (`Stage 1: Triage & Storm Gate`, `Stage 2: Log Sanitization`, `Stage 3: Native & Java Symbolication`, `Stage 4: Hypothesis Generation & Adversarial Falsification via gemini-cli`, `Stage 5: Akashic Crystallization & Lint Gate`, `Stage 6: Resolution Tracking & Supersession`) on the developer workstation using SQLite WAL durable state checkpointing, replacing the heavy Dockerized Orkes Conductor (Java/Postgres/Redis) stack while preserving deterministic retries, Crash Signature deduplication (`Soak Storm Deduplication Gate`), and local credential isolation?
