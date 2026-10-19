# Akashic

Unified Markdown knowledge graph engine, incremental AST code graph engine, and root cause analysis (RCA) system for autonomous coding agents and OEM Android engineering.

## Language

### Vault Graph

**Vault**:
The version-controlled Git directory containing typed Markdown documents and their validated dependency graph.
_Avoid_: Knowledge base, document store, wiki

**Vault Node**:
A discrete Markdown document in a Vault representing a single concept, decision, architecture note, incident, resolution, runbook, or evidence source.
_Avoid_: Document, file, page, block, code symbol

**Relation**:
A typed, directed forward edge asserted in Vault Node frontmatter connecting a source Vault Node to a target Vault Node with optional evidence and scope.
_Avoid_: Link, backlink, connection, code edge

**Supersession**:
The explicit replacement of an obsolete Vault Node by an authoritative successor declaring a forward `supersedes` Relation within an identical or covering Scope.
_Avoid_: Deprecation, deletion, removal, overwrite

**Current Guidance**:
The authoritative set of active Vault Nodes for a given Scope, where superseded nodes are resolved and redirected to their successors.
_Avoid_: Fresh notes, latest edits

**Historical Context**:
The point-in-time state or lineage tracing how decisions and resolutions evolved and why changes occurred.
_Avoid_: Archive, graveyard

**Scope**:
A set of key-value attributes (such as system, environment, app, or platform) bounding the domain where a Vault Node or Relation applies.
_Avoid_: Tag, category

**Plan**:
A machine-readable, reviewable diff proposing missing metadata, normalized IDs, or inferred Relations with supporting evidence before any Vault file mutation.
_Avoid_: Patch, migration script

**Unresolved Conflict**:
An invalid Vault graph topology (such as a supersession cycle or competing active successors in the same Scope) where the engine abstains from choosing a winner and surfaces an explicit alert.
_Avoid_: Heuristic choice, silent fallback

**Crystallization**:
The transition of ephemeral investigation findings and empirical artifacts into immutable, linted Vault Nodes.
_Avoid_: Ingestion, saving, dumping

**Vault Index**:
The disposable, reconstructible SQLite database (`vault_index.db`) that caches parsed Vault Node metadata, Relations, full-text search indexes, and vector embeddings while Markdown files remain the canonical source of truth.
_Avoid_: Vault database, primary store, master index

### Code Graph

**Code Graph**:
The unified directed multigraph representing source code structures, declarations, semantic relationships, and vector embeddings for a repository.
_Avoid_: AST graph, knowledge graph, vault, repository index

**Code Symbol**:
A discrete source code declaration (class, interface, function, method, constructor, or field) in the Code Graph identified by an arity- and parameter-aware unique ID.
_Avoid_: Vault node, vertex, entity

**Code Edge**:
A directed structural relationship between two Code Symbols in the Code Graph, representing call invocations, type references, or inheritance.
_Avoid_: Relation, link, connection, backlink

**Projection**:
An atomic, serialized JSON representation of a repository's Code Graph at a specific commit SHA.
_Avoid_: Dump, snapshot export, cache file

**Blast Radius**:
The transitive set of upstream callers, downstream dependencies, and graph communities impacted by a change to a Code Symbol or patchset.
_Avoid_: Impact tree, dependency cascade, footprint

**Graph Reconciliation**:
The differential maintenance of a Code Graph across Git commits, pruning deleted Code Symbols, updating modified Code Edges, and preserving structural consistency.
_Avoid_: Sanitization, clean up, rebuild, refresh

**Non-Destructive Ingestion**:
The extraction of file content directly from the Git object database into memory buffers without altering developer working directories or checkout state.
_Avoid_: Worktree checkout, repo clone, dirty checkout

**Structural Significance Filter**:
The multi-stage AST analysis pipeline that inspects bare Git diffs to detect architectural boundary, type hierarchy, and public/protected API modifications while filtering out internal implementation changes.
_Avoid_: Commit filter, git ignore, change detector, code linter

**Package-to-Repository Index**:
The auto-discovered mapping connecting Android package identifiers (`applicationId`, `namespace`, and manifest `package` attributes) to local Git worktree paths, Gerrit project names, and partitioned Vault directories.
_Avoid_: Project list, hardcoded config, repo map, package lookup

**Clone-to-Onboard Discovery**:
The zero-configuration onboarding mechanism where cloning an Android repository into the workspace root automatically registers its Gerrit remote, extracts its package identifiers, and provisions its Code Graph and Vault partition.
_Avoid_: Manual registration, static project onboarding, config-file enrollment

**Edge Provenance**:
The reliability classification of a Code Edge (`EXTRACTED` from deterministic AST parsing, `INFERRED` from embedding similarity or heuristic resolution, or `AMBIGUOUS`).
_Avoid_: Edge type, link weight, confidence tag

**Graph Community**:
A cohesive cluster of densely connected Code Symbols identified by graph community detection algorithms and scored by intra-cluster cohesion.
_Avoid_: Module group, package folder, cluster bucket

**God Node**:
A highest-degree Code Symbol that acts as a central architectural hub bridging multiple Graph Communities.
_Avoid_: Super node, big class, core file

### RCA & Ingestion

**Incident**:
An authoritative Vault Node representing a concrete production or testing failure event, containing verified symptoms, impacted device/build Scope, and timeline.
_Avoid_: Bug, ticket, issue, defect, glitch

**Root Cause**:
The verified code defect, architectural flaw, or environmental condition directly responsible for an Incident.
_Avoid_: Reason, fault, problem, trigger

**Evidence**:
An immutable, empirical artifact (such as a stack trace, logcat excerpt, tombstone dump, or Gerrit commit diff) directly substantiating a causal claim or Hypothesis.
_Avoid_: Proof, attachment, data, snippet

**Hypothesis**:
A structured candidate explanation linking an Incident to a potential Root Cause, pending verification against Evidence.
_Avoid_: Guess, theory, conjecture, idea

**Investigation**:
The automated or engineer-guided workflow that gathers Evidence, tests Hypotheses, and deduces the Root Cause for an Incident.
_Avoid_: Debugging session, troubleshooting, analysis run

**Investigation Trigger**:
The external issue identifier (such as a MotoJira issue key or Bug2Go identifier) that initiates an Investigation.
_Avoid_: Incident ticket, bug report, incoming alert

**Resolution Node**:
An authoritative Vault Node documenting a verified code fix, workaround, architectural redesign, or revert addressing one or more Incidents via a forward `resolves` Relation.
_Avoid_: Patch ticket, fix note, solution document, PR summary

**Remediation Type**:
The categorical classification of a Resolution Node (`workaround`, `hotfix`, `permanent_fix`, or `revert`) specifying its lifecycle intent and durability.
_Avoid_: Fix category, patch type, solution status

**Negative Knowledge**:
Documented causal knowledge detailing failed fix attempts, invalidated Hypotheses, abandoned workarounds, and regressions introduced by prior solutions.
_Avoid_: Failed experiments, rejected PRs, debugging dead-ends

**Gerrit Event Stream**:
The continuous SSH JSON stream emitted by a Gerrit server broadcasting patchset and merge lifecycle events.
_Avoid_: Webhook, notification feed, message queue

**Gerrit Event Trigger**:
The event payload emitted by the Gerrit Event Stream (such as `change-merged`) signaling the system to reconcile the Code Graph or supersede Vault Nodes.
_Avoid_: Git hook, commit signal, push notification

**Symbolication**:
The deterministic mapping of native crash hex addresses and obfuscated stack frames to source files, line numbers, and Code Symbols using build artifacts.
_Avoid_: Decoding, address resolving, unstripping

**Crash Signature**:
A deterministic cryptographic hash computed from the application identifier, exception or signal type, and top symbolicated application frames to identify identical crash instances across builds.
_Avoid_: Crash ID, bug hash, exception fingerprint

**Soak Storm Deduplication Gate**:
An ingestion filter that prevents redundant Investigation runs and duplicate Incident nodes during fleet crash storms by attaching incoming triggers as Evidence to an active Incident with a matching Crash Signature.
_Avoid_: De-duper, storm blocker, rate limiter

**Symbol Store LRU Cache**:
A local caching layer mapping build fingerprints to unstripped ELF binaries and R8/ProGuard mapping files for Symbolication.
_Avoid_: Symbol buffer, cache folder, binary store

**Local-Host Conductor**:
The host-native RCA workflow state machine that executes the multi-stage Investigation pipeline using durable SQLite state checkpointing.
_Avoid_: Cloud Conductor, Orkes server, containerized orchestrator

### Sanitization

**Data Sanitization**:
The redaction and scrubbing of proprietary tokens, signing keys, credentials, and PII from source code chunks prior to vectorization or Code Graph storage while retaining syntax structure.
_Avoid_: Graph cleaning, filtering, masking

**Format-Preserving Semantic Pseudonymization**:
A deterministic, grammar-aware token replacement process that redacts sensitive PII and secrets in logs while substituting typed syntactic equivalents (such as `<PHONE:US_VALID_10DIGIT>`) to preserve log formatting and anomaly semantics.
_Avoid_: Generic redaction, data masking, anonymization
