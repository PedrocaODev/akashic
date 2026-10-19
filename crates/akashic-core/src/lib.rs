pub use akashic_daemon as daemon;
pub use akashic_embedding as embedding;
pub use akashic_engine as engine;
pub use akashic_lint as lint;
pub use akashic_rca as rca;
pub use akashic_sanitize as sanitize;
pub use akashic_vault as vault;

pub mod mcp;

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug, PartialEq)]
#[command(
    name = "akashic",
    version,
    about = "Unified Akashic Knowledge Graph, Code Graph, and RCA CLI"
)]
pub struct Cli {
    #[arg(short, long, global = true, help = "Path to configuration file")]
    pub config: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum Commands {
    /// Vault management and mutation operations
    Vault {
        #[command(subcommand)]
        command: VaultCommands,
    },
    /// Vault schema and custom field rule verification
    Lint(LintArgs),
    /// Code Graph extraction, analysis, and visualization
    Graph {
        #[command(subcommand)]
        command: GraphCommands,
    },
    /// Local-Host RCA Conductor investigation and triage
    Rca {
        #[command(subcommand)]
        command: RcaCommands,
    },
    /// Gerrit event streaming and background reconciliation
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
    /// Embedding sidecar lifecycle and status diagnostics
    Embedding {
        #[command(subcommand)]
        command: EmbeddingCommands,
    },
    /// Code and log sanitization filter testing
    Sanitize(SanitizeArgs),
    /// Run Model Context Protocol (MCP) server over standard I/O
    Mcp(McpArgs),
}

// ---------------------------------------------------------------------------
// Vault Command Specs
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug, PartialEq)]
pub enum VaultCommands {
    Init(VaultInitArgs),
    Query(VaultQueryArgs),
    Inspect(VaultInspectArgs),
    Plan(VaultPlanArgs),
    Apply(VaultApplyArgs),
    Reindex(VaultReindexArgs),
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct VaultInitArgs {
    #[arg(long)]
    pub path: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct VaultQueryArgs {
    #[arg(long, default_value = "current")]
    pub mode: String,
    #[arg(long)]
    pub scope: Option<String>,
    #[arg(long)]
    pub note_id: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct VaultInspectArgs {
    pub note_id: String,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct VaultPlanArgs {
    #[arg(long)]
    pub out: Option<String>,
    #[arg(long)]
    pub path: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct VaultApplyArgs {
    #[arg(long)]
    pub plan: Option<String>,
    #[arg(long)]
    pub path: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct VaultReindexArgs {
    #[arg(long)]
    pub path: Option<String>,
}

// ---------------------------------------------------------------------------
// Lint Command Specs
// ---------------------------------------------------------------------------

#[derive(Args, Debug, PartialEq, Default)]
pub struct LintArgs {
    #[arg(long, alias = "path")]
    pub vault: Option<String>,
    #[arg(long)]
    pub strict: bool,
    #[arg(long)]
    pub json: bool,
}

// ---------------------------------------------------------------------------
// Graph Command Specs
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug, PartialEq)]
pub enum GraphCommands {
    Index(GraphIndexArgs),
    Reconcile(GraphReconcileArgs),
    Query(GraphQueryArgs),
    Path(GraphPathArgs),
    Explain(GraphExplainArgs),
    Communities(GraphCommunitiesArgs),
    GodNodes(GraphGodNodesArgs),
    Surprising(GraphSurprisingArgs),
    BlastRadius(GraphBlastRadiusArgs),
    Report(GraphReportArgs),
    Wiki(GraphWikiArgs),
    Discover(GraphDiscoverArgs),
    Symbol(GraphSymbolArgs),
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphIndexArgs {
    #[arg(long)]
    pub commit: Option<String>,
    #[arg(long)]
    pub repo_path: Option<String>,
    #[arg(long)]
    pub repo_name: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphReconcileArgs {
    #[arg(long)]
    pub old_rev: Option<String>,
    #[arg(long)]
    pub new_rev: Option<String>,
    #[arg(long)]
    pub repo_path: Option<String>,
    #[arg(long)]
    pub repo_name: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphQueryArgs {
    pub query: String,
    #[arg(long)]
    pub repo_name: Option<String>,
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphPathArgs {
    pub source: String,
    pub target: String,
    #[arg(long)]
    pub repo_name: Option<String>,
    #[arg(long)]
    pub max_hops: Option<usize>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphExplainArgs {
    pub symbol: String,
    #[arg(long)]
    pub repo_name: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphCommunitiesArgs {
    #[arg(long)]
    pub repo_name: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphGodNodesArgs {
    #[arg(long)]
    pub repo_name: Option<String>,
    #[arg(long)]
    pub top_n: Option<usize>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphSurprisingArgs {
    #[arg(long)]
    pub repo_name: Option<String>,
    #[arg(long)]
    pub top_n: Option<usize>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphBlastRadiusArgs {
    pub symbol_or_diff: String,
    #[arg(long)]
    pub repo_name: Option<String>,
    #[arg(long)]
    pub max_depth: Option<usize>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphReportArgs {
    #[arg(long, default_value = "md")]
    pub format: String,
    #[arg(long)]
    pub repo_name: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphWikiArgs {
    #[arg(long)]
    pub repo_name: Option<String>,
    #[arg(long)]
    pub out_dir: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphDiscoverArgs {
    #[arg(long)]
    pub path: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphSymbolArgs {
    pub query: String,
}

// ---------------------------------------------------------------------------
// RCA Command Specs
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug, PartialEq)]
pub enum RcaCommands {
    Run(RcaRunArgs),
    Resume(RcaResumeArgs),
    Status(RcaStatusArgs),
    Symbolicate(RcaSymbolicateArgs),
    Crystallize(RcaCrystallizeArgs),
    Supersede(RcaSupersedeArgs),
    Start(RcaStartArgs),
    Triage,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct RcaRunArgs {
    #[arg(long)]
    pub app_id: Option<String>,
    #[arg(long)]
    pub raw_log: Option<String>,
    #[arg(long)]
    pub log_file: Option<String>,
    #[arg(long)]
    pub jira_key: Option<String>,
    #[arg(long)]
    pub trigger_id: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct RcaResumeArgs {
    pub investigation_id: String,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct RcaStatusArgs {
    pub investigation_id: String,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct RcaSymbolicateArgs {
    #[arg(long)]
    pub app_id: Option<String>,
    #[arg(long)]
    pub raw_log: Option<String>,
    #[arg(long)]
    pub log_file: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct RcaCrystallizeArgs {
    #[arg(long)]
    pub run_id: String,
    #[arg(long)]
    pub summary: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct RcaSupersedeArgs {
    #[arg(long)]
    pub old_id: String,
    #[arg(long)]
    pub new_id: String,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct RcaStartArgs {
    pub trigger_id: String,
}

// ---------------------------------------------------------------------------
// Daemon Command Specs
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug, PartialEq)]
pub enum DaemonCommands {
    Start,
    Stop,
    Status,
    Catchup(DaemonCatchupArgs),
    Discover(DaemonDiscoverArgs),
    Run,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct DaemonCatchupArgs {
    #[arg(long)]
    pub stream: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct DaemonDiscoverArgs {
    #[arg(long)]
    pub workspace: Option<String>,
}

// ---------------------------------------------------------------------------
// Embedding Command Specs
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug, PartialEq)]
pub enum EmbeddingCommands {
    Start,
    Stop,
    Status,
    Benchmark(EmbeddingBenchmarkArgs),
    StartSidecar,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct EmbeddingBenchmarkArgs {
    #[arg(long, default_value = "10")]
    pub count: usize,
}

// ---------------------------------------------------------------------------
// Sanitize Command Specs
// ---------------------------------------------------------------------------

#[derive(Args, Debug, PartialEq, Default)]
pub struct SanitizeArgs {
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(short, long)]
    pub input: Option<String>,
    #[command(subcommand)]
    pub command: Option<SanitizeCommands>,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum SanitizeCommands {
    TestCode(SanitizeTestCodeArgs),
    TestLog(SanitizeTestLogArgs),
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct SanitizeTestCodeArgs {
    #[arg(short, long)]
    pub input: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct SanitizeTestLogArgs {
    #[arg(short, long)]
    pub input: Option<String>,
}

// ---------------------------------------------------------------------------
// MCP Command Specs
// ---------------------------------------------------------------------------

#[derive(Args, Debug, PartialEq, Default)]
pub struct McpArgs {
    #[arg(long)]
    pub vault: Option<String>,
    #[arg(long)]
    pub workspace: Option<String>,
}

// ---------------------------------------------------------------------------
// Unit tests for CLI parsing
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_subcommands_parsing() {
        let cli = Cli::try_parse_from(["akashic", "vault", "query", "--mode", "lineage", "--scope", "repo=demo"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Vault {
                command: VaultCommands::Query(args)
            } if args.mode == "lineage" && args.scope.as_deref() == Some("repo=demo")
        ));

        let cli = Cli::try_parse_from(["akashic", "vault", "inspect", "INC-123"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Vault {
                command: VaultCommands::Inspect(args)
            } if args.note_id == "INC-123"
        ));

        let cli = Cli::try_parse_from(["akashic", "vault", "plan", "--out", "plan.json"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Vault {
                command: VaultCommands::Plan(args)
            } if args.out.as_deref() == Some("plan.json")
        ));

        let cli = Cli::try_parse_from(["akashic", "vault", "apply", "--plan", "plan.json"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Vault {
                command: VaultCommands::Apply(args)
            } if args.plan.as_deref() == Some("plan.json")
        ));

        let cli = Cli::try_parse_from(["akashic", "vault", "reindex"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Vault {
                command: VaultCommands::Reindex(VaultReindexArgs::default())
            }
        );
    }

    #[test]
    fn test_lint_subcommand_parsing() {
        let cli = Cli::try_parse_from(["akashic", "lint"]).unwrap();
        assert!(matches!(cli.command, Commands::Lint(args) if args.vault.is_none()));

        let cli = Cli::try_parse_from(["akashic", "lint", "--vault", "vault", "--strict", "--json"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Lint(args) if args.vault.as_deref() == Some("vault") && args.strict && args.json
        ));

        let cli = Cli::try_parse_from(["akashic", "lint", "--path", "vault"]).unwrap();
        assert!(matches!(cli.command, Commands::Lint(args) if args.vault.as_deref() == Some("vault")));
    }

    #[test]
    fn test_graph_subcommands_parsing() {
        let cli = Cli::try_parse_from(["akashic", "graph", "index", "--commit", "abcdef1"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Graph {
                command: GraphCommands::Index(args)
            } if args.commit.as_deref() == Some("abcdef1")
        ));

        let cli = Cli::try_parse_from(["akashic", "graph", "reconcile", "--old-rev", "a", "--new-rev", "b"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Graph {
                command: GraphCommands::Reconcile(args)
            } if args.old_rev.as_deref() == Some("a") && args.new_rev.as_deref() == Some("b")
        ));

        let cli = Cli::try_parse_from(["akashic", "graph", "query", "MySymbol"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Graph {
                command: GraphCommands::Query(args)
            } if args.query == "MySymbol"
        ));

        let cli = Cli::try_parse_from(["akashic", "graph", "path", "SymA", "SymB"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Graph {
                command: GraphCommands::Path(args)
            } if args.source == "SymA" && args.target == "SymB"
        ));

        let cli = Cli::try_parse_from(["akashic", "graph", "blast-radius", "MySymbol"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Graph {
                command: GraphCommands::BlastRadius(args)
            } if args.symbol_or_diff == "MySymbol"
        ));

        let cli = Cli::try_parse_from(["akashic", "graph", "communities"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Graph {
                command: GraphCommands::Communities(GraphCommunitiesArgs::default())
            }
        );

        let cli = Cli::try_parse_from(["akashic", "graph", "god-nodes"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Graph {
                command: GraphCommands::GodNodes(GraphGodNodesArgs::default())
            }
        );

        let cli = Cli::try_parse_from(["akashic", "graph", "report", "--format", "wiki"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Graph {
                command: GraphCommands::Report(args)
            } if args.format == "wiki"
        ));
    }

    #[test]
    fn test_rca_subcommands_parsing() {
        let cli = Cli::try_parse_from(["akashic", "rca", "run", "--app-id", "com.demo"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Rca {
                command: RcaCommands::Run(args)
            } if args.app_id.as_deref() == Some("com.demo")
        ));

        let cli = Cli::try_parse_from(["akashic", "rca", "start", "T1001"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Rca {
                command: RcaCommands::Start(args)
            } if args.trigger_id == "T1001"
        ));

        let cli = Cli::try_parse_from(["akashic", "rca", "resume", "INV-42"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Rca {
                command: RcaCommands::Resume(args)
            } if args.investigation_id == "INV-42"
        ));

        let cli = Cli::try_parse_from(["akashic", "rca", "status", "INV-42"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Rca {
                command: RcaCommands::Status(args)
            } if args.investigation_id == "INV-42"
        ));

        let cli = Cli::try_parse_from(["akashic", "rca", "crystallize", "--run-id", "RUN-1"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Rca {
                command: RcaCommands::Crystallize(args)
            } if args.run_id == "RUN-1"
        ));
    }

    #[test]
    fn test_daemon_subcommands_parsing() {
        let cli = Cli::try_parse_from(["akashic", "daemon", "start"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Daemon {
                command: DaemonCommands::Start
            }
        );

        let cli = Cli::try_parse_from(["akashic", "daemon", "status"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Daemon {
                command: DaemonCommands::Status
            }
        );

        let cli = Cli::try_parse_from(["akashic", "daemon", "catchup", "--stream", "default"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Daemon {
                command: DaemonCommands::Catchup(args)
            } if args.stream.as_deref() == Some("default")
        ));
    }

    #[test]
    fn test_embedding_subcommands_parsing() {
        let cli = Cli::try_parse_from(["akashic", "embedding", "status"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Embedding {
                command: EmbeddingCommands::Status
            }
        );

        let cli = Cli::try_parse_from(["akashic", "embedding", "start"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Embedding {
                command: EmbeddingCommands::Start
            }
        );

        let cli = Cli::try_parse_from(["akashic", "embedding", "benchmark", "--count", "50"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Embedding {
                command: EmbeddingCommands::Benchmark(args)
            } if args.count == 50
        ));
    }

    #[test]
    fn test_sanitize_subcommands_parsing() {
        let cli = Cli::try_parse_from(["akashic", "sanitize", "--profile", "code", "--input", "code snippet"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Sanitize(args) if args.profile.as_deref() == Some("code") && args.input.as_deref() == Some("code snippet")
        ));

        let cli = Cli::try_parse_from(["akashic", "sanitize", "test-code", "--input", "code snippet"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Sanitize(args) if matches!(&args.command, Some(SanitizeCommands::TestCode(tc)) if tc.input.as_deref() == Some("code snippet"))
        ));
    }

    #[test]
    fn test_mcp_subcommand_parsing() {
        let cli = Cli::try_parse_from(["akashic", "mcp"]).unwrap();
        assert_eq!(cli.command, Commands::Mcp(McpArgs::default()));

        let cli = Cli::try_parse_from(["akashic", "mcp", "--vault", "vault_dir", "--workspace", "ws_dir"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Mcp(McpArgs {
                vault: Some("vault_dir".to_string()),
                workspace: Some("ws_dir".to_string()),
            })
        );
    }
}
