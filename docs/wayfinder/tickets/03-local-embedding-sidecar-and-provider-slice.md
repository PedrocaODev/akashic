# 03 — Local Embedding Sidecar and Provider Slice

**What to build:** Implement the `akashic-embedding` crate providing the `EmbeddingProvider` trait, an HTTP client supporting OpenAI-compatible local endpoints, and a robust sidecar process manager for spawning, health-checking, and terminating a local `llama.cpp` or ONNX sidecar. Implement graceful offline degradation so callers can detect availability and seamlessly fall back to non-vector operations when the sidecar is offline. Wire `akashic embedding status` and `akashic embedding start-sidecar` CLI subcommands.

**Blocked by:** 01 — Three-Repo Merge and Eight-Crate Workspace Prefactor

**Status:** done

- [x] `EmbeddingProvider` trait defines async `embed_batch`, `dimension`, and `is_available` methods.
- [x] HTTP client connects to local OpenAI-compatible endpoints (`/v1/embeddings`), handling batching, timeouts, and error responses cleanly.
- [x] Process sidecar manager launches and monitors a local embedding model process (e.g. `llama.cpp` server) with active HTTP health probes and clean shutdown on SIGINT/SIGTERM.
- [x] Graceful fallback API reports offline status without throwing fatal errors, enabling callers to downgrade to lexical/AST operations.
- [x] CLI subcommands `akashic embedding status` and `akashic embedding start-sidecar` report real-time health and manage process states.
