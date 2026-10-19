# 7. Profile-Driven Sanitization and Format-Preserving Pseudonymization

Date: 2026-10-19

## Status

Accepted

## Context

OEM Android development environments and crash diagnostic reports frequently contain sensitive proprietary information, signing keys, hardware identifiers (IMEI, MAC addresses), IP addresses, and user personally identifiable information (PII). Ingesting raw logs and source code into embedding stores or vector databases risks compliance violations and credential leakage. Conversely, naive redaction (e.g. replacing tokens with blank spaces or generic `[REDACTED]` markers) destroys AST syntax validity in code and breaks tabular alignment, regex parsers, and anomaly entropy in logcat output.

## Decision

Consolidate all redaction and anonymization logic into `akashic-sanitize` using two specialized sanitization profiles:

1. **`CodeProfile` (Syntax-Preserving Code Sanitization)**:
   Scans source code buffers prior to AST embedding and vector storage. Replaces API secrets, private keys, bearer tokens, and internal server endpoints with syntax-equivalent placeholder literals (e.g., maintaining quote types and valid identifier formats). This eliminates credential leakage while ensuring Tree-sitter AST parsers process valid syntax trees.

2. **`LogProfile` (Format-Preserving Semantic Pseudonymization)**:
   Scans diagnostic logcat feeds, Bug2Go packages, tombstones, and incident dumps prior to RCA reasoning or Vault storage. Replaces sensitive fields with deterministic, grammar-aware typed pseudonyms (such as `<PHONE:US_VALID_10DIGIT>`, `<MAC:ADDR_VALID>`, `<IPV4:INTERNAL>`). This preserves fixed-width column alignments, preserves multi-occurrence correlation across log lines, and prevents regex or parser crashes in downstream diagnostic tooling.

## Consequences

### Positive
- Strict data compliance and credential safety across all vector databases, embeddings, and crystallized Vault artifacts.
- Preserves downstream parser robustness: code remains valid AST syntax, and logs maintain structured layout and cross-line correlation.
- Completely isolated in `akashic-sanitize`, reusable by both `akashic-engine` and `akashic-rca`.

### Negative
- Regular expression and semantic token replacement introduces modest latency during high-volume log ingestion.
