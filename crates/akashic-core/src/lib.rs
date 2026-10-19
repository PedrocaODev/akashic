pub use akashic_daemon as daemon;
pub use akashic_embedding as embedding;
pub use akashic_engine as engine;
pub use akashic_lint as lint;
pub use akashic_rca as rca;
pub use akashic_sanitize as sanitize;
pub use akashic_vault as vault;

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
    Sanitize {
        #[command(subcommand)]
        command: SanitizeCommands,
    },
    /// Run Model Context Protocol (MCP) server over standard I/O
    Mcp,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum VaultCommands {
    Query(VaultQueryArgs),
    Plan(VaultPlanArgs),
    Apply(VaultApplyArgs),
    Reindex,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct VaultQueryArgs {
    #[arg(long, default_value = "current")]
    pub mode: String,
    #[arg(long)]
    pub scope: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct VaultPlanArgs {
    #[arg(long)]
    pub out: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct VaultApplyArgs {
    #[arg(long)]
    pub plan: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct LintArgs {
    #[arg(long)]
    pub path: Option<String>,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum GraphCommands {
    Index(GraphIndexArgs),
    Symbol(GraphSymbolArgs),
    BlastRadius(GraphBlastRadiusArgs),
    Communities,
    GodNodes,
    Report(GraphReportArgs),
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphIndexArgs {
    #[arg(long)]
    pub commit: Option<String>,
}

#[derive(Args, Debug, PartialEq)]
pub struct GraphSymbolArgs {
    pub query: String,
}

#[derive(Args, Debug, PartialEq)]
pub struct GraphBlastRadiusArgs {
    pub symbol_or_diff: String,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct GraphReportArgs {
    #[arg(long, default_value = "md")]
    pub format: String,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum RcaCommands {
    Start(RcaStartArgs),
    Resume(RcaResumeArgs),
    Status(RcaStatusArgs),
    Triage,
}

#[derive(Args, Debug, PartialEq)]
pub struct RcaStartArgs {
    pub trigger_id: String,
}

#[derive(Args, Debug, PartialEq)]
pub struct RcaResumeArgs {
    pub investigation_id: String,
}

#[derive(Args, Debug, PartialEq)]
pub struct RcaStatusArgs {
    pub investigation_id: String,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum DaemonCommands {
    Run,
    Status,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum EmbeddingCommands {
    Status,
    StartSidecar,
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
                command: VaultCommands::Reindex
            }
        );
    }

    #[test]
    fn test_lint_subcommand_parsing() {
        let cli = Cli::try_parse_from(["akashic", "lint"]).unwrap();
        assert!(matches!(cli.command, Commands::Lint(args) if args.path.is_none()));

        let cli = Cli::try_parse_from(["akashic", "lint", "--path", "vault"]).unwrap();
        assert!(matches!(cli.command, Commands::Lint(args) if args.path.as_deref() == Some("vault")));
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

        let cli = Cli::try_parse_from(["akashic", "graph", "symbol", "MySymbol"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Graph {
                command: GraphCommands::Symbol(args)
            } if args.query == "MySymbol"
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
                command: GraphCommands::Communities
            }
        );

        let cli = Cli::try_parse_from(["akashic", "graph", "god-nodes"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Graph {
                command: GraphCommands::GodNodes
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

        let cli = Cli::try_parse_from(["akashic", "rca", "triage"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Rca {
                command: RcaCommands::Triage
            }
        );
    }

    #[test]
    fn test_daemon_subcommands_parsing() {
        let cli = Cli::try_parse_from(["akashic", "daemon", "run"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Daemon {
                command: DaemonCommands::Run
            }
        );

        let cli = Cli::try_parse_from(["akashic", "daemon", "status"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Daemon {
                command: DaemonCommands::Status
            }
        );
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

        let cli = Cli::try_parse_from(["akashic", "embedding", "start-sidecar"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Embedding {
                command: EmbeddingCommands::StartSidecar
            }
        );
    }

    #[test]
    fn test_sanitize_subcommands_parsing() {
        let cli = Cli::try_parse_from(["akashic", "sanitize", "test-code", "--input", "code snippet"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Sanitize {
                command: SanitizeCommands::TestCode(args)
            } if args.input.as_deref() == Some("code snippet")
        ));

        let cli = Cli::try_parse_from(["akashic", "sanitize", "test-log", "--input", "log line"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Sanitize {
                command: SanitizeCommands::TestLog(args)
            } if args.input.as_deref() == Some("log line")
        ));
    }

    #[test]
    fn test_mcp_subcommand_parsing() {
        let cli = Cli::try_parse_from(["akashic", "mcp"]).unwrap();
        assert_eq!(cli.command, Commands::Mcp);
    }
}
