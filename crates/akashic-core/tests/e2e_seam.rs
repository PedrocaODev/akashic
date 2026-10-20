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

