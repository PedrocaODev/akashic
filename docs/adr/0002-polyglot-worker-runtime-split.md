# Polyglot Worker Runtime Split

We separate compute and parsing tasks from LLM reasoning tasks across two specialized worker runtimes:

- **Rust Native Services**: Rust workers handle heavy data processing, native crash symbolication (`android_symbolicator_worker` utilizing a local NVMe-backed Symbol Store LRU cache mapping build fingerprints to unstripped ELF binaries and R8 Proguard mapping files), logcat parsing and slicing, BM25 lexical window scoring (`bm25_log_window_scorer`), format-preserving semantic pseudonymization (`format_preserving_sanitizer`), and Akashic graph verification (`akashic lint` via `akashic-cli`), ensuring memory safety, deterministic throughput, and zero runtime panics.
- **Python Workers**: Python workers handle Conductor workflow orchestration tasks, LiteLLM gateway integration, Alexandria MCP queries, external service integrations (MotoJira, Gerrit REST/SSH), and two-stage multi-agent reasoning prompts.

This preserves high-performance binary compute while leveraging Python's distributed orchestration and AI ecosystem without compromise.
