# Research Report: Gemini CLI Headless Invocation and Local-Host Conductor Integration Contract

**Ticket**: [Gemini CLI Headless Invocation and Local-Host Conductor Integration Contract](../tickets/02-gemini-cli-headless-contract.md)  
**Date**: 2026-10-15  
**Target Binary**: `@google/gemini-cli` v0.64.0 (`/home/pedrogoncalves/.nvm/versions/node/v24.19.0/bin/gemini`)  
**Verified Against Source Bundles**:
- `/home/pedrogoncalves/.nvm/versions/node/v24.19.0/lib/node_modules/@google/gemini-cli/bundle/gemini-Q2GRYIAK.js`
- `/home/pedrogoncalves/.nvm/versions/node/v24.19.0/lib/node_modules/@google/gemini-cli/bundle/chunk-NR7L3FXV.js`
- `/home/pedrogoncalves/.nvm/versions/node/v24.19.0/lib/node_modules/@google/gemini-cli/bundle/chunk-QCHFD56F.js`

---

## 1. Executive Summary

To execute Stage 4 Hypothesis Generation and Adversarial Falsification from the Rust **Local-Host Conductor** (`rca-conductor`) without paid API keys or containerized credential exposure, `rca-conductor` spawns the local authenticated `gemini` v0.64.0 CLI as a child process using `tokio::process::Command`.

Direct inspection of the compiled v0.64.0 bundles uncovered five critical implementation contracts:
1. **CLI Flag Contract & Stdin Framing**: Pass large crash evidence via `stdin` (closing `ChildStdin` immediately to bypass the 500ms `readStdin` timeout) and pass the role directive via `-p`, alongside `--output-format json --approval-mode default --skip-trust -e none --allowed-mcp-server-names akashic,alexandria,motojira-mcp,gerrit-mcp --policy <path/to/stage4-readonly.toml> -m <model>`.
2. **Why `--approval-mode plan` Cannot Be Used**: Built-in `policies/plan.toml` denies `*` at priority `1.040`, downgrades MCP `readOnlyHint` tools to `ask_user` (which converts to `DENY` in headless non-interactive mode), and `--allowed-mcp-server-names` (`4.100`) only applies to `nonPlanModes`.
3. **Output Envelope & `stdout`/`stderr` Stream Asymmetry**: On exit code `0`, `runNonInteractive()` writes `{ "session_id", "response", "stats", "warnings"? }` to `stdout`, where `response` is an ANSI-stripped string requiring second-pass `serde_json` parsing. On any fatal error (`1`, `41`, `42`, `52`, `53`, `130`), `handleError()` writes `{ "session_id", "error": { "type", "message", "code" } }` to **`stderr`** while leaving `stdout` empty.
4. **Enterprise OAuth (`oauth-personal`) & Environment Contract**: `gemini-cli` reuses `~/.gemini/settings.json` (`selectedType: "oauth-personal"`), `~/.gemini/oauth_creds.json`, and `GOOGLE_CLOUD_PROJECT` in `~/.gemini/.env`. Because `findEnvFile()` skips `~/.gemini/.env` if the current working directory is untrusted, `rca-conductor` must pass `--skip-trust` and set `GEMINI_CLI_TRUST_WORKSPACE=true`, `GEMINI_CLI_NO_RELAUNCH=true`, `NO_BROWSER=true`, and `CI=true`, while stripping `GEMINI_API_KEY` / `GOOGLE_API_KEY`.
5. **MCP Discovery & Tier-4 Policy TOML**: `gemini-cli` reads `mcpServers` exclusively from `~/.gemini/settings.json` or `<cwd>/.gemini/settings.json` (ignoring Antigravity's `~/.gemini/config/mcp_config.json`). A custom `--policy stage4-readonly.toml` (loaded at Tier 4) enforces read-only tool execution during Stage 4.

---

## 2. Exact Non-Interactive CLI Flags & Stdin Protocol

```bash
gemini \
  --output-format json \
  --approval-mode default \
  --skip-trust \
  -e none \
  --allowed-mcp-server-names akashic,alexandria,motojira-mcp,gerrit-mcp \
  --policy /home/pedrogoncalves/projects/knowledge-base/config/policies/stage4-readonly.toml \
  -m gemini-2.5-pro \
  -p "Execute Stage 4 RCA Hypothesis Generation and return ONLY valid JSON matching RcaHypothesisOutput."
```

### Stdin + `-p` Concatenation (`gemini-Q2GRYIAK.js:6845-6855`)
- When `stdin` is not a TTY (`!process.stdin.isTTY`), `gemini-cli` invokes `readStdin()`.
- `readStdin()` waits up to **500ms** if no `data` chunk arrives before `end`. Always write the full evidence bundle to `ChildStdin` and call `drop(stdin)` (`shutdown().await`) immediately.
- `gemini-cli` prepends `stdinData` to the `-p` prompt: `` `${stdinData}\n\n${question}` ``.
- Passing `-e none` disables unnecessary global CLI extensions for faster cold start while keeping `--allowed-mcp-server-names` active.

---

## 3. Why `--approval-mode plan` Fails for Headless MCP Calls

In `bundle/policies/plan.toml` and `chunk-NR7L3FXV.js`:
1. `plan.toml` defines a catch-all `decision = "deny"` at priority `40` (effective Tier 1 priority `1.040`) and only allows hardcoded built-in tools (`glob`, `grep_search`, `read_file`, `list_directory`) or MCP tools annotated with `annotations.readOnlyHint = true` with `decision = "ask_user"` (`1.050`).
2. In headless mode (`!process.stdin.isTTY`), `createPolicyEngineConfig()` sets `nonInteractive: true`. Inside `PolicyEngine.check()` (`chunk-QCHFD56F.js:68335`), any `"ask_user"` decision is converted directly into `"deny"`.
3. Furthermore, `--allowed-mcp-server-names` injects rules at priority `4.100`, but restricts them to `modes: nonPlanModes` (`['default', 'autoEdit', 'yolo']`).
4. Even in `default` mode, built-in `policies/read-only.toml` only allows built-in read tools (`1.050`), while `--allowed-mcp-server-names` (`4.100`) allows *all* tools on those MCP servers (including write tools like `create_issue` or `akashic_crystallize`).
5. **Solution**: Run with `--approval-mode default` + `--allowed-mcp-server-names akashic,alexandria,motojira-mcp,gerrit-mcp` + `--policy stage4-readonly.toml`.

---

## 4. Tier-4 Policy Engine Configuration (`config/policies/stage4-readonly.toml`)

User policies passed via `--policy <file.toml>` are loaded at **Tier 4** (`priority = 4 + rule.priority / 1000`), overriding Tier 1 built-in policies (`1.xxx`) and the `--allowed-mcp-server-names` baseline (`4.100`):

```toml
# /home/pedrogoncalves/projects/knowledge-base/config/policies/stage4-readonly.toml

# 1. Default deny all shell/write/edit built-in tools and unknown MCP tools (effective priority 4.500 > 4.100)
[[rule]]
toolName = "*"
decision = "deny"
priority = 500
 deny_message = "Stage 4 RCA reasoning is strictly read-only. Mutation tools and arbitrary shell commands are forbidden."

# 2. Allow safe built-in read-only file/search tools (effective priority 4.700)
[[rule]]
toolName = ["read_file", "glob", "grep_search", "list_directory"]
decision = "allow"
priority = 700

# 3. Allow read-only MCP tools on akashic, alexandria, motojira-mcp, and gerrit-mcp (effective priority 4.800)
[[rule]]
mcpName = ["akashic", "alexandria", "motojira-mcp", "gerrit-mcp"]
toolName = "*"
decision = "allow"
priority = 800

# 4. Explicitly block any mutating MCP tools even on allowed MCP servers (effective priority 4.950)
[[rule]]
mcpName = ["akashic", "alexandria", "motojira-mcp", "gerrit-mcp"]
toolName = [
  "create_issue",
  "update_issue",
  "add_comment",
  "transition_issue",
  "gerrit_review",
  "gerrit_submit",
  "gerrit_abandon",
  "crystallize",
  "write_node",
  "delete_node"
]
decision = "deny"
priority = 950
deny_message = "Stage 4 cannot mutate Jira, Gerrit, or the Akashic Vault; Stage 5 Crystallizer owns all writes."
```

---

## 5. JSON Output Envelope, Exit Codes & Stream Asymmetry

### 5.1 Success Envelope (`exit code 0` -> `stdout`)
Implemented in `JsonFormatter.format()` (`chunk-NR7L3FXV.js:126401`) and written via `process.stdout.write` (`gemini-Q2GRYIAK.js:6591`):

```json
{
  "session_id": "e4d909c2-...",
  "response": "{\"incident_id\":\"INC-MOTO-1234\",\"hypotheses\":[...]}",
  "stats": {
    "models": {
      "gemini-2.5-pro": {
        "api": { "totalRequests": 2, "totalErrors": 0, "totalLatencyMs": 8420 },
        "tokens": { "input": 4120, "prompt": 4120, "candidates": 690, "total": 4810, "cached": 0, "thoughts": 512, "tool": 890 }
      }
    },
    "tools": {
      "totalCalls": 3,
      "totalSuccess": 3,
      "totalFail": 0,
      "totalDurationMs": 215,
      "totalDecisions": { "accept": 3, "reject": 0, "modify": 0, "auto_accept": 3 },
      "byName": {}
    },
    "files": { "totalLinesAdded": 0, "totalLinesRemoved": 0 }
  },
  "warnings": []
}
```

- **Two-Stage Deserialization**: `envelope.response` is a `String` (ANSI-stripped via `stripAnsi`). `rca-conductor` must first deserialize `GeminiCliJsonEnvelope` from `stdout`, strip any optional ```` ```json ... ``` ```` markdown fences from `envelope.response`, and then deserialize `RcaHypothesisOutput` / `RcaFalsifierOutput`.

### 5.2 Error Envelope (`exit code != 0` -> `stderr`)
Implemented in `handleError()` (`chunk-NR7L3FXV.js:126502`):
- When an error occurs in `--output-format json` mode, `gemini-cli` writes the JSON error payload to **`process.stderr.write(...)`**, **NOT `stdout`**.

```json
{
  "session_id": "e4d909c2-...",
  "error": {
    "type": "FatalAuthenticationError",
    "message": "...",
    "code": 41
  }
}
```

### 5.3 Deterministic Exit Code Handling in `rca-conductor`
| Exit Code | Error Type | `rca-conductor` Action |
| :---: | :--- | :--- |
| `0` | Success | Parse `stdout` as `GeminiCliJsonEnvelope`, then parse `.response`. |
| `41` | `FatalAuthenticationError` | Do **not** retry. Mark investigation `PausedAuthRequired` and instruct user to run interactive `gemini` to refresh OAuth. |
| `42` | `FatalInputError` | Do **not** retry. Fail stage permanently (`InvalidPromptPayload`). |
| `52` | `FatalConfigError` | Do **not** retry. Fail stage permanently (`InvalidSettingsOrPolicy`). |
| `53` | `FatalTurnLimitedError` | Retry once with compacted evidence or higher `--max-session-turns`. |
| `130` | `FatalCancellationError` | Process interrupted (`SIGINT`/`SIGTERM`); checkpoint state for resume. |
| `1` | General / API / Quota Error | Inspect `stderr` error message; retry with exponential backoff if transient (HTTP 429/503). |

---

## 6. Enterprise OAuth (`oauth-personal`) & Subprocess Environment Contract

1. **Host Credentials**:
   - `~/.gemini/settings.json`: `"security": { "auth": { "selectedType": "oauth-personal" } }`
   - `~/.gemini/oauth_creds.json`: OAuth refresh/access tokens (fallback when OS keychain is unavailable).
   - `~/.gemini/.env`: `GOOGLE_CLOUD_PROJECT=motorola.com:ally` (required by Code Assist Enterprise `oauth-personal` flow).
2. **Critical Workspace Trust Pitfall (`findEnvFile` in `chunk-NR7L3FXV.js:132245`)**:
   - If the subprocess working directory is not trusted in `~/.gemini/trustedFolders.json` and workspace trust is not bypassed, `findEnvFile()` refuses to load `.env` files, causing `GOOGLE_CLOUD_PROJECT` to be missing and authentication to fail.
   - Always pass `--skip-trust` **and** set `GEMINI_CLI_TRUST_WORKSPACE=true`.
3. **Mandatory Environment Variables for `tokio::process::Command`**:
   - Inherit `HOME`, `PATH`, `USER`, `LOGNAME`, `DBUS_SESSION_BUS_ADDRESS`, `XDG_RUNTIME_DIR` (for libsecret keychain access), and `GOOGLE_CLOUD_PROJECT` (if set).
   - Set `GEMINI_CLI_TRUST_WORKSPACE="true"`
   - Set `GEMINI_CLI_NO_RELAUNCH="true"` (prevents wrapper process relaunch so `Child::kill()` directly kills the active Node worker on timeout)
   - Set `NO_BROWSER="true"` and `CI="true"` (prevents hanging on interactive browser launch if OAuth expires)
   - Explicitly `env_remove("GEMINI_API_KEY")` and `env_remove("GOOGLE_API_KEY")` so `gemini-cli` never accidentally switches from `oauth-personal` to paid API key billing.

---

## 7. Workspace `.gemini/settings.json` MCP Registration

`gemini-cli` reads `mcpServers` from `<workspace>/.gemini/settings.json` (or `~/.gemini/settings.json`), **not** from `~/.gemini/config/mcp_config.json`. Provision `/home/pedrogoncalves/projects/knowledge-base/.gemini/settings.json` with `trust: true` on each server:

```json
{
  "mcpServers": {
    "akashic": {
      "command": "/home/pedrogoncalves/projects/akashic-records/target/release/akashic",
      "args": ["mcp"],
      "cwd": "/home/pedrogoncalves/projects/knowledge-base",
      "timeout": 30000,
      "trust": true
    },
    "alexandria": {
      "command": "/home/pedrogoncalves/projects/alexandria/target/release/alexandria",
      "args": ["mcp"],
      "cwd": "/home/pedrogoncalves/projects/knowledge-base",
      "timeout": 30000,
      "trust": true
    },
    "motojira-mcp": {
      "command": "/home/pedrogoncalves/projects/motojira/mcp-server/bin/motojira-mcp",
      "args": [],
      "timeout": 60000,
      "trust": true
    },
    "gerrit-mcp": {
      "command": "uv",
      "args": [
        "--directory",
        "/home/pedrogoncalves/projects/gerrit-mcp/gerrit-mcp-server",
        "run",
        "python",
        "-m",
        "gerrit_mcp_server.main"
      ],
      "timeout": 60000,
      "trust": true
    }
  }
}
```
