const fs = require('fs');
const file = '/home/pedrogoncalves/projects/akashic/docs/presentation/akashic-pov-deck.html';
let content = fs.readFileSync(file, 'utf8');

const slide4Replacement = `<!-- Slide 4 -->
        <div class="slide" id="slide-4" data-act="LIVE DEMO" data-time="08:00">
          <div class="kicker">04 // TWO-MINUTE LIVE / RECORDED TERMINAL WALKTHROUGH</div>
          <div class="slide-title">From Multi-Repo Blast Radius to Governed Vault Crystallization in Seconds</div>
          <div class="slide-subtitle">Interactive chapter replay of live CLI and rmcp MCP execution across Motorola OEM repositories</div>
          <div class="split-layout">
            <!-- Left Column (40%) -->
            <div class="split-col col-40" style="display: flex; flex-direction: column; gap: 16px;">
              <!-- Chapter Selector Tabs -->
              <div id="chapter-tabs" style="display: flex; flex-direction: column; gap: 8px;">
                <div class="badge accent" id="tab-1" onclick="window.setDemoChapter(1)" style="cursor: pointer; padding: 12px; font-size: 16px; border: 1px solid var(--accent); transition: all 0.2s;">Chapter 1: Multi-Repo OEM Discovery &amp; Scale</div>
                <div class="badge" id="tab-2" onclick="window.setDemoChapter(2)" style="cursor: pointer; padding: 12px; font-size: 16px; border: 1px solid transparent; transition: all 0.2s;">Chapter 2: Pre-Review Blast Radius &amp; Provenance</div>
                <div class="badge" id="tab-3" onclick="window.setDemoChapter(3)" style="cursor: pointer; padding: 12px; font-size: 16px; border: 1px solid transparent; transition: all 0.2s;">Chapter 3: Camera3 RCA, Lint Gate &amp; Forward Supersession</div>
              </div>
              <!-- Takeaways Card -->
              <div class="card" id="takeaways-card" style="flex: 1; display: flex; flex-direction: column; justify-content: center;">
                <div class="card-title" id="takeaways-title" style="margin-bottom: 12px;">Chapter 1 Highlights</div>
                <div class="card-text" id="takeaways-content" style="font-size: 16px; line-height: 1.5;"></div>
              </div>
            </div>
            <!-- Right Column (60%) -->
            <div class="split-col col-60">
              <div class="card" style="flex: 1; padding: 0; overflow: hidden; display: flex; flex-direction: column; height: 100%;">
                <!-- Terminal Header -->
                <div style="height: 40px; background: #1e1e1e; display: flex; align-items: center; padding: 0 16px; gap: 8px; border-bottom: 1px solid #333; flex-shrink: 0;">
                  <div style="width: 12px; height: 12px; border-radius: 50%; background: #ff5f56;"></div>
                  <div style="width: 12px; height: 12px; border-radius: 50%; background: #ffbd2e;"></div>
                  <div style="width: 12px; height: 12px; border-radius: 50%; background: #27c93f;"></div>
                  <div style="color: #888; font-family: var(--font-mono); font-size: 13px; margin-left: 12px;">akashic-cli v0.2.0 -- deterministic gate</div>
                </div>
                <!-- Terminal Content -->
                <div class="code-block" id="terminal-content" style="flex: 1; border: none; border-radius: 0; background: #0a0a0a; margin: 0; font-size: 13px; line-height: 1.4; overflow-y: auto; white-space: pre-wrap;"></div>
              </div>
            </div>
          </div>
        </div>`;

content = content.replace(/<!-- Slide 4 -->[\s\S]*?(?=<!-- Slide 5 -->)/, slide4Replacement + '\n\n        ');

const demoScript = `
    const demoData = {
      1: {
        title: "Chapter 1 Highlights",
        takeaways: "4 OEM Repositories, 810 Package Mappings, 20,231 Symbols, 91,077 AST Edges vs 52,183 Inferred Edges. Architectural God-Nodes (<code>logD</code>, <code>run</code>, <code>TranscribeAudioAICore</code>).",
        terminal: '<span style="color:#00d8d6;">akashic-pov</span> <span style="color:#2ecc71;">❯</span> <span style="color:#fff;">akashic graph discover --summary</span>\\nMulti-Repo OEM Discovery (settings.gradle + AndroidManifest.xml 5-pass index):\\n\\n<span style="color:#00d8d6;">REPOSITORY</span>         | GERRIT PROJECT                   | PRIMARY APPLICATION ID (VAULT PARTITION)         |  <span style="color:#2ecc71;">PKGS</span> | <span style="color:#2ecc71;">SYMBOLS</span>\\n--------------------------------------------------------------------------------------------------------------------------\\n<span style="color:#00d8d6;">AudioRecorder</span>      | ...a/packages/apps/AudioRecorder | com.motorola.audiorecorder.recordingservice      |   <span style="color:#2ecc71;">180</span> |   <span style="color:#2ecc71;">9,792</span>\\n<span style="color:#00d8d6;">FMRadio</span>            | ...otorola/packages/apps/FMRadio | com.motorola.fmsdktestapp                        |    <span style="color:#2ecc71;">54</span> |   <span style="color:#2ecc71;">2,434</span>\\n<span style="color:#00d8d6;">MotoAssistPremium</span>  | apps/MotoAssistPremium           | com.motorola.moto                                |   <span style="color:#2ecc71;">488</span> |   <span style="color:#2ecc71;">4,857</span>\\n<span style="color:#00d8d6;">motoDolbyUI</span>        | ...ola/packages/apps/motoDolbyUI | com.motorola.motoaudioeffects                    |    <span style="color:#2ecc71;">88</span> |   <span style="color:#2ecc71;">3,148</span>\\n==========================================================================================================================\\n<span style="color:#ffbd2e;">TOTALS: 4 OEM Repositories | 810 Package Mappings | 20,231 Code Symbols</span>\\nEDGE PROVENANCE: 143,260 Edges (91,077 EXTRACTED AST [calls/inherits] | 52,183 INFERRED [score >= 0.82])\\n\\n<span style="color:#00d8d6;">akashic-pov</span> <span style="color:#2ecc71;">❯</span> <span style="color:#fff;">akashic graph god-nodes --repo-name AudioRecorder --top-n 5</span>\\nlogD: in=2160, out=5, total=2165\\nlogW: in=836, out=3, total=839\\nlogI: in=513, out=1, total=514\\nverify: in=255, out=4, total=259\\nTranscribeAudioAICore: in=162, out=6, total=168'
      },
      2: {
        title: "Chapter 2 Highlights",
        takeaways: "Deep Transitive Closure for <code>removeUIObservers()</code>. Pre-review gate feeds 46 upstream callers, 24 downstream dependencies. Provenance breakdown (100% Extracted AST vs 0.95+ semantic similarity).",
        terminal: '<span style="color:#00d8d6;">akashic-pov</span> <span style="color:#2ecc71;">❯</span> <span style="color:#fff;">akashic graph blast-radius --repo-name AudioRecorder "AudioRecorder::...::MainFragment.removeUIObservers()#0" --max-depth 2</span>\\nBlast radius: <span style="color:#2ecc71;">46</span> upstream callers, <span style="color:#2ecc71;">24</span> downstream dependencies\\n\\n<span style="color:#00d8d6;">akashic-pov</span> <span style="color:#2ecc71;">❯</span> <span style="color:#fff;">akashic graph edges --symbol "MainFragment.removeUIObservers" --provenance</span>\\nTarget Symbol: MainFragment.removeUIObservers()\\n\\nEDGE TYPE            | PROVENANCE TIER  | SCORE  | CONNECTED TARGET SYMBOL\\n------------------------------------------------------------------------------------------\\nCALLS                | EXTRACTED        | 1.00   | logD\\nCALLS                | EXTRACTED        | 1.00   | requireActivity\\nCALLS                | EXTRACTED        | 1.00   | unregisterLargeDisplayUIObservers\\nCALLS                | EXTRACTED        | 1.00   | removeRecordingObservers\\nCALLS                | EXTRACTED        | 1.00   | removePermissionsObservers\\nSEMANTIC_SIMILARITY  | INFERRED         | 0.96   | MainFragmentCLI.removeUIObservers()#0\\nSEMANTIC_SIMILARITY  | INFERRED         | 0.95   | RecordsListFragment.removeObservers()#0\\n==========================================================================================\\n<span style="font-weight:bold;">Verification Axis: Complexity & Blast Radius feeds automated pre-review gate.</span>'
      },
      3: {
        title: "Chapter 3 Highlights",
        takeaways: "Deterministic Lint Gate (100% valid, 0 issues). Automated Forward Supersession: detects stale optimization, auto-routes to active revert <code>decision-revert-cl-489201</code> (Gerrit CL 490102).",
        terminal: '<span style="color:#00d8d6;">akashic-pov</span> <span style="color:#2ecc71;">❯</span> <span style="color:#fff;">akashic lint --vault prototype-akashic-rca-vault/akashic/com.motorola.camera3 --strict</span>\\n<span style="font-weight:bold;">Lint result: valid = true, issues = 0</span>\\n\\n<span style="color:#00d8d6;">akashic-pov</span> <span style="color:#2ecc71;">❯</span> <span style="color:#fff;">akashic vault query --mode current --node-id decision-buffer-release-optimization</span>\\n{\\n  "mode": "current",\\n  "seed_id": "decision-buffer-release-optimization",\\n  "is_current": false,\\n  "active_guidance_id": "decision-revert-cl-489201",\\n  "supersession_chain": [\\n    {\\n      "predecessor": "decision-buffer-release-optimization",\\n      "successor": "decision-revert-cl-489201",\\n      "evidence": "../evidence/evidence-motocam-9102-stack.md"\\n    }\\n  ]\\n}\\n\\n<span style="color:#2ecc71; font-weight:bold;">[FORWARD SUPERSESSION RESOLVED]</span>\\n  Query Seed: decision-buffer-release-optimization (is_current: false)\\n  Active Guidance: decision-revert-cl-489201 (Gerrit CL 490102)\\n  Supersession Chain: decision-buffer-release-optimization -> decision-revert-cl-489201'
      }
    };

    window.setDemoChapter = function(chapter) {
      document.getElementById('tab-1').className = 'badge';
      document.getElementById('tab-2').className = 'badge';
      document.getElementById('tab-3').className = 'badge';
      document.getElementById('tab-1').style.borderColor = 'transparent';
      document.getElementById('tab-2').style.borderColor = 'transparent';
      document.getElementById('tab-3').style.borderColor = 'transparent';
      
      const activeTab = document.getElementById('tab-' + chapter);
      if (activeTab) {
        activeTab.className = 'badge accent';
        activeTab.style.borderColor = 'var(--accent)';
      }
      
      const tc = document.getElementById('takeaways-content');
      const tt = document.getElementById('takeaways-title');
      const term = document.getElementById('terminal-content');
      
      if (tc && tt && term && demoData[chapter]) {
        tt.innerHTML = demoData[chapter].title;
        tc.innerHTML = demoData[chapter].takeaways;
        term.innerHTML = demoData[chapter].terminal;
      }
    };
    
    // Initialize chapter 1 once DOM is loaded
    setTimeout(() => window.setDemoChapter(1), 100);
`;

content = content.replace("const loadVideoBtn = document.getElementById('load-video-btn');\\n    const demoPlaceholder = document.getElementById('demo-placeholder');", demoScript);
content = content.replace(/const video = document\.getElementById\('demo-video'\);\n\s*const loadVideoBtn = document\.getElementById\('load-video-btn'\);\n\s*const demoPlaceholder = document\.getElementById\('demo-placeholder'\);/g, demoScript);

const oldKeydown = "if (e.key >= '1' && e.key <= '9') { updateSlide(parseInt(e.key) - 1); }";
const newKeydown = "if (e.key >= '1' && e.key <= '3' && slides[currentSlide].id === 'slide-4') {\n        window.setDemoChapter(parseInt(e.key));\n        return;\n      }\n      if (e.key >= '1' && e.key <= '9') { updateSlide(parseInt(e.key) - 1); }";

content = content.replace(oldKeydown, newKeydown);

// Remove the load video event listener block using regex
content = content.replace(/if\s*\(loadVideoBtn\s*&&\s*video\)\s*\{[\s\S]*?\}\n/, '');

fs.writeFileSync(file, content, 'utf8');
console.log("Successfully replaced Slide 4 content!");
