# 09 — Gerrit Stream Daemon, Debounce, and Auto-Discovery Slice

**What to build:** Implement background synchronization, streaming, and repository auto-enrollment in `akashic-daemon`. Monitor the Gerrit SSH event stream (`stream-events`), filter out self-trigger loop events from `akashic/*`, batch commit notifications using an adaptive sliding-window debounce (`30s` quiet window / `300s` maximum cap), maintain a persistent event watermark in `.akashic/rca_state.db` with catch-up query replay on reconnect, and implement Clone-to-Onboard Discovery using filesystem events (`notify`) and 5-pass Gradle/Manifest static analysis to populate the `Package-to-Repository Index`.

**Blocked by:** 07 — Graph Communities, God Nodes, Blast Radius, and Report Slice, 08 — Host-Native RCA Conductor, Symbolication, and Crystallization Slice

**Status:** done

- [x] Gerrit SSH event stream watcher captures patchset-created and change-merged events with automatic reconnect on network drops.
- [x] Self-trigger loop drop filter ignores events originating from the `akashic/*` knowledge repository.
- [x] Adaptive sliding debounce groups commits within a `30s` quiet window up to a `300s` maximum cap, dispatching batched incremental indexing to `akashic-engine`.
- [x] Persistent watermark tracking in `.akashic/rca_state.db` triggers catch-up replay queries (`gerrit query after:<watermark>`) after disconnections or service restarts.
- [x] Clone-to-Onboard Discovery detects newly cloned Git repositories, extracts Android package identifiers across Gradle and Manifest files via a 5-pass extractor, and auto-provisions Code Graph and Vault partitions.
- [x] CLI commands `akashic daemon run` and `akashic daemon status` manage background execution and expose monitoring diagnostics.
