# 9. Gerrit Stream Adaptive Debounce, Watermark Catch-Up, and Auto-Discovery

Date: 2026-10-19

## Status

Accepted

## Context

Enterprise Android development environments merge hundreds of Gerrit change sets per day. Processing every Gerrit event immediately causes thrashing, redundant graph updates during batch merges, and unneeded CPU load. Furthermore, SSH network drops can cause missed event notifications, creating graph drift if missed commits are never ingested. Finally, developers frequently clone new Android repositories into their workspace; requiring manual configuration files for each repository hinders adoption.

## Decision

Implement background synchronization and auto-enrollment in `akashic-daemon` with three key capabilities:

1. **Sliding-Window Adaptive Debounce**:
   Listen to the continuous Gerrit SSH JSON event stream. Group incoming merge and patchset events within an adaptive sliding window: a `30s` quiet window with a hard maximum cap of `300s`. Multiple commits touching the same repository within the quiet window are batched into a single incremental Code Graph reconciliation run.

2. **Persistent Watermark Catch-Up**:
   Persist the high-watermark timestamp of processed Gerrit events in `.akashic/rca_state.db`. If the SSH connection drops, the workstation sleeps, or the daemon restarts, `akashic-daemon` automatically executes a catch-up query (`gerrit query after:<watermark>`) to reconcile any missed change sets before resuming real-time streaming.

3. **Clone-to-Onboard Repository Discovery**:
   Monitor the workspace root directory using the native `notify` filesystem watcher. When an Android repository is cloned into the workspace, the daemon automatically:
   - Detects the Git directory and identifies its Gerrit remote.
   - Parses `AndroidManifest.xml` and Gradle files to discover Android package identifiers (`applicationId`, `namespace`, `package`).
   - Registers the package-to-repository index and provisions its `Code Graph` and Vault partition without manual user intervention.

## Consequences

### Positive
- Batched commit processing prevents CPU thrashing during large multi-project Gerrit merges.
- Guarantees zero missed commits across SSH reconnects, daemon restarts, or system sleep.
- Zero-friction onboarding for new Android projects via automatic filesystem discovery.

### Negative
- Adaptive debounce introduces an intentional processing latency of up to 30–300 seconds before merged code reflects in the Code Graph.
