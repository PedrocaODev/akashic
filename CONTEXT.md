# OEM Android Knowledge & RCA Infrastructure

The authoritative knowledge infrastructure and root cause analysis system for Motorola OEM Android applications, integrating Akashic knowledge graph vaults with MotoJira, Gerrit, and codebase embeddings.

## Language

**Vault**:
The version-controlled Git repository containing typed Akashic Markdown documents and their validated dependency graph.
_Avoid_: Knowledge base, document store, wiki

**Incident**:
An authoritative Akashic node representing a concrete production or testing failure event, containing verified symptoms, impacted build/device scope, and timeline.
_Avoid_: Bug, ticket, issue, defect, glitch

**Root Cause**:
The verified code defect, architectural flaw, or environmental condition directly responsible for an Incident.
_Avoid_: Reason, fault, problem, trigger

**Evidence**:
An immutable, empirical artifact (such as a stack trace, logcat excerpt, tombstone dump, or Gerrit commit diff) directly substantiating a causal claim or hypothesis.
_Avoid_: Proof, attachment, data, snippet

**Hypothesis**:
A structured candidate explanation linking an Incident to a potential Root Cause, pending verification against Evidence.
_Avoid_: Guess, theory, conjecture, idea

**Investigation**:
The automated or engineer-guided workflow that gathers Evidence, tests Hypotheses, and deduces the Root Cause for an Incident.
_Avoid_: Debugging session, troubleshooting, analysis run

**Investigation Trigger**:
The external issue key (such as a MotoJira issue key or Bug2Go identifier) that initiates an Investigation.
_Avoid_: Incident ticket, bug report, incoming alert

**Crystallization**:
The transition of ephemeral investigation findings and evidence into immutable, linted nodes in the Vault.
_Avoid_: Ingestion, saving, dumping

**Supersession**:
The explicit replacement of an obsolete or reverted node by an authoritative successor declaring a forward `supersedes` edge within the Vault, preserving historical lineage without discarding knowledge or mutating prior nodes.
_Avoid_: Deletion, removal, overwrite

**Gerrit Event Trigger**:
The event payload emitted by Gerrit (such as `change-merged`) signaling the knowledge graph to ingest, link, or supersede nodes in the Vault.
_Avoid_: Git hook, commit signal, push notification

**Symbolication**:
The deterministic mapping of native crash hex addresses and tombstone frames to source files, line numbers, and function symbols using OEM build artifacts.
_Avoid_: Decoding, address resolving, unstripping

**Code Graph**:
The structural dependency graph of source code (AST symbols, files, relationships, and embeddings) extracted from an OEM Android codebase.
_Avoid_: Vault, knowledge base, repository index

**Data Sanitization**:
The redaction and scrubbing of proprietary tokens, OEM signing keys, credentials, and PII from source code chunks prior to vectorization or graph storage.
_Avoid_: Graph cleaning, filtering, masking

**Graph Reconciliation**:
The differential maintenance of a Code Graph, pruning deleted symbols, updating modified edges, and preserving structural consistency across Git commits.
_Avoid_: Sanitization, clean up, rebuild, refresh

**Resolution Node**:
An authoritative Akashic node (`RES-<JIRA_KEY>-<SEQ>`) built on OKF v0.2 documenting a verified code fix, workaround, architectural redesign, or revert addressing one or more Incidents, including superseded attempts and negative knowledge via forward `supersedes` relations.
_Avoid_: Patch ticket, fix note, solution document, PR summary

**Remediation Type**:
The categorical classification of a Resolution (`workaround`, `hotfix`, `permanent_fix`, or `revert`) specifying its lifecycle intent and durability.
_Avoid_: Fix category, patch type, solution status

**Negative Knowledge**:
Documented causal knowledge detailing failed fix attempts, invalidated hypotheses, abandoned workarounds, and regressions introduced by prior solutions, preventing repeated engineering mistakes.
_Avoid_: Failed experiments, rejected PRs, debugging dead-ends

**Format-Preserving Semantic Pseudonymization**:
A deterministic, grammar-aware token replacement process that redacts sensitive PII while substituting typed syntactic equivalents (such as `<PHONE:US_VALID_10DIGIT>`) to preserve log formatting, structural parsing, and anomaly semantics.
_Avoid_: Redaction, data masking, scrubbing, anonymization

**Structural Significance Filter**:
A multi-stage AST analysis pipeline in Alexandria that inspects bare Git diffs to detect architectural boundary, type hierarchy, and public/protected API modifications while filtering out superficial or internal implementation changes.
_Avoid_: Commit filter, git ignore, change detector, code linter

**Crash Signature**:
A deterministic SHA-256 hash calculated as `sha256(app_id + exception_or_signal + top_3_symbolicated_app_frames)` used to identify identical crash instances across diverse devices, soak benches, and nightly builds.
_Avoid_: Crash ID, bug hash, exception fingerprint

**Soak Storm Deduplication Gate**:
An automated ingestion filter in Conductor that prevents redundant LLM reasoning and duplicate `INC-` nodes during fleet crash storms by appending incoming tickets as `EVD-` nodes to an active `Incident` with matching `Crash Signature`.
_Avoid_: De-duper, storm blocker, rate limiter

**Symbol Store LRU Cache**:
A local NVMe-backed high-speed caching layer within the `android_symbolicator_worker` mapping `ro.build.fingerprint` to unstripped ELF binaries and R8 Proguard mapping files to eliminate symbol server round-trips.
_Avoid_: Symbol buffer, cache folder, binary store

**Package-to-Repository Index**:
The auto-discovered mapping table in Alexandria connecting Android package identifiers (`applicationId`, `namespace`, and manifest `package` attributes) to local Git worktree paths, Gerrit project names, and partitioned Vault directories (`akashic/<app_id>/`).
_Avoid_: Project list, hardcoded config, repo map, package lookup

**Clone-to-Onboard Discovery**:
The zero-configuration onboarding mechanism where cloning any Motorola Android repository into the workspace root automatically registers its Gerrit remote, extracts its Android package identifiers across multi-module Gradle/Manifest files, and provisions its SQLite Code Graph and Vault partition.
_Avoid_: Manual registration, static project onboarding, config-file enrollment

**Local-Host Conductor**:
The lightweight, host-native RCA workflow engine written in Rust that executes the 6-stage investigation pipeline on the developer workstation using SQLite state checkpointing and the authenticated `gemini` CLI session, avoiding containerized credential exposure or paid API keys.
_Avoid_: Cloud Conductor, Orkes server, containerized orchestrator

## Open Knowledge Format (OKF) & Node Architecture

The Vault implements the Google Open Knowledge Format (OKF) v0.2 specification with first-class domain extensions:

### First-Class OKF Extensions
`type: Resolution` and `type: Architecture` are native, first-class node types extending the OKF v0.2 schema rather than generic or untyped documents.

### Forward-Only Relations Rule
All graph edges adhere strictly to forward-only navigation semantics to preserve node immutability and Git historical integrity:
- **Supersession**: Successor nodes declare `supersedes: <target>`, pointing forward in the investigation timeline. Superseded nodes are never retroactively edited or injected with an inverse `superseded_by` field.
- **Resolution**: Resolution nodes declare `resolves: <target>` pointing to the target Incident node. Incident nodes are never modified with an inverse `resolved_by` property.

### Deterministic Node Identifiers
Node filenames and identifiers adhere to a standardized deterministic naming convention:
- **Incidents**: `INC-<JIRA_KEY>` (e.g., `INC-MOTOCAM-9102.md`)
- **Resolutions**: `RES-<JIRA_KEY>-<SEQ>` (e.g., `RES-MOTOCAM-9102-01.md`, `RES-MOTOCAM-9102-02.md`)
- **Evidence**: `EVD-<JIRA_KEY>-<SRC>` (e.g., `EVD-MOTOCAM-9102-BUG2GO.md`, `EVD-MOTOCAM-9102-SOAK.md`)
- **Architecture**: `arch/<subsystem>.md` (e.g., `arch/camera-client-lifecycle.md`)

### Inter-Note Relations vs External References
Graph edges declared under `relations:` are strictly reserved for inter-note links targeting relative markdown files within the Vault (`./<path>.md` or `../<path>.md`). External entities (such as Gerrit change IDs, repository paths, source code locations, Bug2Go IDs, and Jira keys) must be placed in `external_refs:` (or `gerrit_cls:`) or referenced within the Markdown body, never inside `relations:`.
