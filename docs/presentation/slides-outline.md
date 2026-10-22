# Akashic POV Presentation Storyboard

## Slide 1: POV THESIS
* **Timing**: 00:00 - 02:00
* **Action Title**: Context Retrieval Is the True Bottleneck for Enterprise AI Workflows
* **Layout Archetype**: `split-comparison`
* **Core Data Points**:
  1. The Ask & The Bottleneck Found: Tasked with building 3-Axis Code Review (Standards, Spec, Complexity) and Automated RCA. The LLM is not the bottleneck; deterministic context retrieval at OEM Android scale is.
  2. Why Off-the-Shelf Tools Fail: Python graphify invokes LLMs during indexing (costly, non-deterministic). Raw Confluence/MotoJira MCPs retrieve obsolete 2023 pages alongside 2025 specs with zero Scope or Supersession links.
  3. The Solution: Akashic. A single host-native Rust binary and rmcp MCP server unifying a deterministic Code Graph, governed OKF v0.2 Vault, and checkpointed Local-Host Conductor for RCA.
* **Speaker Notes**: Start strong. Acknowledge that everyone is trying to build "AI tools", but most hit a ceiling because they rely on vector search over messy, uncurated data. We found this out the hard way when building the 3-Axis code review. The fix wasn't a better prompt, it was a better context substrate.

## Slide 2: ARCHITECTURE
* **Timing**: 02:00 - 04:30
* **Action Title**: An 8-Crate Rust DAG and 3 Isolated SQLite Stores Eliminate Contention
* **Layout Archetype**: `diagram-spotlight`
* **Core Data Points**:
  1. 8-Crate DAG: akashic-core, akashic-daemon, akashic-rca, akashic-lint, akashic-engine, akashic-vault, akashic-sanitize, akashic-embedding.
  2. Strict Tier Layering: Entry, Runtime, Domain, Foundation.
  3. SQLite WAL Isolation: Disposable Cache (.akashic/code_graph.db, .akashic/vault_index.db) vs Durable Checkpoint (.akashic/rca_state.db).
* **Speaker Notes**: Why Rust? Predictability and memory footprint. We can't have an AI daemon dragging down the developer's machine. By splitting the storage into three isolated SQLite databases, we ensure that a massive indexing run never blocks a live incident RCA state update.

## Slide 3: ACT 1 - DAY-0 VALUE
* **Timing**: 04:30 - 08:00
* **Action Title**: Day-0 Static Discovery Indexes 20,231 Symbols Across 4 OEM Apps with Zero LLMs
* **Layout Archetype**: `metric-callout`
* **Core Data Points**:
  1. 20,231 Code Symbols, 810 Package Mappings, ~140k Code Edges, 0 LLM Indexing Calls.
  2. App Breakdown: AudioRecorder (9,792/180), MotoAssistPremium (4,857/488), motoDolbyUI (3,148/88), FMRadio (2,434/54).
  3. 5-Pass Pipeline: build.gradle -> Module namespace -> AndroidManifest.xml -> Java/Kotlin source prefix -> .git/config.
  4. Bare `gix` zero-mutation callout.
* **Speaker Notes**: Day-0 value means it works immediately upon clone. No manual tagging required. By reading bare git blobs in memory via `gix`, we extract the graph without ever touching the working tree or busting the developer's Gradle cache.

## Slide 4: LIVE / RECORDED DEMO
* **Timing**: 08:00 - 10:30
* **Action Title**: From Multi-Repo Blast Radius to Governed Vault Crystallization in Seconds
* **Layout Archetype**: `code-walkthrough`
* **Core Data Points**:
  1. 00:00 Multi-Repo Discovery
  2. 00:35 Transitive Blast Radius
  3. 01:10 Vault Supersession & Lint Gate
* **Speaker Notes**: Let's look at it in action. First, we index across boundaries. Second, we query the blast radius of a symbol change. Finally, we see how the vault catches a conflict and forces a lint gate resolution before accepting the new state.

## Slide 5: ACT 2 - THE INTENT GAP
* **Timing**: 10:30 - 13:30
* **Action Title**: Raw Confluence and MotoJira MCPs Cannot Replace Governed Repository Specs
* **Layout Archetype**: `split-comparison`
* **Core Data Points**:
  1. Code structure reveals what the code is, but verifying intent requires versioned, conflict-free rules.
  2. Day-0 Graph gives you structure. Raw RAG gives you noise. Day-1 Vault + Graph gives you governance.
  3. Bootstrap Contract: Domain teams must curate, verify, and commit authoritative specs into their repo Vault.
* **Speaker Notes**: Code tells you what the machine is doing, but not what the business wanted. If an LLM reads Confluence directly, it might grab a deprecated 2023 design doc. The Vault forces the team to commit their intent alongside the code.

## Slide 6: ACT 3 - GOVERNED VAULT
* **Timing**: 13:30 - 16:00
* **Action Title**: Forward-Only Supersession Prevents Stale Decisions from Poisoning RCA
* **Layout Archetype**: `diagram-spotlight`
* **Core Data Points**:
  1. OKF v0.2 Lineage on com.motorola.camera3 (MOTOCAM-9102 / B2G-882194).
  2. L001-L011 Linting.
  3. Forward-Only Supersession: You don't delete history, you supersede it.
* **Speaker Notes**: When an incident happens, you need to know *why* a decision was made. If you just overwrite a Markdown file, you lose the 'why'. By using Forward-Only Supersession, Akashic maintains a cryptographic chain of custody for architectural decisions.

## Slide 7: ROLLOUT & ASK
* **Timing**: 16:00 - 18:00
* **Action Title**: Workstation-First Adoption Scales Smoothly to Centralized Gerrit Automation
* **Layout Archetype**: `three-pillar-grid`
* **Core Data Points**:
  1. Phase 1: Developer & Triage Workstation First (Immediate) - Zero server infrastructure required.
  2. Phase 2: Centralized Gerrit Stream & Soak Runner (Scale) - Daemonized processing.
  3. Asks: Platform PE vs Android PE alignment.
* **Speaker Notes**: We are not asking for a massive DevOps cluster on Day 1. This runs locally on developer workstations today. Once proven across the triage teams, we promote the exact same binary to a Gerrit soak runner.

## Slide 8: APPENDIX A (PLATFORM PE)
* **Timing**: 18:00 - 30:00
* **Action Title**: Native Rust Indexing and Four-Layer Gerrit Burst Defense
* **Layout Archetype**: `split-comparison`
* **Core Data Points**:
  1. Rust akashic vs Python graphify: 0 LLM calls vs $$$, 100% AST vs Stochastic, 1 Binary vs 4 Repos.
  2. 4-Layer Burst Defense: Min-Heap Sliding Debouncer, Persistent Watermark, Structural Filter, WAL Isolation.
* **Speaker Notes**: For the Platform team: This is how we survive gerrit.mot.com merge storms. We debounce, we watermark, and we filter structurally significant events before hitting the databases.

## Slide 9: APPENDIX B (SECURITY & PII)
* **Timing**: 18:00 - 30:00
* **Action Title**: Mandatory Tier-4 Sanitization Scrubs Secrets and Pseudonymizes Logcat PII
* **Layout Archetype**: `code-walkthrough`
* **Core Data Points**:
  1. akashic-sanitize sits at the bottom of the crate DAG. No bypass possible.
  2. CodeProfile: AST-preserving secret replacement.
  3. LogProfile: Format-Preserving Semantic Pseudonymization (Phone, IMEI, MAC).
* **Speaker Notes**: Security is not a middleware, it is the lowest tier crate. Before any text hits an LLM or an embedding model, it must pass through the sanitize module. Format-preserving pseudonymization means the LLM can still trace network flows without seeing real IP or MAC addresses.

## Slide 10: APPENDIX C (ANDROID PE)
* **Timing**: 18:00 - 30:00
* **Action Title**: Zero-Gradle Bare Git Ingestion and Pre-LLM Soak Storm Deduplication
* **Layout Archetype**: `diagram-spotlight`
* **Core Data Points**:
  1. Bare git ingestion via gix (0 Gradle cache invalidation).
  2. 2GB NVMe LRU Symbolication Cache.
  3. CrashSignature deduplication halts duplicate soak crashes with 0 LLM calls.
* **Speaker Notes**: For the Android PEs: We know Gradle is slow. We don't touch it. We read directly from the .git/objects directory. When a soak storm hits, we deduplicate at the native crash signature level before ever waking up the LLM, saving thousands of API calls.

## Slide 11: APPENDIX D (HALLUCINATION DEFENSE)
* **Timing**: 18:00 - 30:00
* **Action Title**: Four-Layer Hallucination Defense and Pragmatic Team Curation Flywheel
* **Layout Archetype**: `three-pillar-grid`
* **Core Data Points**:
  1. 4-Layer Defense: Explicit Edge Provenance, Temporal Supersession, Strict Conflict Abstention, Adversarial Hypothesis Falsification.
  2. 3-Step Flywheel: Day-0 Graph -> AI-Assisted Bootstrap -> Team Sign-Off.
* **Speaker Notes**: How do we trust it? By not letting the AI guess the architecture. The Code Graph provides explicit edges. If there is a conflict in the Vault, the system abstains and forces a human to resolve it via Lint rules. The AI is a synthesizer, not an oracle.
