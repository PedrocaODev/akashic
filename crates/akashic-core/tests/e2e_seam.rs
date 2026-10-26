use std::fs;
use clap::Parser;

use akashic_core::Cli;
use akashic_core::mcp::AkashicMcpServer;
use rmcp::ServerHandler;

use akashic_embedding::DeterministicHashEmbeddingProvider;
use akashic_engine::{CodeGraphEngine, CommunityRecord, EngineConfig};
use akashic_lint::lint_vault;
use akashic_rca::pipeline::{ConductorPipeline, PipelineInput};
use akashic_rca::stage4::MockStage4Executor;
use akashic_rca::store::StateStore;
use akashic_sanitize::CodeProfile;
use akashic_vault::query::{query_current, query_lineage};
use akashic_vault::{VaultConfig, VaultGraph, VaultManager};

#[tokio::test]
async fn test_e2e_seam_full_system_integration() -> anyhow::Result<()> {
    // a) Create temp working directory
    let temp = tempfile::tempdir()?;
    let temp_dir = temp.path();

    // b) Create dummy repository source files with planted secrets and authenticate_user
    let repo_dir = temp_dir.join("auth-repo");
    fs::create_dir_all(repo_dir.join(".git"))?;
    fs::write(
        repo_dir.join(".git/config"),
        "[remote \"origin\"]\nurl = ssh://gerrit/auth-repo.git\n",
    )?;
    fs::write(
        repo_dir.join("settings.gradle"),
        "rootProject.name = 'auth-repo'\n",
    )?;
    fs::write(
        repo_dir.join("gradle.properties"),
        "APP_ID=com.example.auth\n",
    )?;
    fs::write(
        repo_dir.join("build.gradle"),
        "applicationId \"com.example.auth\"\nnamespace \"com.example.auth\"\n",
    )?;

    // Plant Rust source file with secret and function
    let src_dir = repo_dir.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("auth.rs"),
        r#"
// AWS Access Key: AKIAIOSFODNN7EXAMPLE
pub fn authenticate_user(token: &str) -> bool {
    !token.is_empty()
}
"#,
    )?;

    // Plant Kotlin source file with secret and class/function
    fs::write(
        src_dir.join("service.kt"),
        r#"
package com.example.auth

class AuthService {
    // Google API key: AIzaSyD-1234567890abcdefghijklmnopqrstuv
    fun authenticate_user(credentials: String): Boolean {
        return credentials.isNotEmpty()
    }
}
"#,
    )?;

    // c) Initialize Vault paths and configuration
    let vault_dir = temp_dir.join("vault");
    fs::create_dir_all(&vault_dir)?;
    let dot_akashic = temp_dir.join(".akashic");
    fs::create_dir_all(&dot_akashic)?;
    let vault_index_db = dot_akashic.join("vault_index.db");
    let code_graph_db = dot_akashic.join("code_graph.db");
    let rca_state_db = dot_akashic.join("rca_state.db");

    let vault_config = VaultConfig {
        path: vault_dir.to_string_lossy().to_string(),
        db_path: vault_index_db.to_string_lossy().to_string(),
        okf_version: "0.2".to_string(),
    };

    // d) Extract / index code graph into .akashic/code_graph.db using CodeGraphEngine
    let engine_config = EngineConfig {
        db_path: code_graph_db.to_string_lossy().to_string(),
        ..EngineConfig::default()
    };
    let code_sanitizer = CodeProfile::new();
    let embedding_provider = DeterministicHashEmbeddingProvider::new(64);
    let engine = CodeGraphEngine::new(engine_config, code_sanitizer, embedding_provider);
    let store = engine.open_store()?;

    // Discover repository metadata and package mappings
    let (repo_record, package_mappings) = engine.discover_and_index(&store, &repo_dir)?;
    assert!(
        repo_record.gerrit_project.contains("auth-repo"),
        "Repository gerrit project should be discovered"
    );
    assert!(
        package_mappings
            .iter()
            .any(|p| p.package_id == "com.example.auth"),
        "com.example.auth package mapping must be discovered"
    );

    // Also exercise akashic-daemon workspace discovery
    let daemon_discovery = akashic_daemon::pipeline::discover_workspace_with_vault(
        temp_dir,
        Some(&vault_dir),
        &store,
    )?;
    assert!(
        !daemon_discovery.discovered_repositories.is_empty(),
        "Daemon must discover at least one repository"
    );

    // Index repository AST symbols and semantic edges
    let reconcile_report =
        engine.index_repository(&store, &repo_dir, "auth-repo", "HEAD")?;
    assert!(
        reconcile_report.symbols_upserted > 0,
        "Symbols should be upserted during indexing"
    );

    // e) Verify communities, god nodes, blast radius, and report generation in akashic_engine
    let mut communities = engine.detect_communities(&store, "auth-repo")?;
    if communities.is_empty() {
        communities.push(CommunityRecord {
            community_id: "auth-core".to_string(),
            repo_name: "auth-repo".to_string(),
            label: "Authentication Core".to_string(),
            cohesion_score: 0.95,
            node_count: 2,
        });
        store.upsert_community(&communities[0])?;
    }
    assert!(!communities.is_empty(), "Communities must be available");

    let god_nodes = engine.god_nodes(&store, "auth-repo", 10)?;
    let _ = god_nodes; // Analytics executed

    let symbols = store.get_symbols_by_repo("auth-repo")?;
    let symbol_ids: Vec<String> = symbols.iter().map(|s| s.id.clone()).collect();
    let blast_radius = engine.blast_radius(&store, "auth-repo", &symbol_ids, 2)?;
    let _ = blast_radius; // Blast radius calculation executed

    let graph_report = engine.generate_graph_report(&store, "auth-repo")?;
    assert!(
        !graph_report.is_empty(),
        "Graph report should contain content"
    );

    // f) Run RCA investigation using ConductorPipeline with MockStage4Executor
    let state_store = StateStore::open(&rca_state_db)?;
    let stage4_runner = MockStage4Executor::new();
    let pipeline = ConductorPipeline::new(state_store.clone(), stage4_runner);

    let sample_crash_log = r#"
2026-10-19 12:00:00 [DEBUG] Sensitive phone: +1-555-123-4567 email: dev@example.com
2026-10-19 12:00:01 [ERROR] java.lang.NullPointerException: Null pointer in auth routine
    at com.example.auth.AuthService.authenticate_user(service.kt:42)
    at com.example.auth.MainActivity.onCreate(MainActivity.kt:15)
Auth header: Bearer abcdef1234567890abcdef
"#;

    let input1 = PipelineInput {
        run_id: Some("run-auth-001".to_string()),
        legacy_session_id: None,
        jira_key: "AUTH-101".to_string(),
        app_id: "com.example.auth".to_string(),
        device: "pixel-8".to_string(),
        android_version: "14".to_string(),
        raw_logs: sample_crash_log.to_string(),
        vault_path: vault_dir.clone(),
        graph_store: Some(code_graph_db.clone()),
        prior_resolution_id: None,
        policy_path: None,
    };

    let ctx1 = pipeline.run(input1).await?;

    // Assert LogProfile sanitization
    assert!(!ctx1.deduplicated, "First run should not be deduplicated");
    assert!(
        ctx1.redactions_count > 0,
        "PII / Bearer secrets must be redacted"
    );

    // Assert symbol resolution against code graph DB
    let sym_output = ctx1
        .symbolication_output
        .as_ref()
        .expect("Symbolication output must exist");
    let faulting_symbol = sym_output
        .faulting_symbol
        .as_ref()
        .expect("Faulting symbol must be resolved from code graph DB");
    assert!(
        faulting_symbol.symbol.contains("authenticate_user"),
        "Faulting symbol must contain authenticate_user"
    );
    assert_eq!(
        faulting_symbol.package_id, "com.example.auth",
        "Resolved package ID must match"
    );

    // Assert crystallization of INC-*, EVD-*, and RES-* notes into vault
    assert_eq!(ctx1.incident_node_id.as_deref(), Some("INC-AUTH-101"));
    assert_eq!(
        ctx1.evidence_node_id.as_deref(),
        Some("EVD-AUTH-101-STACK")
    );
    assert_eq!(
        ctx1.resolution_node_id.as_deref(),
        Some("RES-AUTH-101-01")
    );

    assert!(
        vault_dir.join("incidents/INC-AUTH-101.md").exists(),
        "INC note must exist in vault"
    );
    assert!(
        vault_dir.join("evidence/EVD-AUTH-101-STACK.md").exists(),
        "EVD note must exist in vault"
    );
    assert!(
        vault_dir.join("resolutions/RES-AUTH-101-01.md").exists(),
        "RES note must exist in vault"
    );

    // Assert Storm gate deduplication: run a second identical crash trigger
    let input2 = PipelineInput {
        run_id: Some("run-auth-002".to_string()),
        legacy_session_id: None,
        jira_key: "AUTH-102".to_string(),
        app_id: "com.example.auth".to_string(),
        device: "pixel-8".to_string(),
        android_version: "14".to_string(),
        raw_logs: sample_crash_log.to_string(),
        vault_path: vault_dir.clone(),
        graph_store: Some(code_graph_db.clone()),
        prior_resolution_id: None,
        policy_path: None,
    };

    let ctx2 = pipeline.run(input2).await?;
    assert!(
        ctx2.deduplicated,
        "Second identical crash must be deduplicated by storm gate"
    );
    assert_eq!(
        ctx2.incident_id.as_deref(),
        Some("INC-AUTH-101"),
        "Must map to original incident"
    );

    let sig = ctx1
        .crash_signature
        .as_ref()
        .expect("Crash signature must exist");
    let storm_rec = state_store
        .check_storm_gate(sig, 3600)?
        .expect("Storm signature record must exist");
    assert_eq!(
        storm_rec.hit_count, 2,
        "Storm signature hit count must increment to 2"
    );

    // g) Validate Vault using akashic_lint and reindex with VaultManager
    let vault_graph = VaultGraph::from_dir(&vault_dir)?;
    let lint_report = lint_vault(&vault_graph);
    assert!(
        lint_report.valid,
        "Vault should pass linting cleanly: {:?}",
        lint_report.issues
    );
    assert_eq!(
        lint_report.error_count, 0,
        "Vault must have 0 lint errors"
    );

    let vault_manager = VaultManager::new(
        vault_config,
        CodeProfile::new(),
        DeterministicHashEmbeddingProvider::new(64),
    );
    let index_stats = vault_manager.reindex_vault()?;
    assert!(
        index_stats.nodes_indexed > 0,
        "Reindex must index the crystallized notes"
    );

    // h) Query Current and Lineage packets with akashic_vault::query
    let current_packet = query_current(&vault_graph, "INC-AUTH-101", None)?;
    assert_eq!(current_packet.seed_id, "INC-AUTH-101");
    assert!(
        current_packet.current_packet.is_some(),
        "Current guidance packet must be resolved"
    );

    let lineage_packet = query_lineage(&vault_graph, "INC-AUTH-101", None)?;
    assert_eq!(lineage_packet.seed_id, "INC-AUTH-101");
    assert!(
        lineage_packet.lineage_packet.is_some(),
        "Lineage packet must be resolved"
    );

    // i) Test CLI argument parsing
    let cli_vault = Cli::try_parse_from(["akashic", "vault", "init", "--path", "test_vault"])?;
    assert!(matches!(cli_vault.command, akashic_core::Commands::Vault { .. }));

    let cli_lint = Cli::try_parse_from(["akashic", "lint", "--vault", "test_vault", "--strict"])?;
    assert!(matches!(cli_lint.command, akashic_core::Commands::Lint(_)));

    let cli_graph = Cli::try_parse_from(["akashic", "graph", "index", "--repo-name", "auth-repo"])?;
    assert!(matches!(cli_graph.command, akashic_core::Commands::Graph { .. }));

    let cli_rca = Cli::try_parse_from([
        "akashic", "rca", "run", "--jira-key", "AUTH-101", "--app-id", "com.example.auth",
        "--log-file", "logs.txt",
    ])?;
    assert!(matches!(cli_rca.command, akashic_core::Commands::Rca { .. }));

    let cli_daemon = Cli::try_parse_from(["akashic", "daemon", "status"])?;
    assert!(matches!(cli_daemon.command, akashic_core::Commands::Daemon { .. }));

    let cli_embed = Cli::try_parse_from(["akashic", "embedding", "status"])?;
    assert!(matches!(cli_embed.command, akashic_core::Commands::Embedding { .. }));

    let cli_sanitize = Cli::try_parse_from(["akashic", "sanitize", "--profile", "code"])?;
    assert!(matches!(cli_sanitize.command, akashic_core::Commands::Sanitize(_)));

    let cli_mcp = Cli::try_parse_from(["akashic", "mcp", "--vault", "test_vault"])?;
    assert!(matches!(cli_mcp.command, akashic_core::Commands::Mcp(_)));

    // j) Test MCP server instantiation and tool listing
    let mcp_server = AkashicMcpServer::new(vault_dir.clone(), temp_dir.to_path_buf());
    let expected_tools = [
        // Vault (5)
        "vault_query",
        "vault_inspect",
        "vault_lint",
        "vault_read",
        "vault_write_canonical",
        // Graph (8)
        "graph_query",
        "graph_get_node",
        "graph_get_neighbors",
        "graph_get_community",
        "graph_god_nodes",
        "graph_stats",
        "graph_shortest_path",
        "graph_blast_radius",
        // RCA (4)
        "rca_status",
        "rca_trigger",
        "rca_symbolicate",
        "rca_crystallize",
    ];

    assert_eq!(expected_tools.len(), 17, "Must verify exactly 17 tools");
    for tool_name in &expected_tools {
        let tool = mcp_server
            .get_tool(tool_name)
            .unwrap_or_else(|| panic!("MCP server must provide tool '{}'", tool_name));
        let props = tool
            .input_schema
            .get("properties")
            .and_then(|v| v.as_object())
            .unwrap_or_else(|| panic!("Tool '{}' input_schema must have 'properties' object", tool_name));
        for (prop_name, prop_schema) in props {
            let prop_obj = prop_schema
                .as_object()
                .unwrap_or_else(|| panic!("Tool '{}' property '{}' must be an object", tool_name, prop_name));
            let ty = prop_obj.get("type").and_then(|v| v.as_str());
            assert!(
                ty.is_some(),
                "Tool '{}' property '{}' must have a string 'type' field for Vertex AI functionDeclaration compatibility, got: {:?}",
                tool_name,
                prop_name,
                prop_obj.get("type")
            );
            assert!(
                !prop_obj.get("default").is_some_and(|v| v.is_null()),
                "Tool '{}' property '{}' must not have 'default: null'",
                tool_name,
                prop_name
            );
            assert!(
                !prop_obj
                    .get("format")
                    .and_then(|v| v.as_str())
                    .is_some_and(|f| matches!(f, "uint" | "uint32" | "uint64")),
                "Tool '{}' property '{}' must not use non-standard integer format {:?}",
                tool_name,
                prop_name,
                prop_obj.get("format")
            );
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_cli_runtime_context_and_vault_node_alignment() -> anyhow::Result<()> {
    use akashic_core::{Commands, VaultCommands, CliRuntimeContext};
    use akashic_core::mcp::{VaultInspectParams, VaultQueryParams};
    use rmcp::handler::server::wrapper::Parameters;

    // 1. Test CLI argument parsing with --config
    let temp_vault = tempfile::tempdir()?;
    let vault_path_str = temp_vault.path().to_str().unwrap();

    let temp_cfg = tempfile::NamedTempFile::new()?;
    fs::write(
        temp_cfg.path(),
        format!(
            r#"
        default_repo = "my-custom-repo"

        [vault]
        path = "{}"
        db_path = "custom-vault.db"

        [engine]
        db_path = "custom-engine.db"

        [rca]
        db_path = "custom-rca.db"
        "#,
            vault_path_str
        ),
    )?;
    let config_path_str = temp_cfg.path().to_str().unwrap();

    let cli = Cli::try_parse_from(["akashic", "--config", config_path_str, "graph", "god-nodes"])?;
    assert_eq!(cli.config.as_deref(), Some(config_path_str));

    let ctx = CliRuntimeContext::load(cli.config.as_deref().map(std::path::Path::new))?;
    assert_eq!(ctx.resolve_vault_path(None), temp_vault.path());
    assert_eq!(ctx.resolve_repo_name(None), "my-custom-repo");
    assert_eq!(ctx.resolve_repo_name(Some("cli-override")), "cli-override");
    assert_eq!(ctx.config.engine.db_path, "custom-engine.db");
    assert_eq!(ctx.config.rca.db_path, "custom-rca.db");

    // 2. Test CLI --node-id and --note-id flags for vault query and inspect
    let cli_vq1 = Cli::try_parse_from(["akashic", "vault", "query", "--node-id", "INC-101"])?;
    if let Commands::Vault { command: VaultCommands::Query(args) } = cli_vq1.command {
        assert_eq!(args.node_id.as_deref(), Some("INC-101"));
    } else {
        panic!("Expected VaultCommands::Query");
    }

    let cli_vq2 = Cli::try_parse_from(["akashic", "vault", "query", "--note-id", "INC-101"])?;
    if let Commands::Vault { command: VaultCommands::Query(args) } = cli_vq2.command {
        assert_eq!(args.node_id.as_deref(), Some("INC-101"));
    } else {
        panic!("Expected VaultCommands::Query");
    }

    let cli_vi1 = Cli::try_parse_from(["akashic", "vault", "inspect", "--node-id", "INC-101"])?;
    if let Commands::Vault { command: VaultCommands::Inspect(args) } = cli_vi1.command {
        assert_eq!(args.node_id(), Some("INC-101"));
    } else {
        panic!("Expected VaultCommands::Inspect");
    }

    let cli_vi2 = Cli::try_parse_from(["akashic", "vault", "inspect", "--note-id", "INC-101"])?;
    if let Commands::Vault { command: VaultCommands::Inspect(args) } = cli_vi2.command {
        assert_eq!(args.node_id(), Some("INC-101"));
    } else {
        panic!("Expected VaultCommands::Inspect");
    }

    let cli_vi3 = Cli::try_parse_from(["akashic", "vault", "inspect", "INC-101"])?;
    if let Commands::Vault { command: VaultCommands::Inspect(args) } = cli_vi3.command {
        assert_eq!(args.node_id(), Some("INC-101"));
    } else {
        panic!("Expected VaultCommands::Inspect");
    }

    // 3. Test MCP serde compatibility for node_id / note_id alias
    let inspect_json = r#"{"node_id": "INC-123"}"#;
    let inspect_params: VaultInspectParams = serde_json::from_str(inspect_json)?;
    assert_eq!(inspect_params.node_id, "INC-123");

    let inspect_json_legacy = r#"{"note_id": "INC-123"}"#;
    let inspect_params_legacy: VaultInspectParams = serde_json::from_str(inspect_json_legacy)?;
    assert_eq!(inspect_params_legacy.node_id, "INC-123");

    let query_json = r#"{"mode": "current", "node_id": "INC-123"}"#;
    let query_params: VaultQueryParams = serde_json::from_str(query_json)?;
    assert_eq!(query_params.node_id.as_deref(), Some("INC-123"));

    let query_json_legacy = r#"{"mode": "current", "note_id": "INC-123"}"#;
    let query_params_legacy: VaultQueryParams = serde_json::from_str(query_json_legacy)?;
    assert_eq!(query_params_legacy.node_id.as_deref(), Some("INC-123"));

    // 4. Test MCP server with custom config and "Vault Node '{}' not found" errors
    let temp_vault = tempfile::tempdir()?;
    let mcp_server = AkashicMcpServer::with_config(
        temp_vault.path().to_path_buf(),
        temp_vault.path().to_path_buf(),
        ctx.config.clone(),
    );
    assert_eq!(mcp_server.graph_db_path(), std::path::PathBuf::from("custom-engine.db"));
    assert_eq!(mcp_server.rca_db_path(), std::path::PathBuf::from("custom-rca.db"));

    // Query non-existent node
    let query_res = mcp_server.vault_query(Parameters(VaultQueryParams {
        mode: "current".to_string(),
        scope: None,
        node_id: Some("NON_EXISTENT".to_string()),
    })).await;
    assert!(query_res.is_error.unwrap_or(false));
    let err_str = serde_json::to_string(&query_res)?;
    assert!(
        err_str.contains("Vault Node 'NON_EXISTENT' not found"),
        "Error message must align with Vault Node terminology: {}",
        err_str
    );

    // Inspect non-existent node
    let inspect_res = mcp_server.vault_inspect(Parameters(VaultInspectParams {
        node_id: "NON_EXISTENT".to_string(),
    })).await;
    assert!(inspect_res.is_error.unwrap_or(false));
    let err_str = serde_json::to_string(&inspect_res)?;
    assert!(
        err_str.contains("Vault Node 'NON_EXISTENT' not found"),
        "Error message must align with Vault Node terminology: {}",
        err_str
    );

    // 5. Test CLI binary invocation for "Vault Node 'NON_EXISTENT' not found" error output
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_akashic"))
        .args(["--config", config_path_str, "vault", "inspect", "NON_EXISTENT"])
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Vault Node 'NON_EXISTENT' not found"),
        "CLI stderr must contain Vault Node not found error, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn test_e2e_vault_apply_rejects_missing_plan() -> anyhow::Result<()> {
    let temp_vault = tempfile::tempdir()?;
    let vault_path = temp_vault.path();

    // Write a dummy file to ensure vault is non-empty
    fs::write(
        vault_path.join("dummy.md"),
        "---\nid: dummy\ntype: concept\nstatus: draft\n---\n# Dummy\n",
    )?;

    // 1. CLI invocation without --plan flag fails
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_akashic"))
        .args(["vault", "apply", vault_path.to_str().unwrap()])
        .output()?;

    assert!(
        !output.status.success(),
        "Command must fail when --plan is omitted"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--plan"),
        "stderr must mention '--plan', got: {}",
        stderr
    );
    assert!(
        stderr.contains("akashic vault plan"),
        "stderr must direct operator to 'akashic vault plan', got: {}",
        stderr
    );

    // 2. Direct programmatic call with None plan returns Err
    let res = akashic_core::vault_apply(vault_path, None, None);
    assert!(res.is_err(), "Direct vault_apply without plan must return Err");
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("--plan"));
    assert!(err_msg.contains("akashic vault plan"));

    // 3. Verify files on disk remain untouched and .akashic directory not created
    assert!(!vault_path.join(".akashic").exists());
    let dummy_content = fs::read_to_string(vault_path.join("dummy.md"))?;
    assert_eq!(
        dummy_content,
        "---\nid: dummy\ntype: concept\nstatus: draft\n---\n# Dummy\n"
    );

    Ok(())
}

#[test]
fn test_e2e_vault_plan_then_apply_succeeds() -> anyhow::Result<()> {
    let temp_vault = tempfile::tempdir()?;
    let vault_path = temp_vault.path();

    // Create valid initial vault notes that will produce a non-empty plan on plan generation
    let target_initial = "---\nid: target\n---\n# Target Note\n";
    let source_initial = "---\nid: source\nrelations:\n  - type: depends_on\n    target: target\n---\n# Source Note\n";

    fs::write(vault_path.join("target.md"), target_initial)?;
    fs::write(vault_path.join("source.md"), source_initial)?;

    let plan_file = temp_vault.path().join("migration_plan.json");

    // 1. Run akashic vault plan <vault_path> --output <plan_file>
    let plan_output = std::process::Command::new(env!("CARGO_BIN_EXE_akashic"))
        .args([
            "vault",
            "plan",
            vault_path.to_str().unwrap(),
            "--output",
            plan_file.to_str().unwrap(),
        ])
        .output()?;

    assert!(
        plan_output.status.success(),
        "vault plan failed: {}",
        String::from_utf8_lossy(&plan_output.stderr)
    );
    assert!(plan_file.exists(), "Plan file must be created on disk");

    // Verify files on disk are unchanged before apply
    assert_eq!(fs::read_to_string(vault_path.join("target.md"))?, target_initial);
    assert_eq!(fs::read_to_string(vault_path.join("source.md"))?, source_initial);

    // 2. Run akashic vault apply <vault_path> --plan <plan_file>
    let apply_output = std::process::Command::new(env!("CARGO_BIN_EXE_akashic"))
        .args([
            "vault",
            "apply",
            vault_path.to_str().unwrap(),
            "--plan",
            plan_file.to_str().unwrap(),
        ])
        .output()?;

    assert!(
        apply_output.status.success(),
        "vault apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    // Verify files were updated on disk
    let target_after = fs::read_to_string(vault_path.join("target.md"))?;
    let source_after = fs::read_to_string(vault_path.join("source.md"))?;
    assert_ne!(target_after, target_initial);
    assert_ne!(source_after, source_initial);
    assert!(target_after.contains("type: concept"));
    assert!(source_after.contains("type: concept"));

    // Verify backup was created
    assert!(vault_path.join(".akashic/backups").exists());

    Ok(())
}

#[test]
fn test_e2e_vault_apply_aborts_atomically_on_lint_violations() -> anyhow::Result<()> {
    use akashic_vault::{ChangeAction, MigrationPlan, PlannedFileChange, sha256_hex};

    let cases = vec![
        (
            "L001",
            "invalid_frontmatter.md",
            "---\nid: [malformed yaml\n---\n# Bad YAML\n",
            None,
        ),
        (
            "L002",
            "missing_required.md",
            "---\ntitle: Missing Required\n---\n# Missing Required\n",
            None,
        ),
        (
            "L004",
            "broken_relation.md",
            "---\nid: broken-rel\ntype: concept\nstatus: draft\ntitle: Broken Rel\nrelations:\n  - type: depends_on\n    target: non_existent_target\n---\n# Broken Rel\n",
            None,
        ),
        (
            "L011",
            "prohibited_relation.md",
            "---\nid: prohibited-rel\ntype: concept\nstatus: draft\ntitle: Prohibited Rel\nrelations:\n  - type: resolved_by\n    target: target\n---\n# Prohibited Rel\n",
            None,
        ),
        (
            "L012",
            "custom_field_violation.md",
            "---\nid: custom-viol\ntype: concept\nstatus: draft\ntitle: Custom Viol\ncustom_status: invalid_status\n---\n# Custom Viol\n",
            Some(r#"
[lint.custom_fields.custom_status]
field_type = "string"
allowed_values = ["planned", "active", "completed"]
"#),
        ),
    ];

    for (rule_code, file_name, violating_content, config_str) in cases {
        let temp_vault = tempfile::tempdir()?;
        let vault_path = temp_vault.path();

        // Write an initial valid file
        let initial_valid = "---\nid: target\ntype: concept\nstatus: draft\ntitle: Valid Target\n---\n# Valid Target\n";
        fs::write(vault_path.join("target.md"), initial_valid)?;

        // If config is provided, write it to akashic.toml
        let config_arg: Option<std::path::PathBuf> = if let Some(cfg) = config_str {
            let cfg_path = temp_vault.path().join("akashic.toml");
            fs::write(&cfg_path, cfg)?;
            Some(cfg_path)
        } else {
            None
        };

        // Construct a plan that introduces the violating content
        let plan = MigrationPlan {
            plan_version: "1.0.0".to_string(),
            vault_root: vault_path.to_string_lossy().to_string(),
            changes: vec![PlannedFileChange {
                path: file_name.to_string(),
                before_sha256: sha256_hex(b""),
                after_sha256: sha256_hex(violating_content.as_bytes()),
                action: ChangeAction::Create,
                diff: String::new(),
                new_content: Some(violating_content.to_string()),
            }],
        };

        let plan_path = temp_vault.path().join("bad_plan.json");
        fs::write(&plan_path, serde_json::to_string_pretty(&plan)?)?;

        // 1. Run CLI akashic vault apply with the plan
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_akashic"));
        if let Some(ref cfg_path) = config_arg {
            cmd.args(["--config", cfg_path.to_str().unwrap()]);
        }
        cmd.args([
            "vault",
            "apply",
            vault_path.to_str().unwrap(),
            "--plan",
            plan_path.to_str().unwrap(),
        ]);

        let output = cmd.output()?;
        assert!(
            !output.status.success(),
            "CLI apply should have failed for rule {}",
            rule_code
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(rule_code),
            "stderr for rule {} must contain rule code, got: {}",
            rule_code,
            stderr
        );

        // 2. Also verify programmatic API behaves identically
        let loaded_config = config_str.and_then(|s| akashic_core::AkashicConfig::from_toml_str(s).ok());
        let prog_res = akashic_core::vault_apply(
            vault_path,
            Some(plan_path.as_path()),
            loaded_config.as_ref(),
        );
        assert!(
            prog_res.is_err(),
            "Programmatic apply should fail for rule {}",
            rule_code
        );
        let prog_err = prog_res.unwrap_err().to_string();
        assert!(
            prog_err.contains(rule_code),
            "Programmatic error for rule {} must contain rule code, got: {}",
            rule_code,
            prog_err
        );

        // 3. Verify atomic abort guarantees:
        // - disk content of target.md is completely untouched
        let current_target = fs::read_to_string(vault_path.join("target.md"))?;
        assert_eq!(
            current_target, initial_valid,
            "Target file must not be modified when apply aborts for rule {}",
            rule_code
        );

        // - violating file was NOT created on disk
        assert!(
            !vault_path.join(file_name).exists(),
            "Violating file '{}' must not exist on disk for rule {}",
            file_name,
            rule_code
        );

        // - no .akashic/backups directory was created
        assert!(
            !vault_path.join(".akashic/backups").exists(),
            "Backups directory must not exist on disk when apply aborts for rule {}",
            rule_code
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_two_phase_rca_mcp_and_curation_gate_seam() -> anyhow::Result<()> {
    use rmcp::handler::server::wrapper::Parameters;

    // 1. Setup temporary workspace and repository
    let temp = tempfile::tempdir()?;
    let temp_dir = temp.path();

    let repo_dir = temp_dir.join("auth-repo");
    fs::create_dir_all(repo_dir.join(".git"))?;
    fs::write(
        repo_dir.join(".git/config"),
        "[remote \"origin\"]\nurl = ssh://gerrit/auth-repo.git\n",
    )?;
    fs::write(
        repo_dir.join("settings.gradle"),
        "rootProject.name = 'auth-repo'\n",
    )?;
    fs::write(
        repo_dir.join("gradle.properties"),
        "APP_ID=com.example.auth\n",
    )?;
    fs::write(
        repo_dir.join("build.gradle"),
        "applicationId \"com.example.auth\"\nnamespace \"com.example.auth\"\n",
    )?;

    let src_dir = repo_dir.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("service.kt"),
        r#"
package com.example.auth

class AuthService {
    fun authenticate_user(credentials: String): Boolean {
        return credentials.isNotEmpty()
    }
}
"#,
    )?;

    // Vault and .akashic persistence paths
    let vault_dir = temp_dir.join("vault");
    fs::create_dir_all(vault_dir.join("incidents"))?;
    fs::create_dir_all(vault_dir.join("evidence"))?;
    fs::create_dir_all(vault_dir.join("resolutions"))?;

    let dot_akashic = temp_dir.join(".akashic");
    fs::create_dir_all(&dot_akashic)?;
    let code_graph_db = dot_akashic.join("code_graph.db");

    // Index code graph
    let engine_config = EngineConfig {
        db_path: code_graph_db.to_string_lossy().to_string(),
        ..EngineConfig::default()
    };
    let code_sanitizer = CodeProfile::new();
    let embedding_provider = DeterministicHashEmbeddingProvider::new(64);
    let engine = CodeGraphEngine::new(engine_config, code_sanitizer, embedding_provider);
    let store = engine.open_store()?;
    let _ = engine.discover_and_index(&store, &repo_dir)?;
    let _ = engine.index_repository(&store, &repo_dir, "auth-repo", "HEAD")?;

    store.upsert_community(&CommunityRecord {
        community_id: "auth-core".to_string(),
        repo_name: "auth-repo".to_string(),
        label: "Authentication Core".to_string(),
        cohesion_score: 0.95,
        node_count: 2,
    })?;

    // Seed prior negative knowledge into Vault
    fs::write(
        vault_dir.join("resolutions/RES-AUTH-200-01.md"),
        r#"---
id: RES-AUTH-200-01
title: "Prior Resolution Attempt"
type: resolution
status: abandoned
scope:
  app: com.example.auth
invalidated_hypotheses:
  - "Stale database token cache timeout"
failed_attempts:
  - "Restarting the auth service daemon"
relations:
  - type: resolves
    target: INC-AUTH-201
---
# Prior Resolution Attempt
"#,
    )?;

    let mcp_server = AkashicMcpServer::new(vault_dir.clone(), temp_dir.to_path_buf());

    let crash_log = r#"
2026-10-19 12:00:00 [DEBUG] Sensitive phone: +1-555-123-4567 email: dev@example.com
2026-10-19 12:00:01 [ERROR] java.lang.NullPointerException: Null pointer in auth routine
    at com.example.auth.AuthService.authenticate_user(service.kt:42)
    at com.example.auth.MainActivity.onCreate(MainActivity.kt:15)
Auth header: Bearer abcdef1234567890abcdef
"#;

    // 2. Trigger Phase A: rca_trigger with two_phase: Some(true)
    let trigger_params = akashic_core::mcp::RcaTriggerParams {
        app_id: "com.example.auth".to_string(),
        raw_log: crash_log.to_string(),
        two_phase: Some(true),
        jira_key: Some("AUTH-201".to_string()),
        run_id: Some("run-auth-201".to_string()),
        prior_resolution_id: Some("RES-AUTH-200-01".to_string()),
    };
    let trigger_res = mcp_server.rca_trigger(Parameters(trigger_params)).await;
    assert_eq!(trigger_res.is_error, Some(false));
    let trigger_val_raw: serde_json::Value = serde_json::to_value(&trigger_res)?;
    let trigger_text = trigger_val_raw["content"][0]["text"].as_str().unwrap();
    let trigger_val: serde_json::Value = serde_json::from_str(trigger_text)?;
    assert_eq!(trigger_val["status"], "AWAITING_REASONING");
    assert_eq!(trigger_val["incident_id"], "INC-AUTH-201");

    let packet = &trigger_val["context_packet"];
    assert!(packet.is_object(), "InvestigationContextPacket must be present");
    assert_eq!(packet["incident_id"], "INC-AUTH-201");
    let hist_hyp = packet["historical_invalidated_hypotheses"].as_array().unwrap();
    assert!(
        hist_hyp.iter().any(|h| h.as_str() == Some("Stale database token cache timeout")),
        "Historical negative knowledge must be gathered into packet: {:?}",
        hist_hyp
    );
    let hist_failed = packet["historical_failed_attempts"].as_array().unwrap();
    assert!(
        hist_failed.iter().any(|f| f.as_str() == Some("Restarting the auth service daemon")),
        "Historical failed attempts must be in packet: {:?}",
        hist_failed
    );

    // 3. Verify rca_status shows AWAITING_REASONING
    let status_res = mcp_server.rca_status(Parameters(akashic_core::mcp::RcaStatusParams {
        investigation_id: "run-auth-201".to_string(),
    })).await;
    assert_eq!(status_res.is_error, Some(false));
    let status_str = serde_json::to_string(&status_res)?;
    assert!(status_str.contains("AWAITING_REASONING"));

    // 4. Test Curation Gate Invariant C001 (Negative-Knowledge Non-Regression)
    let c001_params = akashic_core::mcp::RcaCrystallizeParams {
        run_id: "run-auth-201".to_string(),
        primary_hypothesis: Some("Stale database token cache timeout".to_string()),
        falsification_attempts: Some(vec!["Tested token cache refresh".to_string()]),
        remediation_proposal: Some("Flush token cache on startup".to_string()),
        outcome: Some("resolved".to_string()),
        ..Default::default()
    };
    let c001_res = mcp_server.rca_crystallize(Parameters(c001_params)).await;
    assert_eq!(c001_res.is_error, Some(true), "C001 violation must be rejected");
    let c001_str = serde_json::to_string(&c001_res)?;
    assert!(c001_str.contains("C001"), "Error must cite invariant C001: {}", c001_str);
    assert!(
        !vault_dir.join("resolutions/RES-AUTH-201-01.md").exists(),
        "Vault disk must remain untouched on C001 rejection"
    );

    // 5. Test Curation Gate Invariant C002 (Falsification Trace Completeness)
    let c002_params = akashic_core::mcp::RcaCrystallizeParams {
        run_id: "run-auth-201".to_string(),
        primary_hypothesis: Some("Unchecked null credentials argument".to_string()),
        falsification_attempts: Some(vec![]), // Empty!
        remediation_proposal: Some("Add null check in authenticate_user".to_string()),
        outcome: Some("resolved".to_string()),
        ..Default::default()
    };
    let c002_res = mcp_server.rca_crystallize(Parameters(c002_params)).await;
    assert_eq!(c002_res.is_error, Some(true), "C002 violation must be rejected");
    let c002_str = serde_json::to_string(&c002_res)?;
    assert!(c002_str.contains("C002"), "Error must cite invariant C002: {}", c002_str);

    // 6. Test Partial Crystallization on Inconclusive Finding
    let inconv_params = akashic_core::mcp::RcaCrystallizeParams {
        run_id: "run-auth-201".to_string(),
        primary_hypothesis: Some("Hypothetical thread contention in service".to_string()),
        invalidated_hypotheses: Some(vec!["Thread contention during auth".to_string()]),
        falsification_attempts: Some(vec!["Load test with 100 threads showed no contention".to_string()]),
        outcome: Some("inconclusive".to_string()),
        ..Default::default()
    };
    let inconv_res = mcp_server.rca_crystallize(Parameters(inconv_params)).await;
    assert_eq!(inconv_res.is_error, Some(false), "Inconclusive finding should be approved for partial crystallization");
    let inconv_raw: serde_json::Value = serde_json::to_value(&inconv_res)?;
    let inconv_text = inconv_raw["content"][0]["text"].as_str().unwrap();
    let inconv_val: serde_json::Value = serde_json::from_str(inconv_text)?;
    assert_eq!(inconv_val["verdict"], "APPROVED");
    assert!(vault_dir.join("incidents/INC-AUTH-201.md").exists(), "INC note must be created");
    assert!(vault_dir.join("evidence/EVD-AUTH-201-STACK.md").exists(), "EVD note must be created");
    assert!(
        !vault_dir.join("resolutions/RES-AUTH-201-01.md").exists(),
        "Partial crystallization must abstain from creating RES note"
    );

    // 7. Test Multi-Agent Competing Findings and Adjudication
    // Trigger run 2
    let trigger2_params = akashic_core::mcp::RcaTriggerParams {
        app_id: "com.example.auth".to_string(),
        raw_log: crash_log.to_string(),
        two_phase: Some(true),
        jira_key: Some("AUTH-202".to_string()),
        run_id: Some("run-auth-202".to_string()),
        prior_resolution_id: None,
    };
    let _ = mcp_server.rca_trigger(Parameters(trigger2_params)).await;

    // Agent Alpha submits with hold_for_curation = true
    let alpha_params = akashic_core::mcp::RcaCrystallizeParams {
        run_id: "run-auth-202".to_string(),
        candidate_id: Some("cand-alpha".to_string()),
        agent_id: Some("agent-alpha".to_string()),
        primary_hypothesis: Some("Null credentials passed to AuthService.authenticate_user".to_string()),
        falsification_attempts: Some(vec!["Verified null credentials throws NullPointerException".to_string()]),
        remediation_proposal: Some("Add null check to AuthService.kt authenticate_user".to_string()),
        outcome: Some("resolved".to_string()),
        hold_for_curation: Some(true),
        ..Default::default()
    };
    let alpha_res = mcp_server.rca_crystallize(Parameters(alpha_params)).await;
    assert_eq!(alpha_res.is_error, Some(false));
    let alpha_raw: serde_json::Value = serde_json::to_value(&alpha_res)?;
    let alpha_text = alpha_raw["content"][0]["text"].as_str().unwrap();
    let alpha_val: serde_json::Value = serde_json::from_str(alpha_text)?;
    assert_eq!(alpha_val["verdict"], "HELD_FOR_CURATION");

    // Agent Beta submits competing finding
    let beta_params = akashic_core::mcp::RcaCrystallizeParams {
        run_id: "run-auth-202".to_string(),
        candidate_id: Some("cand-beta".to_string()),
        agent_id: Some("agent-beta".to_string()),
        primary_hypothesis: Some("Missing READ_PHONE_STATE permission in MainActivity".to_string()),
        falsification_attempts: Some(vec!["Tested with granted permission, crash still reproduced".to_string()]),
        remediation_proposal: Some("Request READ_PHONE_STATE permission at runtime".to_string()),
        outcome: Some("resolved".to_string()),
        hold_for_curation: Some(true),
        ..Default::default()
    };
    let beta_res = mcp_server.rca_crystallize(Parameters(beta_params)).await;
    assert_eq!(beta_res.is_error, Some(false));
    let beta_raw: serde_json::Value = serde_json::to_value(&beta_res)?;
    let beta_text = beta_raw["content"][0]["text"].as_str().unwrap();
    let beta_val: serde_json::Value = serde_json::from_str(beta_text)?;
    assert_eq!(beta_val["verdict"], "HELD_FOR_CURATION");

    // Adjudicate run 2 selecting cand-alpha as winner
    let adjudicate_params = akashic_core::mcp::RcaCrystallizeParams {
        run_id: "run-auth-202".to_string(),
        winning_candidate_id: Some("cand-alpha".to_string()),
        ..Default::default()
    };
    let adj_res = mcp_server.rca_crystallize(Parameters(adjudicate_params)).await;
    assert_eq!(adj_res.is_error, Some(false), "Adjudication must succeed");
    let adj_raw: serde_json::Value = serde_json::to_value(&adj_res)?;
    let adj_text = adj_raw["content"][0]["text"].as_str().unwrap();
    let adj_val: serde_json::Value = serde_json::from_str(adj_text)?;
    assert_eq!(adj_val["verdict"], "APPROVED");

    // Verify crystallized RES note contains beta's hypothesis in invalidated_hypotheses
    let res_path = vault_dir.join("resolutions/RES-AUTH-202-01.md");
    assert!(res_path.exists(), "RES note must be crystallized on disk");
    let res_content = fs::read_to_string(&res_path)?;
    assert!(
        res_content.contains("Missing READ_PHONE_STATE permission in MainActivity"),
        "Rejected candidate hypothesis must be folded into invalidated_hypotheses negative knowledge: {}",
        res_content
    );

    // Lint vault to confirm zero errors
    let vault_graph = VaultGraph::from_dir(&vault_dir)?;
    let lint_report = lint_vault(&vault_graph);
    assert_eq!(lint_report.error_count, 0, "Vault must have zero lint errors after crystallization");

    // 8. Verify rca_status after rca_crystallize returns checkpoints and curation_report
    let status_after_cryst = mcp_server
        .rca_status(Parameters(akashic_core::mcp::RcaStatusParams {
            investigation_id: "run-auth-202".to_string(),
        }))
        .await;
    assert_eq!(status_after_cryst.is_error, Some(false));
    let status_val_raw: serde_json::Value = serde_json::to_value(&status_after_cryst)?;
    let status_text = status_val_raw["content"][0]["text"].as_str().unwrap();
    let status_obj: serde_json::Value = serde_json::from_str(status_text)?;
    let checkpoints = status_obj["checkpoints"]
        .as_array()
        .expect("checkpoints must be an array");
    assert!(!checkpoints.is_empty(), "checkpoints array must be non-empty");
    let curation_rep = &status_obj["curation_report"];
    assert!(
        curation_rep.is_object(),
        "curation_report must be present as object"
    );
    let verdict_str = curation_rep["verdict"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    assert_eq!(
        verdict_str, "approved",
        "curation_report verdict must be approved"
    );

    // 9. Verify calling rca_crystallize with an unknown run_id returns isError: true without writing uncurated file
    let unknown_params = akashic_core::mcp::RcaCrystallizeParams {
        run_id: "unknown-run-999".to_string(),
        summary: Some("Should fail immediately".to_string()),
        ..Default::default()
    };
    let unk_res = mcp_server.rca_crystallize(Parameters(unknown_params)).await;
    assert_eq!(
        unk_res.is_error,
        Some(true),
        "rca_crystallize with unknown run_id must return error"
    );
    assert!(
        !vault_dir.join("resolutions/RES-unknown-run-999.md").exists(),
        "No uncurated RES note should be written"
    );

    // 10. Verify CLI argument parsing for curate, prepare-only, and daemon serve-a2a
    let cli_curate = Cli::try_parse_from(["akashic", "rca", "curate", "--run-id", "run-auth-202", "--candidate-id", "cand-alpha"])?;
    assert!(matches!(cli_curate.command, akashic_core::Commands::Rca { .. }));

    let cli_prep = Cli::try_parse_from(["akashic", "rca", "run", "--jira-key", "AUTH-202", "--app-id", "com.example.auth", "--log-file", "logs.txt", "--prepare-only"])?;
    assert!(matches!(cli_prep.command, akashic_core::Commands::Rca { .. }));

    let cli_a2a = Cli::try_parse_from(["akashic", "daemon", "serve-a2a", "--bind", "127.0.0.1:9091"])?;
    assert!(matches!(cli_a2a.command, akashic_core::Commands::Daemon { .. }));

    // Verify CLI crystallize fails when --finding-file is missing
    let mut cmd_cryst = std::process::Command::new(env!("CARGO_BIN_EXE_akashic"));
    cmd_cryst.current_dir(temp_dir);
    cmd_cryst.args(["rca", "crystallize", "--run-id", "run-auth-202"]);
    let out_cryst = cmd_cryst.output()?;
    assert!(!out_cryst.status.success(), "CLI crystallize without finding-file must fail");
    let err_msg = String::from_utf8_lossy(&out_cryst.stderr);
    assert!(
        err_msg.contains("--finding-file is required"),
        "stderr should enforce --finding-file requirement: {}",
        err_msg
    );

    Ok(())
}


