const fs = require('fs');
const file = '/home/pedrogoncalves/projects/akashic/docs/presentation/akashic-pov-deck.html';
let content = fs.readFileSync(file, 'utf8');

// 1. Update Slide 4
content = content.replace(
    'Chapter 1: Multi-Repo OEM Discovery &amp; Scale',
    'Chapter 1: Multi-Repo OEM Discovery &amp; Scale [PRODUCTION FACT // LIVE AST]'
);
content = content.replace(
    'Chapter 2: Pre-Review Blast Radius &amp; Provenance',
    'Chapter 2: Pre-Review Blast Radius &amp; Provenance [PRODUCTION FACT // LIVE AST]'
);
content = content.replace(
    'Chapter 3: Camera3 RCA, Lint Gate &amp; Forward Supersession',
    'Chapter 3: Camera3 RCA &amp; Supersession [PoC SIMULATION // SYNTHETIC SCENARIO]'
);

// In the demoData for chapter 3
content = content.replace(
    'takeaways: "Deterministic Lint Gate (100% valid, 0 issues). Automated Forward Supersession: detects stale optimization, auto-routes to active revert <code>decision-revert-cl-489201</code> (Gerrit CL 490102).",',
    'takeaways: "Proof of Concept Reference Scenario: Built on synthetic reference vault fixtures (prototype-akashic-rca-vault) to validate OKF v0.2 L001-L011 lint gates and forward-only supersession resolution prior to production vault rollout.",'
);


// 2. Update Slide 6
const slide6TitleRegex = /(<div class="slide-title">Forward-Only Supersession Prevents Stale Decisions from Poisoning RCA<\/div>)/;
content = content.replace(slide6TitleRegex, '<div class="badge" style="background: rgba(255, 189, 46, 0.15); color: #ffbd2e; border: 1px solid rgba(255, 189, 46, 0.4); margin-bottom: 8px; align-self: flex-start; display: inline-block;">SYNTHETIC REFERENCE ARCHITECTURE // PoC SCENARIO</div>\n          $1');

const slide6SubtitleRegex = /(<div class="slide-subtitle">)OKF v0.2 lineage on com\.motorola\.camera3 \(MOTOCAM-9102 \/ B2G-882194\) with deterministic L001-L011 linting(<\/div>)/;
content = content.replace(slide6SubtitleRegex, '$1Illustrative reference architecture in prototype-akashic-rca-vault demonstrating forward-only supersession and deterministic L001-L011 linting before production bootstrap.$2');

// Add the callout banner inside Slide 6 (in the right column below the inspector)
const slide6RightColRegex = /(<div class="code-block".*?>\{\n\s+"seed_id": "decision-buffer-opt",[\s\S]*?<\/div>)/;
content = content.replace(slide6RightColRegex, '$1\n              <div style="background: rgba(255, 189, 46, 0.08); border: 1px solid rgba(255, 189, 46, 0.25); border-radius: 8px; padding: 12px 16px; margin-top: 12px; font-size: 13px; color: var(--text-secondary);"><strong style="color: #ffbd2e;">Data Provenance Notice:</strong> The com.motorola.camera3 incident, stack trace, and Gerrit CL references (489201 / 490102) are synthetic PoC test fixtures authored to stress-test OKF v0.2 supersession DAG resolution. Production vaults will be bootstrapped with real team artifacts.</div>');

// 3. Update Slide 7
const slide7Phase1Regex = /(<div class="card-text">)Deployed as a local CLI\. Developers run <code>akashic<\/code> to query the graph and manage the vault locally before pushing to Gerrit\. Zero ops overhead\.(<\/div>)/;
content = content.replace(slide7Phase1Regex, '$1Transition from Synthetic PoC Reference Vault (Camera3) to the First Production Domain Vault on an indexed OEM repository (AudioRecorder or motoDolbyUI). Deployed as a local CLI with zero ops overhead.$2');


fs.writeFileSync(file, content, 'utf8');
console.log("Successfully replaced Slide 4, 6 and 7 PoC content!");
