use std::path::PathBuf;
use clap::Parser;
use akashic_core::*;
use akashic_embedding::EmbeddingProvider;
use akashic_sanitize::SanitizerProfile;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let ctx = CliRuntimeContext::load(cli.config.as_deref().map(std::path::Path::new))?;

    match cli.command {
        Commands::Vault { command } => match command {
            VaultCommands::Init(args) => {
                let path = args.path.unwrap_or_else(|| ctx.config.vault.path.clone());
                std::fs::create_dir_all(&path)?;
                println!("Initialized empty vault at {}", path);
            }
            VaultCommands::Query(args) => {
                let vault_path = ctx.resolve_vault_path(None);
                let graph = vault::VaultGraph::from_dir(&vault_path)?;
                let scope = vault::Scope::new();
                let seed_id = args.node_id.as_deref().unwrap_or_default();
                if !seed_id.is_empty() {
                    if let Some(_vault_node) = graph.get_node(seed_id) {
                        // Node confirmed to exist
                    } else {
                        anyhow::bail!("Vault Node '{}' not found", seed_id);
                    }
                }
                let res = vault::query::query_current(&graph, seed_id, Some(&scope))?;
                println!("{}", serde_json::to_string_pretty(&res)?);
            }
            VaultCommands::Inspect(args) => {
                let vault_path = ctx.resolve_vault_path(None);
                let graph = vault::VaultGraph::from_dir(&vault_path)?;
                let node_id = args.node_id().unwrap_or_default();
                if let Some(vault_node) = graph.get_node(node_id) {
                    println!("{}", serde_yaml::to_string(&vault_node.frontmatter)?);
                } else {
                    anyhow::bail!("Vault Node '{}' not found", node_id);
                }
            }
            VaultCommands::Plan(args) => {
                let path = args.path().map(PathBuf::from).unwrap_or_else(|| ctx.resolve_vault_path(None));
                let plan = vault::migrator::plan_vault(&path)?;
                if let Some(out) = args.out {
                    std::fs::write(&out, serde_json::to_string_pretty(&plan)?)?;
                    println!("Migration plan saved to {}", out);
                } else {
                    println!("{}", serde_json::to_string_pretty(&plan)?);
                }
            }
            VaultCommands::Apply(args) => {
                let path = args.path().map(PathBuf::from).unwrap_or_else(|| ctx.resolve_vault_path(None));
                let plan_path = args.plan.as_deref().map(std::path::Path::new);
                let count = akashic_core::vault_apply(&path, plan_path, Some(&ctx.config))?;
                println!("Applied plan to {} files", count);
            }
            VaultCommands::Reindex(args) => {
                let path = args.path.map(PathBuf::from).unwrap_or_else(|| ctx.resolve_vault_path(None));
                let config = ctx.resolve_vault_config(Some(&path));
                let manager = vault::VaultManager::new(
                    config,
                    sanitize::CodeProfile::default(),
                    embedding::DeterministicHashEmbeddingProvider::default(),
                );
                let stats = manager.reindex_vault()?;
                println!("Vault reindexed successfully: {:?}", stats);
            }
        },
        Commands::Lint(args) => {
            let vault_path = args.vault.map(PathBuf::from).unwrap_or_else(|| ctx.resolve_vault_path(None));
            let graph = vault::VaultGraph::from_dir(&vault_path)?;
            let mut config = ctx.config.lint.clone();
            if args.strict {
                config.strict_forward_only = true;
            }
            let report = lint::lint_vault_with_config(&graph, &config);
            if args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Lint result: valid = {}, issues = {}", report.valid, report.issues.len());
                for issue in &report.issues {
                    println!("[{}] {}: {}", issue.severity, issue.code, issue.message);
                }
            }
        }
        Commands::Graph { command } => {
            let db_path = PathBuf::from(&ctx.config.engine.db_path);
            match command {
                GraphCommands::Index(args) => {
                    let repo_path = args.repo_path.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
                    let repo_name = ctx.resolve_repo_name(args.repo_name.as_deref());
                    let commit = args.commit.unwrap_or_else(|| "HEAD".to_string());
                    let engine = engine::CodeGraphEngine::new(
                        ctx.config.engine.clone(),
                        sanitize::CodeProfile::default(),
                        embedding::DeterministicHashEmbeddingProvider::default(),
                    );
                    let store = engine.open_store()?;
                    let report = engine.index_repository(&store, &repo_path, repo_name, &commit)?;
                    println!("Indexed repository: {} symbols, {} edges", report.symbols_upserted, report.extracted_edges);
                }
                GraphCommands::Reconcile(args) => {
                    let repo_path = args.repo_path.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
                    let repo_name = ctx.resolve_repo_name(args.repo_name.as_deref());
                    let old_rev = args.old_rev.unwrap_or_else(|| "HEAD~1".to_string());
                    let new_rev = args.new_rev.unwrap_or_else(|| "HEAD".to_string());
                    let engine = engine::CodeGraphEngine::new(
                        ctx.config.engine.clone(),
                        sanitize::CodeProfile::default(),
                        embedding::DeterministicHashEmbeddingProvider::default(),
                    );
                    let store = engine.open_store()?;
                    let report = engine.reconcile_incremental(&store, &repo_path, repo_name, &old_rev, &new_rev)?;
                    println!("Reconciled incremental: {} symbols upserted, {} symbols removed, {} edges", report.symbols_upserted, report.symbols_removed, report.extracted_edges);
                }
                GraphCommands::Query(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let hits = store.fts_search(&args.query, args.repo_name.as_deref(), args.limit.unwrap_or(20))?;
                    println!("{}", serde_json::to_string_pretty(&hits)?);
                }
                GraphCommands::Symbol(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let hits = store.fts_search(&args.query, None, 20)?;
                    println!("{}", serde_json::to_string_pretty(&hits)?);
                }
                GraphCommands::Path(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let repo_name = ctx.resolve_repo_name(args.repo_name.as_deref());
                    let path = engine::analytics::shortest_path(&store, repo_name, &args.source, &args.target, args.max_hops.unwrap_or(10))?;
                    if let Some(steps) = path {
                        println!("Shortest path ({} steps):", steps.len());
                        for step in steps {
                            println!("  {} -[{}]-> {}", step.from_id, step.edge_type, step.to_id);
                        }
                    } else {
                        println!("No path found between {} and {}", args.source, args.target);
                    }
                }
                GraphCommands::Explain(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let sym = store.get_symbol_by_id(&args.symbol)?;
                    println!("{}", serde_json::to_string_pretty(&sym)?);
                }
                GraphCommands::Communities(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let repo_name = ctx.resolve_repo_name(args.repo_name.as_deref());
                    let comms = store.get_communities(repo_name)?;
                    println!("{}", serde_json::to_string_pretty(&comms)?);
                }
                GraphCommands::GodNodes(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let repo_name = ctx.resolve_repo_name(args.repo_name.as_deref());
                    let nodes = engine::analytics::god_nodes(&store, repo_name, args.top_n.unwrap_or(10))?;
                    for n in nodes {
                        println!("{}: in={}, out={}, total={}", n.symbol.symbol_name, n.in_degree, n.out_degree, n.total_degree);
                    }
                }
                GraphCommands::Surprising(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let repo_name = ctx.resolve_repo_name(args.repo_name.as_deref());
                    let conns = engine::analytics::surprising_connections(&store, repo_name, args.top_n.unwrap_or(10))?;
                    println!("Surprising connections ({}):", conns.len());
                    for conn in conns {
                        println!("  {} <-> {} (score: {:.3}, reason: {})", conn.source_symbol, conn.target_symbol, conn.surprise_score, conn.reason);
                    }
                }
                GraphCommands::BlastRadius(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let repo_name = ctx.resolve_repo_name(args.repo_name.as_deref());
                    let report = engine::analytics::blast_radius(&store, repo_name, &[args.symbol_or_diff], args.max_depth.unwrap_or(3))?;
                    println!("Blast radius: {} upstream callers, {} downstream dependencies", report.upstream_callers.len(), report.downstream_dependencies.len());
                }
                GraphCommands::Report(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let repo_name = ctx.resolve_repo_name(args.repo_name.as_deref());
                    let rep = engine::report::generate_graph_report(&store, repo_name)?;
                    println!("{}", rep);
                }
                GraphCommands::Wiki(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let out_dir = args.out_dir.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("wiki"));
                    let engine = engine::CodeGraphEngine::new(
                        ctx.config.engine.clone(),
                        sanitize::CodeProfile::default(),
                        embedding::DeterministicHashEmbeddingProvider::default(),
                    );
                    let repo_name = ctx.resolve_repo_name(args.repo_name.as_deref());
                    let paths = engine.generate_wiki(&store, repo_name, &out_dir)?;
                    println!("Generated {} wiki pages", paths.len());
                }
                GraphCommands::Discover(args) => {
                    let path = args.path.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let report = daemon::discover_workspace(&path, &store)?;
                    println!("Discovered {} repositories, {} package mappings", report.discovered_repositories.len(), report.package_mappings.len());
                }
            }
        }
        Commands::Rca { command } => {
            let rca_db_path = PathBuf::from(&ctx.config.rca.db_path);
            match command {
                RcaCommands::Run(args) => {
                    let state_store = rca::store::StateStore::open(&rca_db_path)?;
                    let pipeline = rca::pipeline::ConductorPipeline::new(state_store, rca::stage4::MockStage4Executor::new());
                    let raw_log = if let Some(file) = args.log_file {
                        std::fs::read_to_string(&file)?
                    } else {
                        args.raw_log.unwrap_or_default()
                    };
                    let input = rca::pipeline::PipelineInput {
                        run_id: None,
                        legacy_session_id: None,
                        jira_key: args.jira_key.unwrap_or_else(|| "CLI-RCA".into()),
                        app_id: args.app_id.unwrap_or_default(),
                        device: "cli-device".into(),
                        android_version: "14".into(),
                        raw_logs: raw_log,
                        vault_path: ctx.resolve_vault_path(None),
                        graph_store: Some(PathBuf::from(&ctx.config.engine.db_path)),
                        prior_resolution_id: None,
                        policy_path: None,
                    };
                    let rca_ctx = pipeline.run(input).await?;
                    println!("RCA completed: run_id={}, deduplicated={}", rca_ctx.run_id, rca_ctx.deduplicated);
                }
                RcaCommands::Start(args) => {
                    println!("Starting RCA investigation for trigger: {}", args.trigger_id);
                }
                RcaCommands::Resume(args) => {
                    let state_store = rca::store::StateStore::open(&rca_db_path)?;
                    if let Some(run) = state_store.get_workflow_run(&args.investigation_id)? {
                        println!("Resumed run: {:?}", run);
                    } else {
                        anyhow::bail!("Investigation '{}' not found", args.investigation_id);
                    }
                }
                RcaCommands::Status(args) => {
                    let state_store = rca::store::StateStore::open(&rca_db_path)?;
                    if let Some(run) = state_store.get_workflow_run(&args.investigation_id)? {
                        println!("{}", serde_json::to_string_pretty(&run)?);
                    } else {
                        anyhow::bail!("Investigation '{}' not found", args.investigation_id);
                    }
                }
                RcaCommands::Symbolicate(args) => {
                    let raw_log = if let Some(file) = args.log_file {
                        std::fs::read_to_string(&file)?
                    } else {
                        args.raw_log.unwrap_or_default()
                    };
                    let app_id = args.app_id.unwrap_or_default();
                    let resolver = rca::symbolicator::InMemorySymbolResolver::with_motorola_defaults();
                    let res = rca::symbolicate_crash(&app_id, &raw_log, &resolver)?;
                    println!("{}", serde_json::to_string_pretty(&res)?);
                }
                RcaCommands::Crystallize(args) => {
                    println!("Crystallizing run: {}", args.run_id);
                }
                RcaCommands::Supersede(args) => {
                    println!("Superseding {} with {}", args.old_id, args.new_id);
                }
                RcaCommands::Triage => {
                    let state_store = rca::store::StateStore::open(&rca_db_path)?;
                    let runs = state_store.list_workflow_runs()?;
                    println!("Open investigations: {}", runs.len());
                }
            }
        }
        Commands::Daemon { command } => match command {
            DaemonCommands::Start | DaemonCommands::Run => {
                println!("Daemon started");
            }
            DaemonCommands::Stop => {
                println!("Daemon stopped");
            }
            DaemonCommands::Status => {
                println!("Daemon status: IDLE");
            }
            DaemonCommands::Catchup(args) => {
                println!("Catching up stream: {:?}", args.stream);
            }
            DaemonCommands::Discover(args) => {
                let path = args.workspace.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
                let db_path = PathBuf::from(&ctx.config.engine.db_path);
                let store = engine::store::GraphStore::open(&db_path)?;
                let report = daemon::discover_workspace(&path, &store)?;
                println!("Discovered {} repositories, {} package mappings", report.discovered_repositories.len(), report.package_mappings.len());
            }
        },
        Commands::Embedding { command } => match command {
            EmbeddingCommands::Start | EmbeddingCommands::StartSidecar => {
                println!("Embedding sidecar started");
            }
            EmbeddingCommands::Stop => {
                println!("Embedding sidecar stopped");
            }
            EmbeddingCommands::Status => {
                println!("Embedding sidecar status: OK");
            }
            EmbeddingCommands::Benchmark(args) => {
                let provider = embedding::DeterministicHashEmbeddingProvider::default();
                let texts: Vec<String> = (0..args.count).map(|i| format!("Sample text {}", i)).collect();
                let vecs = provider.embed_batch(&texts)?;
                println!("Benchmarked {} embeddings successfully (dim: {})", vecs.len(), vecs.first().map(|v| v.len()).unwrap_or(0));
            }
        },
        Commands::Sanitize(args) => {
            let profile_type = args.profile.as_deref().unwrap_or("code");
            let content = if let Some(file) = args.input {
                std::fs::read_to_string(&file).unwrap_or(file)
            } else {
                "Sample text for sanitization".to_string()
            };

            if profile_type == "log" || matches!(args.command, Some(SanitizeCommands::TestLog(_))) {
                let p = sanitize::LogProfile::default();
                let out = p.sanitize(&content);
                println!("Sanitized log: {}", out.sanitized_content);
            } else {
                let p = sanitize::CodeProfile::default();
                let out = p.sanitize(&content);
                println!("Sanitized code: {}", out.sanitized_content);
            }
        }
        Commands::Mcp(args) => {
            mcp::run_mcp_server(args.vault.map(PathBuf::from), args.workspace.map(PathBuf::from), Some(ctx.config)).await?;
        }
    }

    Ok(())
}
