const fs = require('fs');
const file = '/home/pedrogoncalves/projects/akashic/docs/presentation/akashic-pov-deck.html';
let content = fs.readFileSync(file, 'utf8');

const slide7Replacement = `        <!-- Slide 7 -->
        <div class="slide" id="slide-7" data-act="ROLLOUT" data-time="16:00">
          <div class="kicker">07 // OPERATIONAL EVALUATION & TECHNICAL HAND-OFF</div>
          <div class="slide-title">Technical Validation and Operational Deployment Model</div>
          <div class="slide-subtitle">Engineering evaluation across local developer workstations and centralized Gerrit stream automation</div>
          <div class="split-col">
            <div class="split-layout" style="flex:0 0 auto;">
              <div class="card split-col">
                <div class="card-title">Operational Tier 1: Workstation-Local Toolchain (Evaluated)</div>
                <div class="card-text">CLI and MCP server deployed directly in local developer/agent workspaces. Zero centralized server infrastructure; local SQLite WAL provides instant query response.</div>
              </div>
              <div class="card split-col" style="border-color: var(--accent);">
                <div class="card-title" style="color: var(--accent-bright);">Operational Tier 2: Centralized Gerrit Stream Automation (Architecture Ready)</div>
                <div class="card-text">Daemon mode architecture running <code>akashic daemon</code> on build infrastructure, watching the Gerrit event stream for pre-merge validation, soak crash deduplication, and automated gating.</div>
              </div>
            </div>
            <div class="split-layout">
              <div class="card split-col" style="background: rgba(255,255,255,0.02);">
                <div class="card-title">Platform Engineering Findings & Interfaces</div>
                <ul class="card-text" style="padding-left: 20px;">
                  <li>Single static Rust binary footprint with zero external runtime dependencies (JVM/Python-free).</li>
                  <li>3-DB physical WAL architecture (<code>code_graph.db</code>, <code>vault.db</code>, <code>rca_state.db</code>) guarantees concurrent agent reads without write lock starvation.</li>
                  <li>Tier-4 sanitization crate enforces mandatory logcat pseudonymization and secret scrubbing at the compiler level prior to agent context injection.</li>
                </ul>
              </div>
              <div class="card split-col" style="background: rgba(255,255,255,0.02);">
                <div class="card-title">Android OEM Engineering Findings & Interfaces</div>
                <ul class="card-text" style="padding-left: 20px;">
                  <li>5-Pass Clone-to-Onboard algorithm accurately resolves OEM multi-repo topologies and Gradle flavors.</li>
                  <li>Bare Git ingestion (<code>gix</code>) reads objects in memory, ensuring zero invalidation of local Gradle build caches.</li>
                  <li>Project domain teams maintain versioned Markdown specs and ADRs in repository vaults rather than relying on uncurated wiki pages.</li>
                </ul>
              </div>
            </div>
          </div>
        </div>`;

const slide8Replacement = `        <!-- Slide 8 -->
        <div class="slide" id="slide-8" data-act="APPENDIX A" data-time="18:00">
          <div class="kicker">08 // APPENDIX A: CONSOLIDATED ARCHITECTURE & AGENT MCP</div>
          <div class="slide-title">Single Consolidated Rust Engine and Native Agent MCP Server</div>
          <div class="slide-subtitle">Consolidating exploratory code-review and RCA research into a unified binary with native stdio JSON-RPC agent tools</div>
          <div class="split-layout">
            <div class="card split-col">
              <div class="card-title">Research Evolution: Prototype vs Unified Engine</div>
              <table class="table-matrix">
                <thead>
                  <tr>
                    <th>Axis</th>
                    <th>Early Research Prototype (Python/LLM)</th>
                    <th>Consolidated Rust Engine</th>
                  </tr>
                </thead>
                <tbody>
                  <tr><th>Indexing Cost</th><td>High Cloud Token Burn</td><td>0 Cloud Tokens (Native AST)</td></tr>
                  <tr><th>Graph Determinism</th><td>Stochastic / Non-deterministic</td><td>100% Deterministic Tree-sitter AST</td></tr>
                  <tr><th>Implementation Scope</th><td>Disparate exploratory scripts</td><td>Single Unified Binary (<code>akashic</code>)</td></tr>
                  <tr><th>Agent Interface</th><td>Fragmented experimental CLIs</td><td>Unified <code>rmcp</code> Model Context Protocol Server</td></tr>
                </tbody>
              </table>
              <div style="font-size: 13px; color: var(--text-muted); margin-top: 16px;">
                Note: <code>alexandria</code> and <code>akashic-records</code> were internal research prototypes developed during code-review and RCA experiments. <code>akashic</code> is the sole consolidated production architecture.
              </div>
            </div>
            <div class="card split-col">
              <div class="card-title">Agent MCP Architecture: Deterministic Grounding</div>
              <div class="code-block" style="font-size: 13px; line-height: 1.5; background: #0a0a0a; border-radius: 6px; padding: 12px; overflow-y: auto;">
<div style="color: #ffbd2e; margin-bottom: 8px;">Transport: stdio (JSON-RPC 2.0 via rmcp)</div>
<div style="color: var(--accent-bright);">akashic_graph_query</div>
<div style="color: var(--text-secondary); margin-bottom: 8px;">Provides instant symbol navigation and caller graphs across OEM apps.</div>
<div style="color: var(--accent-bright);">akashic_blast_radius</div>
<div style="color: var(--text-secondary); margin-bottom: 8px;">Computes transitive caller/callee closures before agent modifies code.</div>
<div style="color: var(--accent-bright);">akashic_vault_query</div>
<div style="color: var(--text-secondary); margin-bottom: 8px;">Injects active, conflict-free domain rules (<code>mode: current</code>) and filters superseded decisions.</div>
<div style="color: var(--accent-bright);">akashic_rca_symbolicate</div>
<div style="color: var(--text-secondary); margin-bottom: 8px;">Automated native/Java crash symbolication and SHA-256 signature deduplication.</div>
              </div>
              <div style="font-size: 14px; font-weight: bold; margin-top: 16px; padding: 12px; background: rgba(255,255,255,0.05); border-radius: 8px; border-left: 4px solid var(--accent);">
                Takeaway: Agents (Gemini Code Assist, Herdr, Claude) obtain verified repository facts and governed rules through low-latency tool calls without hallucination.
              </div>
            </div>
          </div>
        </div>`;

// Replace Slide 7
content = content.replace(/<!-- Slide 7 -->[\s\S]*?(?=<!-- Slide 8 -->)/, slide7Replacement + '\\n\\n');

// Replace Slide 8
content = content.replace(/<!-- Slide 8 -->[\s\S]*?(?=<!-- Slide 9 -->)/, slide8Replacement + '\\n\\n');

fs.writeFileSync(file, content, 'utf8');
console.log("Successfully replaced Slides 7 and 8!");
