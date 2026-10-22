# Akashic Proof of Value: 30-Minute Script, Interactive Console Guide, and Principal Engineer Q&A Battlecard

## 1. Meeting Parameters and Strategy

- **Audience**: Platform Principal Engineer and Android Principal Engineer.
- **Total Duration**: 30 minutes (18 minutes live deck walkthrough with interactive terminal console, 12 minutes Q&A).
- **Positioning**: Proof of Value (PoV), not Proof of Concept (PoC). Resolves the context retrieval bottleneck for autonomous AI workflows: 3-Axis Code Review (Standards, Spec, Complexity) and Automated Root Cause Analysis (RCA).
- **Grounding Contract**: Clearly state the two tiers of data provenance:
  - **Multi-Repo Code Graph (100% Verified Production Facts)**: Extracted live across four Motorola OEM Android repositories (`AudioRecorder`, `FMRadio`, `MotoAssistPremium`, `motoDolbyUI`) via deterministic Tree-sitter AST parsing. Yields 810 package mappings, 20,231 symbols, and 143,260 edges with zero LLM indexing calls.
  - **Camera3 RCA and Supersession DAG (Synthetic Reference Architecture / PoC Simulation)**: Located in `prototype-akashic-rca-vault`. An illustrative test harness to validate Open Knowledge Format (OKF) v0.2 L001-L011 schema lints and forward supersession DAG resolution before production repository vaults are bootstrapped.
- **Core Operational Contract**:
  - **Day-0 Value (Zero Team Setup)**: Akashic maps the as-is architecture across multiple OEM Android repositories out of the box. Powers the **Complexity and Blast Radius** axis of Code Review and code-level RCA (symbolication, soak storm deduplication gate, call-graph traversal).
  - **Day-1 Organizational Prerequisite (Team Curation)**: To unlock the **Spec** and **Standards** axes of Code Review, and **Current Guidance / Negative Knowledge** in RCA, domain teams curate and commit authoritative Specs, Business Rules, Coding Standards, and Architecture Decision Records (ADRs) into their repository Vault. Confluence and MotoJira MCPs assist in drafting candidate documents via `akashic vault plan`, but domain teams own and verify what enters the repository Vault.
- **Rollout Model**: Two operational tiers:
  - **Tier 1 (Workstation-Local Toolchain - Evaluated Today)**: Local CLI and native `rmcp` Model Context Protocol (MCP) server running directly on developer and triage workstations with zero centralized server infrastructure. Local SQLite WAL ensures sub-millisecond query latency.
  - **Tier 2 (Centralized Gerrit Stream Automation - Architecture Ready)**: Dedicated build runner running `akashic daemon`, connected to `gerrit.mot.com:29418`. Features a 30-second sliding debounce (300-second maximum cap), watermark catch-up after disconnects, and a Structural Significance Filter to ignore cosmetic changes.

---

## 2. Timed Presentation Script (18 Minutes)

### Slide 1: Executive Summary: From AI Workflows to Context Substrate (00:00 - 02:00)
- **Visual on Deck**: Title slide with the 3-layer architecture stack diagram:
  - `Layer 1: Autonomous AI Workflows (3-Axis Code Review & Automated RCA)`
  - `Layer 2: Akashic Context & Governance Substrate (Rust CLI + rmcp stdio MCP)`
  - `Layer 3: OEM Engineering Sources (Bare Git .git/objects + OKF v0.2 Vault + Gerrit 29418)`
- **Presenter Cue**: Set an objective engineering tone. Frame Akashic as the missing context substrate for Google enterprise AI (Gemini Code Assist).
- **Spoken Track**:
  > "Good morning. We were tasked with deploying two high-leverage AI workflows: 3-Axis Code Review and Automated Root Cause Analysis.
  > While building and testing those workflows against Motorola OEM Android repositories, we identified a fundamental bottleneck: the model and the prompt are not the constraint. Context retrieval is the constraint.
  > For Code Review, an autonomous agent requires three distinct axes: **Standards**, **Spec**, and **Complexity**. For RCA, an agent requires stack trace symbolication, call-graph blast radius, and historical context regarding prior incidents and failed workarounds.
  > Off-the-shelf retrieval tools fail at OEM scale for two specific reasons:
  > First, LLM-based indexers (such as Python `graphify`) invoke cloud models during indexing. On repositories with millions of lines of code, this burns tokens, runs slowly, and generates non-deterministic graphs across runs.
  > Second, querying raw Confluence or MotoJira MCPs at runtime retrieves outdated 2023 design pages alongside 2025 specs with identical cosine similarity. Because raw wikis lack typed scope and temporal supersession links, the LLM blends obsolete rules with current rules and hallucinates review findings or broken fixes.
  > We built **Akashic** to solve this. Akashic is a single, host-native Rust binary and MCP server that integrates a deterministic **Code Graph**, a governed Markdown **Vault**, and a checkpointed **Local-Host RCA Conductor**."

### Slide 2: Architecture Overview: 8-Crate DAG & Physical SQLite Isolation (02:00 - 04:30)
- **Visual on Deck**: Strict 4-tier 8-crate workspace DAG diagram (`akashic-core`, `akashic-daemon` -> `akashic-engine` -> `akashic-vault`, `akashic-rca`, `akashic-lint` -> `akashic-embedding`, `akashic-sanitize`) and the 3 physically isolated SQLite databases (`code_graph.db`, `vault_index.db`, `rca_state.db`).
- **Presenter Cue**: Highlight physical database separation and compile-time dependency enforcement for platform security.
- **Spoken Track**:
  > "Architecturally, Akashic organizes all capabilities into an 8-crate Rust workspace with a strict Directed Acyclic Graph and zero circular dependencies ([ADR 0001](file:///home/pedrogoncalves/projects/akashic/docs/adr/0001-Adopt-Eight-Crate-Layered-Workspace-Topology.md)).
  > Notice the persistence architecture at the bottom right ([ADR 0002](file:///home/pedrogoncalves/projects/akashic/docs/adr/0002-Isolate-SQLite-Persistence-Into-Domain-Specific-Databases.md)). We physically separate persistence into three distinct SQLite databases:
  > First, `code_graph.db` stores high-churn AST symbols and edge relationships.
  > Second, `vault_index.db` stores document frontmatter, full-text FTS5 search indexes, and vector embeddings.
  > Both of these databases are completely disposable caches. You can delete them at any time and regenerate them deterministically from Git.
  > Third, `rca_state.db` is durable. It preserves active incident investigations, `CrashSignature` deduplication tables, and Gerrit event watermarks.
  > By isolating these three databases into separate SQLite files in WAL mode, high-frequency AST indexing never causes write-lock contention or starves active RCA investigations.
  > Finally, observe `akashic-sanitize` at Tier 4. It enforces security and privacy at compile time ([ADR 0007](file:///home/pedrogoncalves/projects/akashic/docs/adr/0007-Profile-Driven-Sanitization-And-Format-Preserving-Pseudonymization.md)). `CodeProfile` removes proprietary credentials while preserving valid AST syntax, while `LogProfile` applies Format-Preserving Semantic Pseudonymization to logcat lines so PII is scrubbed before embeddings or prompts are created."

### Slide 3: Act 1 (Day-0 Value): Multi-Project OEM Android Architecture Out-of-the-Box (04:30 - 08:00)
- **Visual on Deck**: 4 metric cards (20,231 Symbols, 810 Package Mappings, ~140k Edges, 0 LLM Indexing Calls), OEM application metrics table (`AudioRecorder`, `MotoAssistPremium`, `motoDolbyUI`, `FMRadio`), and the 5-Pass Clone-to-Onboard Discovery workflow.
- **Presenter Cue**: Contrast Day-0 zero-setup discovery with Day-1 team curation. Emphasize that all numbers shown are verified production facts.
- **Spoken Track**:
  > "Let us distinguish what Akashic delivers on **Day 0 with zero team setup** from what requires **Day 1 team curation**.
  > On Day 0, Akashic maps the as-is architecture across our OEM Android applications out of the box. We indexed four Motorola applications: **AudioRecorder**, **MotoAssistPremium**, **motoDolbyUI**, and **FMRadio**.
  > Our `Clone-to-Onboard Discovery` algorithm executes a deterministic 5-pass parser across `settings.gradle`, module `build.gradle` files, and `AndroidManifest.xml` ([ADR 0009](file:///home/pedrogoncalves/projects/akashic/docs/adr/0009-Gerrit-Stream-Adaptive-Debounce-Watermark-Catchup-And-Auto-Discovery.md), [01-android-package-autodiscovery.md](file:///home/pedrogoncalves/projects/akashic/docs/wayfinder/research/01-android-package-autodiscovery.md)). It maps `applicationId`, `namespace`, and manifest package declarations directly to local repositories, Gerrit project branches, and Vault partitions.
  > Across these four applications, Akashic automatically mapped **810 package declarations**, extracted **20,231 code symbols**, and identified **143,260 edges** (including 91,077 extracted AST calls and inheritance links, plus 52,183 inferred similarity edges), requiring **zero LLM indexing calls** ([ADR 0005](file:///home/pedrogoncalves/projects/akashic/docs/adr/0005-Use-Deterministic-AST-And-Embedding-Sidecar-Instead-Of-LLM-Indexing.md)).
  > Ingestion uses `gix` to read Git blobs directly from `.git/objects` into memory ([ADR 0006](file:///home/pedrogoncalves/projects/akashic/docs/adr/0006-Non-Destructive-Git-Ingestion-And-Structural-Significance-Filtering.md)). It never touches the developer's working tree, never alters file timestamps, and never invalidates Gradle incremental build caches.
  > Every edge records explicit **Edge Provenance**: `EXTRACTED` for deterministic AST calls and inheritance, `INFERRED` for embedding similarity above 0.82, and `AMBIGUOUS` for dynamic resolution.
  > Out of the box, this immediately powers the **Complexity** pre-review axis and transitive blast radius analysis, as well as native/Java crash symbolication and soak storm deduplication for RCA."

### Slide 4: Two-Minute Interactive Terminal Walkthrough Console (08:00 - 10:30)
- **Visual on Deck**: Interactive terminal walkthrough console with 3 clickable chapter tabs and real captured CLI execution data:
  - Tab 1: `Chapter 1: Multi-Repo OEM Discovery & Scale [PRODUCTION FACT // LIVE AST]` (hotkey `1`)
  - Tab 2: `Chapter 2: Pre-Review Blast Radius & Provenance [PRODUCTION FACT // LIVE AST]` (hotkey `2`)
  - Tab 3: `Chapter 3: Camera3 RCA & Supersession [PoC SIMULATION // SYNTHETIC SCENARIO]` (hotkey `3`)
- **Presenter Cue**: Use keyboard hotkeys `1`, `2`, and `3` to switch chapters interactively on Slide 4. Point to the data provenance badges.
- **Spoken Track**:
  > *(Press hotkey `1`)*
  > "Here is our live terminal walkthrough console, captured directly from our CLI execution harness ([`record-demo.sh`](file:///home/pedrogoncalves/projects/akashic/docs/presentation/record-demo.sh)).
  > In **Chapter 1**, you see multi-repo discovery running against live production ASTs across `AudioRecorder`, `FMRadio`, `MotoAssistPremium`, and `motoDolbyUI`.
  > Akashic processes 810 packages and 20,231 symbols in sub-second time. Running `akashic graph god-nodes` immediately identifies central hub symbols such as `logD` (with 2,160 incoming call edges), `run`, and `TranscribeAudioAICore`. These are 100% verified production facts extracted directly from source trees.
  >
  > *(Press hotkey `2`)*
  > In **Chapter 2**, we evaluate `MainFragment.kt::removeUIObservers()` in `AudioRecorder` before a code change.
  > Akashic computes the transitive blast radius in milliseconds: 46 upstream callers and 24 downstream dependencies.
  > Notice the explicit edge provenance breakdown: AST calls to `logD`, `requireActivity`, and `removeRecordingObservers` carry a confidence score of 1.00 (`EXTRACTED`), while semantic similarity edges carry 0.95+ (`INFERRED`). This structural telemetry feeds the Complexity axis of automated code review without human manual tracing.
  >
  > *(Press hotkey `3`)*
  > In **Chapter 3**, we examine the governance pipeline.
  > To validate our schema rules prior to team rollout, we run a synthetic reference simulation (`prototype-akashic-rca-vault`).
  > Running `akashic lint` on `com.motorola.camera3` strictly verifies Open Knowledge Format (OKF) rules L001 through L011: `valid = true, issues = 0`.
  > When we query `decision-buffer-release-optimization` in `current` mode, Akashic detects that it has been superseded by `decision-revert-cl-489201`. It redirects the agent to the active revert decision (CL 490102) and supplies the incident evidence. This prevents coding agents from reviving deprecated logic."

### Slide 5: Act 2 (The Intent Gap): Why AI Code Review and RCA Plateau Without Curated Intent (10:30 - 13:30)
- **Visual on Deck**: 3-Axis Code Review and RCA comparison matrix comparing:
  - `Day-0: Code Graph Only`
  - `Raw Confluence / MotoJira MCP`
  - `Day-1: Curated Vault + Graph`
  - Highlighted callout: Bootstrap & Ownership Contract.
- **Presenter Cue**: Explain why structural analysis alone cannot verify engineering intent. Address why raw Confluence/Jira search fails.
- **Spoken Track**:
  > "This brings us to the core technical finding of our evaluation: **code structure alone reveals what code is, but it cannot determine what code was intended to do.**
  > Consider our 3-Axis Code Review workflow:
  > - Axis 3 (**Complexity & Blast Radius**) functions completely on Day 0 from `code_graph.db`.
  > - Axis 1 (**Coding Standards**) falls back to generic stylistic checks if the repository does not specify its local conventions.
  > - Axis 2 (**Spec Compliance**) must skip entirely if there is no authoritative specification to evaluate the diff against.
  > A natural question arises: *'We have Confluence and MotoJira MCPs. Why not allow the agent to query Confluence and Jira directly during review or RCA?'*
  > Because raw Confluence pages and Jira tickets are uncurated text. A vector search over Confluence returns an obsolete 2023 architecture proposal alongside a 2025 production specification with similar cosine scores. Because neither document has a typed scope or supersession link, the agent blends contradictory rules and hallucinates.
  > We can use `confluence-mcp` and `motojira-mcp` to draft candidate specifications via `akashic vault plan`, **but domain engineering teams must verify, curate, and commit their authoritative Specs, Standards, and ADRs into their repository Vault.**"

### Slide 6: Act 3 (Day-1 Full Power): Governed Vault & Lineage (13:30 - 16:00)
- **Visual on Deck**: Prominent amber badge `SYNTHETIC REFERENCE ARCHITECTURE // PoC SCENARIO`, the Camera3 RCA DAG diagram (`incident-motocam-9102`, `evidence-motocam-stack`, `decision-buffer-opt`, `decision-revert-cl-489201`), OKF frontmatter YAML snippet, and the Data Provenance Notice callout box.
- **Presenter Cue**: Reiterate that the Camera3 scenario is a synthetic fixture designed to stress-test OKF v0.2. Detail the 4 governance pillars.
- **Spoken Track**:
  > "Please note the data provenance notice on this slide: the `com.motorola.camera3` incident, crash stack, and Gerrit CLs (489201 / 490102) represent a synthetic test fixture authored to stress-test OKF v0.2 supersession DAG resolution.
  > Once domain teams commit their initial specifications into a repository vault, Akashic enforces four governance pillars to prevent documentation decay:
  > 1. **Forward-Only Supersession (`Current Guidance`) ([ADR 0003](file:///home/pedrogoncalves/projects/akashic/docs/adr/0003-Enforce-OKF-v02-Schema-With-Custom-Fields-And-Forward-Only-Relations.md))**: We never mutate historical decision records or edit past commits. A new Vault node specifies a forward `supersedes` relation within a typed `Scope`. When an agent queries `Current Guidance`, Akashic automatically traverses the DAG and resolves active guidance.
  > 2. **Strict Conflict Abstention ([ADR 0004](file:///home/pedrogoncalves/projects/akashic/docs/adr/0004-Strict-Conflict-Abstention-And-Plan-Then-Apply-Vault-Mutations.md))**: If two active nodes claim authority over the same scope or form a cycle, Akashic refuses to guess. It halts and returns an explicit `Unresolved Conflict` signal for human review.
  > 3. **Plan-Then-Apply Lint Gate (`L001` - `L011`) ([ADR 0004](file:///home/pedrogoncalves/projects/akashic/docs/adr/0004-Strict-Conflict-Abstention-And-Plan-Then-Apply-Vault-Mutations.md))**: Autonomous agents cannot write unverified Markdown files into the repository. Mutations produce a structured plan that must pass `akashic lint` before `akashic vault apply` executes.
  > 4. **RCA Crystallization & Negative Knowledge ([ADR 0008](file:///home/pedrogoncalves/projects/akashic/docs/adr/0008-Local-Host-RCA-Conductor-And-Soak-Storm-Deduplication.md))**: Every resolved RCA investigation crystallizes into the Vault as a Resolution Node recording both the verified fix and `Negative Knowledge` (disproven hypotheses and rejected workarounds), ensuring neither human engineers nor agents repeat failed attempts."

### Slide 7: Operational Evaluation & Technical Hand-Off (16:00 - 18:00)
- **Visual on Deck**: Technical Validation and Operational Deployment Model. Two operational tiers (Tier 1 Workstation-Local Toolchain vs. Tier 2 Centralized Gerrit Stream Automation) alongside Platform Engineering Findings and Android OEM Engineering Findings.
- **Presenter Cue**: Reframe this closing slide as an objective technical evaluation and engineering hand-off.
- **Spoken Track**:
  > "To conclude our technical evaluation, we summarize our findings across two operational tiers:
  > - **Operational Tier 1 (Workstation-Local Toolchain - Evaluated Today)**: The CLI and `rmcp` MCP server run directly in developer workspaces and triage terminals. Operating locally with embedded SQLite WAL delivers instant query performance, requires **zero centralized server infrastructure**, and keeps human engineers in the loop while domain teams curate initial specs.
  > - **Operational Tier 2 (Centralized Gerrit Stream Automation - Architecture Ready)**: For automated pre-merge gating, `akashic daemon` runs on dedicated build infrastructure connected to `gerrit.mot.com:29418` ([ADR 0009](file:///home/pedrogoncalves/projects/akashic/docs/adr/0009-Gerrit-Stream-Adaptive-Debounce-Watermark-Catchup-And-Auto-Discovery.md)). The daemon includes a 30-second sliding debounce (300-second maximum cap), persistent high-watermarks for stream reconnection, and a `Structural Significance Filter` that ignores private method formatting changes ([ADR 0006](file:///home/pedrogoncalves/projects/akashic/docs/adr/0006-Non-Destructive-Git-Ingestion-And-Structural-Significance-Filtering.md)).
  > **Key Engineering Findings**:
  > - **Platform Engineering**: Akashic builds as a single static Rust binary with zero external runtime dependencies (free of JVM or Python runtimes). Its 3-DB physical WAL architecture prevents write lock starvation during concurrent agent reads, and Tier-4 sanitization enforces PII redaction at compile time.
  > - **Android OEM Engineering**: The 5-pass discovery algorithm accurately indexes complex multi-module Android projects. Reading bare Git objects via `gix` guarantees that background code indexing never invalidates local Gradle build caches.
  > We will now move into Q&A. We have detailed technical battlecards and reference slides prepared across platform architecture, sanitization, Android build safety, and hallucination defense."

---

## 3. Spoken Tracks and Presenter Cues for Appendix Slides (8 - 11)

These slides are linked directly to specific Principal Engineer Q&A topics. Use keyboard hotkeys to navigate to them instantly during the discussion.

### Slide 8 (Appendix A - Hotkey `A` / `8`): Single Consolidated Rust Engine and Native Agent MCP Server
- **Visual on Deck**: Evolution table (Early Research Prototype vs. Consolidated Rust Engine) and Agent MCP Architecture box (`akashic_graph_query`, `akashic_blast_radius`, `akashic_vault_query`, `akashic_rca_symbolicate`). Note addressing prototype lineage.
- **Presenter Cue**: Use when asked about prototype history, consolidation, or how coding agents interact with Akashic.
- **Spoken Track**:
  > *(Press hotkey `A`)*
  > "During our early research into 3-axis code review and RCA, we experimented with exploratory scripts and prototypes (previously named `alexandria` and `akashic-records`).
  > While those experiments validated the initial concepts, they suffered from token burn, slow indexing, and fragmented interfaces.
  > We consolidated those learnings into a single production architecture: `akashic` ([ADR 0010](file:///home/pedrogoncalves/projects/akashic/docs/adr/0010-Clean-Slate-Domain-Subcommand-CLI-And-Unified-MCP-Server.md)).
  > For coding agents (such as Gemini Code Assist), Akashic provides a native Model Context Protocol (MCP) server over `stdio` using JSON-RPC 2.0 via `rmcp`. It exposes four targeted tools:
  > - `akashic_graph_query`: instant cross-repo symbol lookup.
  > - `akashic_blast_radius`: pre-change caller and callee closure computation.
  > - `akashic_vault_query`: retrieval of active, conflict-free domain rules in `current` mode.
  > - `akashic_rca_symbolicate`: local native and Java crash symbolication with SHA-256 signature deduplication.
  > This provides coding agents with deterministic facts without web searches or ungrounded heuristics."

### Slide 9 (Appendix B - Hotkey `9`): Mandatory Tier-4 Sanitization (Q4 Battlecard)
- **Visual on Deck**: Tier-4 security constraint callout box, `CodeProfile` diff example (secret scrubbing), and `LogProfile` diff example (semantic pseudonymization).
- **Presenter Cue**: Use when the Platform Principal Engineer asks about credential leaks, Bug2Go privacy, or PII handling.
- **Spoken Track**:
  > *(Press hotkey `9`)*
  > "To ensure enterprise compliance, `akashic-sanitize` resides at Tier 4 of our workspace hierarchy ([ADR 0007](file:///home/pedrogoncalves/projects/akashic/docs/adr/0007-Profile-Driven-Sanitization-And-Format-Preserving-Pseudonymization.md)).
  > Because Rust enforces dependency direction at compile time, no outbound text can reach an embedding model or LLM without traversing `akashic-sanitize`.
  > It applies two profiles:
  > First, `CodeProfile` replaces credentials and API keys with structured tokens like `SAN_STR_TOKEN_A8F1`, preserving valid AST syntax so Tree-sitter can still parse the file.
  > Second, `LogProfile` applies Format-Preserving Semantic Pseudonymization to Bug2Go logcat dumps. It replaces phone numbers, IMEIs, and MAC addresses with consistent pseudo-identifiers using a per-session cryptographic salt. The agent can track state across multiple log lines while zero actual PII leaves the environment."

### Slide 10 (Appendix C - Hotkey `0`): Android PE Battlecard (Q5, Q6 & Q7)
- **Visual on Deck**: Left column covering Q5 (Module Discovery) and Q6 (Gradle Cache Safety); right column detailing the 6-stage Pre-LLM Deduplication Pipeline (Q7).
- **Presenter Cue**: Use when the Android Principal Engineer asks about Gradle flavor mappings, build cache invalidation, or soak crash storms.
- **Spoken Track**:
  > *(Press hotkey `0`)*
  > "For Android platform engineering, Slide 10 addresses build integration and soak testing:
  > Regarding Gradle safety (Q6), `gix` reads objects directly from `.git/objects` in memory ([ADR 0006](file:///home/pedrogoncalves/projects/akashic/docs/adr/0006-Non-Destructive-Git-Ingestion-And-Structural-Significance-Filtering.md)). It never alters file modification timestamps in the working tree, which completely protects Gradle's incremental build cache.
  > Regarding soak testing (Q7), a regression during monkey testing can trigger hundreds of duplicate Bug2Go logs.
  > Akashic prevents redundant LLM invocations through its pre-LLM deduplication gate ([ADR 0008](file:///home/pedrogoncalves/projects/akashic/docs/adr/0008-Local-Host-RCA-Conductor-And-Soak-Storm-Deduplication.md)):
  > In Stage 3, it symbolicates native frames using a local 2GB NVMe Symbol Store LRU Cache.
  > In Stage 4, it computes a deterministic `CrashSignature = sha256(app_id + exception_or_signal + top_3_symbolicated_app_frames)`.
  > If that signature matches an existing incident in `rca_state.db` or the Vault, Akashic increments the occurrence counter, attaches the log as supporting evidence, and halts immediately with zero LLM calls."

### Slide 11 (Appendix D - Hotkey `-`): Hallucination Defense & Curation Flywheel (Q2 & Q8)
- **Visual on Deck**: 4-Layer Hallucination Defense (Edge Provenance, Temporal Supersession, Strict Conflict Abstention, Adversarial Falsification) and the 3-Step Pragmatic Curation Flywheel.
- **Presenter Cue**: Use when asked why vector RAG is insufficient or how much manual documentation work is required from feature teams.
- **Spoken Track**:
  > *(Press hotkey `-`)*
  > "Slide 11 illustrates our four-layer defense against AI hallucinations:
  > 1. **Explicit Edge Provenance**: Structural facts are extracted via AST; vector embeddings only serve as an inferred ranking signal.
  > 2. **Temporal Supersession**: The Vault DAG ensures agents only receive active guidance in `current` mode.
  > 3. **Strict Conflict Abstention**: When conflicting specifications exist, Akashic halts rather than guessing.
  > 4. **Adversarial Falsification**: During Stage 5 of RCA, a secondary check actively attempts to disprove candidate root causes against symbolicated frames and Git diffs.
  > For team curation, we follow a 3-step flywheel: Day 0 requires zero documentation. In Step 2, Confluence and Jira MCPs draft candidate nodes via `akashic vault plan`. In Step 3, the team signs off and commits authoritative specs, after which resolved RCA postmortems maintain the vault continuously."

---

## 4. Interactive Console & Demo Execution Guide

The presentation deck on Slide 4 includes an interactive terminal console. This console replaces static screenshots and video players with high-fidelity CLI output captured directly from the toolchain.

### Console Navigation Hotkeys
When presenting [`akashic-pov-deck.html`](file:///home/pedrogoncalves/projects/akashic/docs/presentation/akashic-pov-deck.html) in any modern browser:
- Press **`4`** to navigate to Slide 4.
- While on Slide 4, press **`1`**, **`2`**, or **`3`** to switch between console chapters instantly.
- Alternatively, click the chapter buttons in the left column.

### Chapter Breakdown
1. **Chapter 1: Multi-Repo OEM Discovery & Scale (`[PRODUCTION FACT // LIVE AST]`)**
   - **Hotkey**: `1`
   - **Command Run**: `akashic graph discover --summary` and `akashic graph god-nodes --repo-name AudioRecorder --top-n 5`
   - **Data Verified**: 4 OEM repositories (`AudioRecorder`, `FMRadio`, `MotoAssistPremium`, `motoDolbyUI`), 810 package mappings, 20,231 symbols, 143,260 edges (91,077 extracted AST calls/inherits, 52,183 inferred similarity >= 0.82). Central hub symbols (`logD`, `run`, `TranscribeAudioAICore`).
   - **Provenance Note**: 100% verified production facts from live AST extraction.
2. **Chapter 2: Pre-Review Blast Radius & Provenance (`[PRODUCTION FACT // LIVE AST]`)**
   - **Hotkey**: `2`
   - **Command Run**: `akashic graph blast-radius` and `akashic graph edges --symbol "MainFragment.removeUIObservers" --provenance`
   - **Data Verified**: Transitive closure showing 46 upstream callers, 24 downstream dependencies. Explicit provenance tiers: 1.00 score (`EXTRACTED` AST) vs. 0.95+ score (`INFERRED` semantic similarity). Feeds the Complexity pre-review gate.
   - **Provenance Note**: 100% verified production facts feeding automated review gates.
3. **Chapter 3: Camera3 RCA & Supersession (`[PoC SIMULATION // SYNTHETIC SCENARIO]`)**
   - **Hotkey**: `3`
   - **Command Run**: `akashic lint --vault prototype-akashic-rca-vault/akashic/com.motorola.camera3 --strict` and `akashic vault query --mode current --node-id decision-buffer-release-optimization`
   - **Data Verified**: OKF v0.2 schema validation passes with `valid = true, issues = 0`. Forward supersession query resolves `decision-buffer-release-optimization` as superseded (`is_current: false`) and redirects to `decision-revert-cl-489201` (CL 490102).
   - **Provenance Note**: Synthetic reference scenario authored to validate OKF v0.2 schema lints and forward supersession DAG traversal before production repository vault bootstrap.

### Executing the Live Demo Script
To run or re-record the CLI demonstration live in a terminal:
```bash
cd /home/pedrogoncalves/projects/akashic

# Instant execution mode (zero typing delay, rapid validation)
./docs/presentation/record-demo.sh --fast

# Step-by-step interactive mode (prompts for Enter before each command)
./docs/presentation/record-demo.sh --step

# Automated presentation mode (simulates typing with pauses)
./docs/presentation/record-demo.sh
```

---

## 5. Principal Engineer Q&A Battlecard

### Part A: Platform Principal Engineer

#### Q1: "Why did you build a custom Rust engine (`akashic`) instead of using an existing vector database, Sourcegraph, or Python `graphify`?"
- **Linked Slide**: Slide 8 (Appendix A - Hotkey `A`).
- **Direct Answer**:
  1. **Zero-LLM Indexing Cost & Determinism ([ADR 0005](file:///home/pedrogoncalves/projects/akashic/docs/adr/0005-Use-Deterministic-AST-And-Embedding-Sidecar-Instead-Of-LLM-Indexing.md))**: Tools like Python `graphify` invoke LLMs during indexing to identify entities and edges. Across millions of lines of OEM Android code, LLM indexing takes hours, incurs substantial token costs, and produces inconsistent graphs across runs. Akashic uses native Tree-sitter parsers and `gix` to index deterministically at CPU memory speeds with zero LLM calls.
  2. **Consolidation of Fragmented Toolchains ([ADR 0001](file:///home/pedrogoncalves/projects/akashic/docs/adr/0001-Adopt-Eight-Crate-Layered-Workspace-Topology.md), [ADR 0010](file:///home/pedrogoncalves/projects/akashic/docs/adr/0010-Clean-Slate-Domain-Subcommand-CLI-And-Unified-MCP-Server.md))**: Early research prototypes (`alexandria` for AST indexing, `akashic-records` for vault governance) were fragmented scripts with separate CLIs and duplicate sanitizers. Consolidating them into `akashic` provides a single unified binary and a cohesive Model Context Protocol (`rmcp`) interface (`graph_*`, `vault_*`, `rca_*`).
  3. **Zero Infrastructure Overhead**: Akashic compiles into a single static binary with embedded SQLite (`rusqlite` + `sqlite-vec`). It requires no external database servers, no Docker containers, and no background microservices.

#### Q2: "Why isn't standard Vector RAG over code and Confluence/Jira enough? How does Akashic actually stop hallucinations?"
- **Linked Slide**: Slide 11 (Appendix D - Hotkey `-`) and Slide 5 (Hotkey `5`).
- **Direct Answer**:
  1. **Vector similarity cannot understand call graphs**: Cosine similarity measures textual proximity, not programmatic invocation. Akashic models code as a directed multigraph with explicit Edge Provenance (`EXTRACTED` AST calls vs. `INFERRED` embedding similarity) and blends lexical search with vector similarity via Reciprocal Rank Fusion (`k = 60`).
  2. **Vector search cannot resolve temporal supersession**: When an architectural rule changes, an obsolete 2023 Confluence page and the 2025 replacement page match vector queries with similar similarity scores. The LLM blends both and hallucinates. Akashic's Vault enforces forward `supersedes` relations: `Current Guidance` queries deterministically filter out superseded decisions before the agent sees them ([ADR 0003](file:///home/pedrogoncalves/projects/akashic/docs/adr/0003-Enforce-OKF-v02-Schema-With-Custom-Fields-And-Forward-Only-Relations.md)).
  3. **Strict Conflict Abstention ([ADR 0004](file:///home/pedrogoncalves/projects/akashic/docs/adr/0004-Strict-Conflict-Abstention-And-Plan-Then-Apply-Vault-Mutations.md))**: When two active Vault nodes contradict each other or form a cycle in the same scope, Akashic halts and reports an `Unresolved Conflict` rather than guessing.
  4. **Adversarial Falsification in RCA ([ADR 0008](file:///home/pedrogoncalves/projects/akashic/docs/adr/0008-Local-Host-RCA-Conductor-And-Soak-Storm-Deduplication.md))**: Stage 5 of RCA executes a secondary check that actively attempts to disprove candidate root causes against symbolicated stack frames, Git blame diffs, and documented `Negative Knowledge`.

#### Q3: "If we turn on `akashic daemon` against `gerrit.mot.com:29418` in Phase 2, won't continuous merge bursts thrash the host CPU and lock the SQLite database?"
- **Linked Slide**: Slide 2 (Hotkey `2`) and Slide 7 (Hotkey `7`).
- **Direct Answer**:
  1. **Physical 3-DB Isolation ([ADR 0002](file:///home/pedrogoncalves/projects/akashic/docs/adr/0002-Isolate-SQLite-Persistence-Into-Domain-Specific-Databases.md))**: `code_graph.db`, `vault_index.db`, and `rca_state.db` are separate SQLite files operating in WAL mode. Continuous AST indexing on `code_graph.db` never blocks incident checkpointing in `rca_state.db`.
  2. **Sliding-Window Debounce + Watermarks ([ADR 0009](file:///home/pedrogoncalves/projects/akashic/docs/adr/0009-Gerrit-Stream-Adaptive-Debounce-Watermark-Catchup-And-Auto-Discovery.md))**: Gerrit events are buffered in a min-heap with a 30-second quiet debounce window and a 300-second maximum cap, collapsing merge trains into single batch updates. A `loop_drop_prefix = "akashic/"` filter drops self-generated vault commit events. If the SSH stream disconnects, persistent high-watermarks replay missed events via `gerrit query after:<watermark>`.
  3. **Structural Significance Filter ([ADR 0006](file:///home/pedrogoncalves/projects/akashic/docs/adr/0006-Non-Destructive-Git-Ingestion-And-Structural-Significance-Filtering.md))**: If a commit only modifies private method bodies, comments, or formatting without altering method signatures, call edges, or type hierarchies, Akashic updates the commit watermark and skips graph re-indexing.

#### Q4: "How do you prevent proprietary keys, credentials, or user PII in Bug2Go logs from leaking into embeddings or prompts?"
- **Linked Slide**: Slide 9 (Appendix B - Hotkey `9`).
- **Direct Answer**:
  - `akashic-sanitize` sits at Tier 4 of the workspace DAG ([ADR 0007](file:///home/pedrogoncalves/projects/akashic/docs/adr/0007-Profile-Driven-Sanitization-And-Format-Preserving-Pseudonymization.md)). Compiler constraints prevent `akashic-engine`, `akashic-vault`, and `akashic-rca` from bypassing it.
  - **`CodeProfile`** scans source code before vectorization and replaces detected API keys and secrets with tokens like `SAN_STR_TOKEN_A8F1`, preserving valid AST syntax so Tree-sitter can parse the code.
  - **`LogProfile`** applies Format-Preserving Semantic Pseudonymization to logcat lines (e.g., `<PHONE:US_VALID_10DIGIT>`, `<MAC:ADDR_VALID>`) using a per-session cryptographic salt. This redacts user PII while preserving entity correlation across log lines and maintaining tabular column alignments.

---

### Part B: Android Principal Engineer

#### Q5: "Motorola Android apps have complex multi-module Gradle setups, product flavors, and split manifests. How does Akashic map crash packages to the right repository and code symbols?"
- **Linked Slide**: Slide 3 (Hotkey `3`) and Slide 10 (Appendix C - Hotkey `0`).
- **Direct Answer**:
  - `Clone-to-Onboard Discovery` runs a deterministic 5-pass parser across every Android repository in the workspace ([ADR 0009](file:///home/pedrogoncalves/projects/akashic/docs/adr/0009-Gerrit-Stream-Adaptive-Debounce-Watermark-Catchup-And-Auto-Discovery.md), [01-android-package-autodiscovery.md](file:///home/pedrogoncalves/projects/akashic/docs/wayfinder/research/01-android-package-autodiscovery.md)):
    1. Module `build.gradle` / `build.gradle.kts` for `applicationId` and `applicationIdSuffix`.
    2. Module-level `namespace` declarations.
    3. `AndroidManifest.xml` `package` attributes across main and flavor source sets.
    4. Java and Kotlin source file package prefix declarations.
    5. `.git/config` remote URL normalization to map Gerrit project paths (`29418/home/repo/dev/...`).
  - Across `AudioRecorder`, `MotoAssistPremium`, `motoDolbyUI`, and `FMRadio`, this automatically indexed **810 package-to-repository mappings** without requiring manual configuration.

#### Q6: "Does the Code Graph require a full Gradle build or Android SDK environment to index, and can it handle mixed Java, Kotlin, and C++ (JNI) apps?"
- **Linked Slide**: Slide 10 (Appendix C - Hotkey `0`) and Slide 3 (Hotkey `3`).
- **Direct Answer**:
  - **No Gradle build or Android SDK required**: Tree-sitter parses Java, Kotlin, C, C++, Rust, and Python directly from `gix` memory buffers in seconds, even on machines without a configured Android build environment.
  - **Protection of Gradle incremental build caches ([ADR 0006](file:///home/pedrogoncalves/projects/akashic/docs/adr/0006-Non-Destructive-Git-Ingestion-And-Structural-Significance-Filtering.md))**: Android builds are sensitive to file modification timestamps in the working tree. Because Akashic reads blobs directly from `.git/objects` in memory via `gix`, background code indexing never touches the working tree, never modifies timestamps, and never invalidates Gradle incremental build caches.

#### Q7: "During production soak testing, a single regression triggers hundreds of duplicate Bug2Go/MotoJira tickets. How does Akashic avoid running hundreds of redundant AI investigations?"
- **Linked Slide**: Slide 10 (Appendix C - Hotkey `0`).
- **Direct Answer**:
  - Stage 3 of `akashic-rca` symbolicates raw crash frames locally (`addr2line` for native ELF binaries and R8 `retrace` for obfuscated Java/Kotlin) using a local 2GB NVMe **Symbol Store LRU Cache** ([ADR 0008](file:///home/pedrogoncalves/projects/akashic/docs/adr/0008-Local-Host-RCA-Conductor-And-Soak-Storm-Deduplication.md)).
  - Stage 4 evaluates the **Soak Storm Deduplication Gate** before invoking any LLM:
    `CrashSignature = sha256(app_id + exception_or_signal + top_3_symbolicated_app_frames)`.
  - If an active Incident in `rca_state.db` or the Vault already matches that `CrashSignature`, Akashic increments the recurrence counter, attaches the new ticket as supporting evidence, and halts the pipeline immediately. This eliminates 100% of redundant LLM calls during soak testing storms.

#### Q8: "Realistically, Android feature teams are busy. How much documentation work are we asking them to do, and why can't we automate all of it from Confluence and Jira?"
- **Linked Slide**: Slide 5 (Hotkey `5`) and Slide 11 (Appendix D - Hotkey `-`).
- **Direct Answer**:
  - **Zero documentation is required on Day 0**: Teams immediately receive multi-repo Code Graph navigation, blast radius analysis, Complexity pre-review checks, and code-level RCA symbolication out of the box.
  - **Confluence and MotoJira MCPs assist in bootstrapping**: We query existing Confluence pages and Jira tickets via MCP tools, running `akashic vault plan` to generate draft OKF Markdown nodes automatically.
  - **Human curation is essential for authority**: An algorithm cannot deduce which of three conflicting Confluence pages reflects the team's true business intent. Domain teams only need to review and commit the authoritative specifications, coding standards, and ADRs via `akashic vault apply` and `akashic lint`. Once committed, RCA resolutions and supersessions update the Vault incrementally during standard engineering workflows.
