---
id: "02-gemini-cli-headless-contract"
title: "Gemini CLI Headless Invocation and Local-Host Conductor Integration Contract"
label: "wayfinder:research"
type: "research"
mode: "AFK"
parent: "wayfinder-map"
blocked_by: []
assignee: "pedrogoncalves"
status: closed
resolution:
  date: "2026-10-15"
  findings: "../research/02-gemini-cli-headless-contract.md"
  gist: "Invoke gemini v0.64.0 via tokio::process::Command with stdin payload + -p directive, --output-format json --approval-mode default --skip-trust -e none --allowed-mcp-server-names ... --policy stage4-readonly.toml, and env GEMINI_CLI_TRUST_WORKSPACE=true, GEMINI_CLI_NO_RELAUNCH=true, NO_BROWSER=true; parse stdout on exit 0 and stderr on fatal exit codes."
---

# Gemini CLI Headless Invocation and Local-Host Conductor Integration Contract

## Question

What are the exact non-interactive CLI flags, prompt/stdin piping conventions, structured JSON output schemas (`--output-format json`), MCP tool attachment flags (`akashic`, `alexandria`, `motojira-mcp`, `gerrit-mcp`), and local enterprise OAuth session reuse mechanics for invoking `gemini-cli` (`gemini` v0.64.0) headlessly as a subprocess from the Rust **Local-Host Conductor** during Stage 4 Hypothesis Generation and Adversarial Falsification without provisioning API keys or exposing credentials to containers?

## Resolution

Full research findings are documented in [02-gemini-cli-headless-contract.md](../research/02-gemini-cli-headless-contract.md).

### Key Findings Summary
1. **CLI Flags & Stdin Protocol**: Invoke `gemini` with `--output-format json --approval-mode default --skip-trust -e none --allowed-mcp-server-names akashic,alexandria,motojira-mcp,gerrit-mcp --policy <path/to/stage4-readonly.toml> -m <model> -p "<directive>"` while piping the sanitized crash/evidence bundle to `stdin` and closing `ChildStdin` immediately (avoiding the 500ms `readStdin` timeout).
2. **Why `--approval-mode plan` Is Unsuitable**: `plan.toml` denies `*` at `1.040`, sets MCP `readOnlyHint` tools to `ask_user` (which converts to `DENY` in headless non-interactive mode), and `--allowed-mcp-server-names` (`4.100`) only applies to `nonPlanModes`. Instead, use `--approval-mode default` paired with a Tier-4 `--policy stage4-readonly.toml` (`priority = 500` default deny `*`, `800` allow MCP servers, `950` deny mutating MCP tools).
3. **Output Envelope & Stream Asymmetry**:
   - On exit code `0`, `stdout` emits `{ "session_id", "response", "stats", "warnings"? }` where `response` is an ANSI-stripped string requiring second-pass `serde_json` deserialization into `RcaHypothesisOutput` / `RcaFalsifierOutput`.
   - On fatal errors (exit codes `1`, `41`, `42`, `52`, `53`, `130`), `handleError()` writes `{ "session_id", "error": { "type", "message", "code" } }` to **`stderr`** while leaving `stdout` empty.
4. **Enterprise OAuth (`oauth-personal`) & Environment Contract**: Reuses `~/.gemini/settings.json`, `~/.gemini/oauth_creds.json`, and `GOOGLE_CLOUD_PROJECT` in `~/.gemini/.env`. The subprocess must set `GEMINI_CLI_TRUST_WORKSPACE=true` (so `findEnvFile()` loads `~/.gemini/.env`), `GEMINI_CLI_NO_RELAUNCH=true` (so `Child::kill()` terminates the worker directly), `NO_BROWSER=true`, and `CI=true`, while stripping `GEMINI_API_KEY` and `GOOGLE_API_KEY`.
5. **MCP Server Discovery**: `gemini-cli` reads `mcpServers` from `.gemini/settings.json` (not Antigravity's `~/.gemini/config/mcp_config.json`).
