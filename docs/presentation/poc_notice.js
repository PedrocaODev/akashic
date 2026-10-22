const fs = require('fs');
const file = '/home/pedrogoncalves/projects/akashic/docs/presentation/akashic-pov-deck.html';
let content = fs.readFileSync(file, 'utf8');

const targetStr = `<li><strong>Plan -&gt; Lint -&gt; Apply:</strong> All vault updates must pass deterministic L001-L011 rules.</li>
                </ul>`;

const replacement = `<li><strong>Plan -&gt; Lint -&gt; Apply:</strong> All vault updates must pass deterministic L001-L011 rules.</li>
                </ul>
                <div style="background: rgba(255, 189, 46, 0.08); border: 1px solid rgba(255, 189, 46, 0.25); border-radius: 8px; padding: 12px 16px; margin-top: 12px; font-size: 13px; color: var(--text-secondary);">
                  <strong style="color: #ffbd2e;">Data Provenance Notice:</strong> The com.motorola.camera3 incident, stack trace, and Gerrit CL references (489201 / 490102) are synthetic PoC test fixtures authored to stress-test OKF v0.2 supersession DAG resolution. Production vaults will be bootstrapped with real team artifacts.
                </div>`;

content = content.replace(targetStr, replacement);
fs.writeFileSync(file, content, 'utf8');
console.log("Successfully added Data Provenance Notice!");
