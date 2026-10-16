# 0005: Conductor RCA Orchestration and Two-Stage Reasoning

## Context

Root Cause analysis for Motorola OEM Android soak crashes and Bug2Go diagnostic packages (Investigation Triggers) requires coordinating heterogeneous systems across disparate domains: extracting diagnostic dumps and tombstones from MotoJira, symbolicating native tombstones and Java stack traces against OEM build targets, querying incremental Code Graphs in Alexandria, cross-referencing Gerrit change histories, and deducing causal mechanisms.

Monolithic agent loops and ad-hoc orchestration scripts fail under real-world production conditions due to unhandled worker timeouts, rate limits, non-deterministic LLM hallucination, and spurious causal correlations where an LLM blames innocent commits simply because they modified adjacent lines or shared source files. Furthermore, investigations must maintain fault tolerance, ensuring progress is never lost when long-running tasks or external network dependencies experience transient failures.

## Decision

We adopt **Netflix/Orkes Conductor** as the authoritative durable workflow orchestration engine for automated Root Cause analysis, structuring the Investigation lifecycle into discrete, stateful, and observable tasks governed by four core architectural pillars:

1. **Durable Workflow Orchestration (Netflix/Orkes Conductor)**:
   Investigation workflows are declared as version-controlled JSON Directed Acyclic Graphs (DAGs) executed by Conductor. Conductor guarantees state persistence across worker failures, coordinates fork-join parallel context harvesting, enforces deterministic timeouts, manages exponential backoff retries for external services, and provides end-to-end execution auditability.

2. **Polyglot Worker Runtime Split**:
   In alignment with ADR 0002, execution responsibility is strictly partitioned by runtime capability:
   - **Rust Workers**: Handle CPU-intensive, deterministic, and graph-critical operations: `android_symbolicator_worker` (addr2line, Proguard/R8 deobfuscation, stack parsing) and `akashic_crystallize_worker` (Vault note formatting and `akashic lint` validation). Rust guarantees memory safety, sub-second execution, and zero panics when processing multi-megabyte tombstone logs.
   - **Python Workers**: Handle distributed Conductor worker polling, external service integration (MotoJira REST, Gerrit REST/SSH, Alexandria MCP), LiteLLM gateway client routing, and multi-agent reasoning execution (`jira_bug2go_ingest_worker`, `alexandria_code_graph_worker`, `gerrit_history_worker`, `akashic_lineage_worker`, `rca_hypothesis_worker`, `rca_falsifier_worker`, and `delivery_notification_worker`).

3. **Two-Stage Hypothesis Generation and Adversarial Falsification**:
   To eradicate spurious correlations and LLM hallucinations, reasoning is decoupled into two sequential, adversarial phases:
   - **Stage 1 (Hypothesis Generation)**: Generates 1 to 3 candidate Hypotheses linking the Incident to a potential Root Cause, synthesizing symbolicated stack traces, Alexandria Code Graph call chains, Gerrit commit diffs, and historical Vault lineage.
   - **Stage 2 (Adversarial Falsification)**: An adversarial falsifier worker stress-tests each candidate Hypothesis against empirical Evidence (tombstone CPU registers, logcat event sequences, AST reachability, and memory state). Hypotheses that contradict empirical Evidence are rejected with explicit falsification rationales; only a verified Hypothesis surviving rigorous stress-testing is selected as the authoritative Root Cause.

4. **On-Demand Crystallization via Gerrit Review (`refs/for/main`)**:
   In alignment with ADR 0001, verified findings do not directly mutate production Vault branches. Verified Root Causes and empirical Evidence undergo on-demand Crystallization into canonical Markdown nodes (`Incident` and `Evidence` with `caused_by` and `supported_by` relations), pass strict `akashic lint` graph topology checks, and are pushed to Gerrit (`refs/for/main`) as reviewable changes for engineering sign-off, automatically cross-linking the MotoJira ticket.

## Consequences

### Positive
- **Durability & Fault Tolerance**: Interrupted Investigations resume from their last successful checkpoint rather than restarting expensive Symbolication or context harvesting.
- **Elimination of Spurious Root Causes**: Adversarial falsification prevents incorrect engineer blame and ensures high trust in automated RCA findings.
- **Strict Vault Integrity**: Enforcing `akashic lint` and Gerrit code review before Crystallization ensures the Vault remains free from phantom nodes, broken links, or speculative conclusions.
- **Deterministic High-Throughput Parsing**: Offloading Symbolication and linting to native Rust workers eliminates Python interpreter bottlenecks.

### Negative & Operational Considerations
- **Orchestration Infrastructure**: Requires deploying and maintaining a Conductor server cluster and backing persistence store (PostgreSQL/Redis).
- **Polyglot Deployment**: Requires packaging and maintaining both native Rust binaries and Python virtual environment runtimes across worker pools.
