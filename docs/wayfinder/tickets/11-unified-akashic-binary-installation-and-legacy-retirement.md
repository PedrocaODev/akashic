# 11 — Unified `akashic` Binary Installation and Legacy Binary Retirement

**What to build:** Install the unified `akashic` binary from `crates/akashic-core` into `~/.cargo/bin/akashic`, retire all legacy CLI entrypoints (`graphify`, `graphify-mcp`, `rca-cli`, `alexandria`, `akashic-records`) from `~/.local/bin` and `~/.cargo/bin`, and verify end-to-end execution of all 8 domain subcommands (`vault`, `lint`, `graph`, `rca`, `daemon`, `embedding`, `sanitize`, `mcp`) against `/home/pedrogoncalves/second-brain` and local workspace repositories.

**Blocked by:** none

**Status:** done

- [x] `cargo install --path crates/akashic-core --force` installs the unified `akashic` binary into `/home/pedrogoncalves/.cargo/bin/akashic`.
- [x] Legacy standalone binaries and wrapper scripts (`~/.local/bin/graphify`, `~/.local/bin/graphify-mcp`, `~/.cargo/bin/rca-cli`, `~/.cargo/bin/alexandria`) are removed from `PATH`.
- [x] `akashic --help` and each domain subcommand (`vault`, `lint`, `graph`, `rca`, `daemon`, `embedding`, `sanitize`, `mcp`) execute cleanly from the installed binary.
- [x] Running `akashic lint --vault /home/pedrogoncalves/second-brain` and `akashic graph index --repo-path /home/pedrogoncalves/projects/knowledge-base` succeeds using the installed binary and populates `.akashic/code_graph.db`.
