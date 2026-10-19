# 02 — Profile-Driven Sanitization Slice

**What to build:** Implement the complete sanitization engine in `akashic-sanitize` providing two distinct deterministic profiles: `CodeProfile` (syntax-preserving redaction of API keys, credentials, and secrets in source code chunks prior to AST indexing or embedding) and `LogProfile` (Format-Preserving Semantic Pseudonymization of PII, IP addresses, MAC addresses, and telephone numbers in crash logs and logcat dumps prior to RCA reasoning or Vault crystallization). Wire CLI subcommands (`akashic sanitize test-code`, `akashic sanitize test-log`) to demo and verify end-to-end sanitization behavior.

**Blocked by:** 01 — Three-Repo Merge and Eight-Crate Workspace Prefactor

**Status:** ready-for-agent

- [ ] `CodeProfile` scrubs API keys, private tokens, passwords, and credentials while retaining valid AST syntax (matching string literal quotes and identifier boundaries).
- [ ] `LogProfile` deterministically replaces sensitive PII (phones, MAC addresses, IPv4/IPv6, serial numbers) with typed syntax-preserving pseudonyms using a per-session cryptographic salt, preserving log column alignment.
- [ ] `akashic-sanitize` exposes clean programmatic APIs consumable by `akashic-engine`, `akashic-rca`, and `akashic-vault`.
- [ ] CLI subcommands `akashic sanitize test-code` and `akashic sanitize test-log` process fixture streams and output sanitized buffers.
- [ ] Unit and property-based test suites verify zero secret leaks, parse invariance, and deterministic pseudonymization idempotency.
