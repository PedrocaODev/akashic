pub use akashic_daemon as daemon;
pub use akashic_embedding as embedding;
pub use akashic_engine as engine;
pub use akashic_lint as lint;
pub use akashic_rca as rca;
pub use akashic_sanitize as sanitize;
pub use akashic_vault as vault;

pub mod mcp;

use std::path::{Path, PathBuf};
use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};

pub const DEFAULT_CONFIG_PATH: &str = "config/akashic.toml";
pub const DEFAULT_REPO_NAME: &str = "default";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SanitizeConfig {
    pub enabled: bool,
    pub custom_patterns: Vec<String>,
}

impl Default for SanitizeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            custom_patterns: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AkashicConfig {
    pub default_repo: Option<String>,
    pub vault: vault::VaultConfig,
    pub lint: lint::LintConfig,
    pub embedding: embedding::EmbeddingConfig,
    pub engine: engine::EngineConfig,
    pub rca: rca::RcaConfig,
    pub daemon: daemon::DaemonConfig,
    pub sanitize: SanitizeConfig,
}

fn merge_toml_tables(base: &mut toml::Table, patch: toml::Table) {
    for (k, v) in patch {
        match (base.get_mut(&k), v) {
            (Some(toml::Value::Table(b_tab)), toml::Value::Table(p_tab)) => {
                merge_toml_tables(b_tab, p_tab);
            }
            (_, patch_val) => {
                base.insert(k, patch_val);
            }
        }
    }
}

impl AkashicConfig {
    pub fn from_toml_str(raw: &str) -> anyhow::Result<Self> {
        let default_val = toml::Value::try_from(Self::default())?;
        let mut default_tab = match default_val {
            toml::Value::Table(t) => t,
            _ => toml::Table::new(),
        };

        let custom_field_rules = lint::parse_custom_field_rules_from_toml(raw).unwrap_or_default();

        if !raw.trim().is_empty() {
            let patch_val: toml::Value = toml::from_str(raw)?;
            if let toml::Value::Table(patch_tab) = patch_val {
                merge_toml_tables(&mut default_tab, patch_tab);
            }
        }

        if let Some(toml::Value::Table(lint_tab)) = default_tab.get_mut("lint") {
            if let Some(toml::Value::Table(cf_tab)) = lint_tab.get_mut("custom_fields") {
                cf_tab.retain(|_, v| matches!(v, toml::Value::String(_)));
            }
        }

        let merged_val = toml::Value::Table(default_tab);
        let mut config: Self = merged_val.try_into()?;

        config.lint.custom_field_rules = custom_field_rules;
        for (k, v) in &config.lint.custom_fields {
            config.lint.custom_field_rules.entry(k.clone()).or_insert_with(|| {
                lint::CustomFieldRule {
                    field_type: v.clone(),
                    ..Default::default()
                }
            });
        }

        Ok(config)
    }

    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        Self::from_toml_str(&raw)
    }

    pub fn load(explicit_path: Option<&Path>) -> anyhow::Result<Self> {
        if let Some(path) = explicit_path {
            Self::from_file(path)
        } else {
            let default_path = Path::new(DEFAULT_CONFIG_PATH);
            if default_path.exists() {
                Self::from_file(default_path)
            } else {
                Ok(Self::default())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CliRuntimeContext {
    pub config_path: PathBuf,
    pub config: AkashicConfig,
}

impl CliRuntimeContext {
    pub fn load(explicit_config: Option<&Path>) -> anyhow::Result<Self> {
        let config = AkashicConfig::load(explicit_config)?;
        let config_path = explicit_config
            .unwrap_or_else(|| Path::new(DEFAULT_CONFIG_PATH))
            .to_path_buf();
        Ok(Self {
            config_path,
            config,
        })
    }

    pub fn from_config(config: AkashicConfig) -> Self {
        Self {
            config_path: PathBuf::from(DEFAULT_CONFIG_PATH),
            config,
        }
    }

    pub fn resolve_vault_path(&self, cli_override: Option<&Path>) -> PathBuf {
        cli_override
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(&self.config.vault.path))
    }

    pub fn resolve_vault_config(&self, cli_override: Option<&Path>) -> vault::VaultConfig {
        let mut cfg = self.config.vault.clone();
        if let Some(path) = cli_override {
            cfg.path = path.to_string_lossy().to_string();
        }
        cfg
    }

    pub fn resolve_repo_name<'a>(&'a self, cli_override: Option<&'a str>) -> &'a str {
        if let Some(r) = cli_override {
            r
        } else if let Some(ref r) = self.config.default_repo {
            r.as_str()
        } else {
            DEFAULT_REPO_NAME
        }
    }
}

// ---------------------------------------------------------------------------
// Vault Plan & Apply Orchestration Gate
// ---------------------------------------------------------------------------

pub fn collect_custom_field_rules(
    root: &Path,
    config: Option<&AkashicConfig>,
) -> std::collections::BTreeMap<String, lint::CustomFieldRule> {
    let mut rules = config
        .map(|c| {
            let mut r = c.lint.custom_field_rules.clone();
            for (k, v) in &c.lint.custom_fields {
                r.entry(k.clone()).or_insert_with(|| lint::CustomFieldRule {
                    field_type: v.clone(),
                    ..Default::default()
                });
            }
            r
        })
        .unwrap_or_default();

    let mut candidates = vec![
        root.join("akashic.toml"),
        root.join("config/akashic.toml"),
    ];
    if let Some(parent) = root.parent() {
        candidates.push(parent.join("akashic.toml"));
        candidates.push(parent.join("config/akashic.toml"));
    }

    for candidate in candidates {
        if candidate.exists() && candidate.is_file() {
            if let Ok(raw) = std::fs::read_to_string(&candidate) {
                if let Ok(file_rules) = lint::parse_custom_field_rules_from_toml(&raw) {
                    for (k, rule) in file_rules {
                        rules.entry(k).or_insert(rule);
                    }
                    break;
                }
            }
        }
    }

    rules
}

pub fn vault_apply_plan_with_config(
    root: &Path,
    plan: &vault::migrator::MigrationPlan,
    config: Option<&AkashicConfig>,
) -> anyhow::Result<usize> {
    let count = vault::migrator::apply_plan_with_validator(root, plan, |preview_graph| {
        let custom_rules = collect_custom_field_rules(root, config);
        let report = lint::lint_vault_with_rules(preview_graph, &custom_rules);
        if !report.valid || report.error_count > 0 {
            let diagnostics: Vec<String> = report
                .issues
                .iter()
                .filter(|i| i.severity == lint::Severity::Error)
                .map(|i| format!("[{}] {}: {}", i.severity, i.code, i.message))
                .collect();
            return Err(vault::VaultError::Migration(format!(
                "Pre-apply lint gate failed with {} error(s):\n{}",
                report.error_count,
                diagnostics.join("\n")
            )));
        }
        Ok(())
    })?;
    Ok(count)
}

pub fn vault_apply_plan(
    root: &Path,
    plan: &vault::migrator::MigrationPlan,
) -> anyhow::Result<usize> {
    vault_apply_plan_with_config(root, plan, None)
}

pub fn vault_apply(
    root: &Path,
    plan_path: Option<&Path>,
    config: Option<&AkashicConfig>,
) -> anyhow::Result<usize> {
    let plan_file = match plan_path {
        Some(p) if !p.as_os_str().is_empty() && !p.to_string_lossy().trim().is_empty() => p,
        _ => {
            anyhow::bail!(
                "Missing required `--plan <path>` argument. Direct implicit vault mutation is prohibited; run `akashic vault plan --output <plan.json>` first, then execute `akashic vault apply --plan <plan.json>`."
            );
        }
    };

    let plan_json = std::fs::read_to_string(plan_file)?;
    let plan: vault::migrator::MigrationPlan = serde_json::from_str(&plan_json)?;
    vault_apply_plan_with_config(root, &plan, config)
}

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
    #[arg(long, alias = "note-id")]
    pub node_id: Option<String>,
    #[arg(long)]
    pub query: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct VaultInspectArgs {
    #[arg(long, alias = "note-id")]
    pub node_id: Option<String>,
    #[arg(value_name = "NODE_ID")]
    pub positional_node_id: Option<String>,
}

impl VaultInspectArgs {
    pub fn node_id(&self) -> Option<&str> {
        self.node_id.as_deref().or(self.positional_node_id.as_deref())
    }
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct VaultPlanArgs {
    #[arg(long, alias = "output", short = 'o')]
    pub out: Option<String>,
    #[arg(long)]
    pub path: Option<String>,
    #[arg(value_name = "VAULT_PATH")]
    pub positional_path: Option<String>,
}

impl VaultPlanArgs {
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref().or(self.positional_path.as_deref())
    }
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct VaultApplyArgs {
    #[arg(long)]
    pub plan: Option<String>,
    #[arg(long)]
    pub path: Option<String>,
    #[arg(value_name = "VAULT_PATH")]
    pub positional_path: Option<String>,
}

impl VaultApplyArgs {
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref().or(self.positional_path.as_deref())
    }
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
    Prepare(RcaRunArgs),
    Context(RcaContextArgs),
    SubmitFinding(RcaSubmitFindingArgs),
    Resume(RcaResumeArgs),
    Status(RcaStatusArgs),
    Symbolicate(RcaSymbolicateArgs),
    Crystallize(RcaCrystallizeArgs),
    Curate(RcaCurateArgs),
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
    #[arg(long)]
    pub two_phase: bool,
    #[arg(long)]
    pub prepare_only: bool,
    #[arg(long)]
    pub finding_file: Option<String>,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct RcaContextArgs {
    #[arg(long)]
    pub run_id: String,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct RcaSubmitFindingArgs {
    #[arg(long)]
    pub run_id: String,
    #[arg(long)]
    pub finding_file: String,
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
    #[arg(long)]
    pub finding_file: Option<String>,
    #[arg(long)]
    pub outcome: Option<String>,
    #[arg(long)]
    pub remediation: Option<String>,
    #[arg(long)]
    pub confidence: Option<f64>,
    #[arg(long)]
    pub hold_for_curation: bool,
}

#[derive(Args, Debug, PartialEq, Default)]
pub struct RcaCurateArgs {
    #[arg(long)]
    pub run_id: String,
    #[arg(long)]
    pub candidate_id: String,
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
    ServeA2a(DaemonServeA2aArgs),
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

#[derive(Args, Debug, PartialEq, Default)]
pub struct DaemonServeA2aArgs {
    #[arg(long)]
    pub bind: Option<String>,
    #[arg(long)]
    pub db: Option<String>,
    #[arg(long)]
    pub vault: Option<String>,
    #[arg(long)]
    pub graph_store: Option<String>,
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

        let cli = Cli::try_parse_from(["akashic", "vault", "query", "--node-id", "INC-123"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Vault {
                command: VaultCommands::Query(args)
            } if args.node_id.as_deref() == Some("INC-123")
        ));

        let cli = Cli::try_parse_from(["akashic", "vault", "query", "--note-id", "INC-123"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Vault {
                command: VaultCommands::Query(args)
            } if args.node_id.as_deref() == Some("INC-123")
        ));

        let cli = Cli::try_parse_from(["akashic", "vault", "inspect", "INC-123"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Vault {
                command: VaultCommands::Inspect(args)
            } if args.node_id() == Some("INC-123")
        ));

        let cli = Cli::try_parse_from(["akashic", "vault", "inspect", "--node-id", "INC-123"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Vault {
                command: VaultCommands::Inspect(args)
            } if args.node_id() == Some("INC-123")
        ));

        let cli = Cli::try_parse_from(["akashic", "vault", "inspect", "--note-id", "INC-123"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Vault {
                command: VaultCommands::Inspect(args)
            } if args.node_id() == Some("INC-123")
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
    fn test_config_and_context() {
        let toml_data = r#"
            default_repo = "custom_repo"

            [vault]
            path = "my_vault"
            db_path = "custom/vault_index.db"

            [engine]
            db_path = "custom/code_graph.db"

            [rca]
            db_path = "custom/rca_state.db"
        "#;
        let config = AkashicConfig::from_toml_str(toml_data).unwrap();
        assert_eq!(config.default_repo.as_deref(), Some("custom_repo"));
        assert_eq!(config.vault.path, "my_vault");
        assert_eq!(config.vault.db_path, "custom/vault_index.db");
        assert_eq!(config.engine.db_path, "custom/code_graph.db");
        assert_eq!(config.rca.db_path, "custom/rca_state.db");

        // Defaults should be retained for omitted fields
        assert_eq!(config.vault.okf_version, "0.2");
        assert_eq!(config.engine.languages, vec!["java", "kotlin", "cpp", "rust", "python", "go", "typescript"]);
        assert_eq!(config.engine.god_node_degree_threshold, 25);

        let ctx = CliRuntimeContext::from_config(config);
        assert_eq!(ctx.resolve_vault_path(None), PathBuf::from("my_vault"));
        assert_eq!(ctx.resolve_vault_path(Some(Path::new("override_vault"))), PathBuf::from("override_vault"));
        assert_eq!(ctx.resolve_repo_name(None), "custom_repo");
        assert_eq!(ctx.resolve_repo_name(Some("cli_repo")), "cli_repo");

        let vc = ctx.resolve_vault_config(None);
        assert_eq!(vc.path, "my_vault");
        assert_eq!(vc.db_path, "custom/vault_index.db");
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

        let cli = Cli::try_parse_from(["akashic", "rca", "prepare"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Rca {
                command: RcaCommands::Prepare(_)
            }
        ));

        let cli = Cli::try_parse_from(["akashic", "rca", "context", "--run-id", "RUN-1"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Rca {
                command: RcaCommands::Context(args)
            } if args.run_id == "RUN-1"
        ));

        let cli = Cli::try_parse_from(["akashic", "rca", "submit-finding", "--run-id", "RUN-1", "--finding-file", "finding.json"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Rca {
                command: RcaCommands::SubmitFinding(args)
            } if args.run_id == "RUN-1" && args.finding_file == "finding.json"
        ));

        let cli = Cli::try_parse_from(["akashic", "rca", "curate", "--run-id", "RUN-1", "--candidate-id", "CAND-1"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Rca {
                command: RcaCommands::Curate(args)
            } if args.run_id == "RUN-1" && args.candidate_id == "CAND-1"
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
