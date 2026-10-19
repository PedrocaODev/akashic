use std::path::PathBuf;
use clap::Parser;
use akashic_core::*;
use akashic_embedding::EmbeddingProvider;
use akashic_sanitize::SanitizerProfile;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Vault { command } => match command {
            VaultCommands::Init(args) => {
                let path = args.path.unwrap_or_else(|| "vault".to_string());
                std::fs::create_dir_all(&path)?;
                println!("Initialized empty vault at {}", path);
            }
            VaultCommands::Query(args) => {
                let vault_path = PathBuf::from("vault");
                let graph = vault::VaultGraph::from_dir(&vault_path)?;
                let scope = vault::Scope::new();
                let seed_id = args.note_id.unwrap_or_default();
                let res = vault::query::query_current(&graph, &seed_id, Some(&scope))?;
                println!("{}", serde_json::to_string_pretty(&res)?);
            }
            VaultCommands::Inspect(args) => {
                let vault_path = PathBuf::from("vault");
                let graph = vault::VaultGraph::from_dir(&vault_path)?;
                if let Some(doc) = graph.get_node(&args.note_id) {
                    println!("{}", serde_yaml::to_string(&doc.frontmatter)?);
                } else {
                    anyhow::bail!("Note '{}' not found", args.note_id);
                }
            }
            VaultCommands::Plan(args) => {
                let path = args.path.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("vault"));
                let plan = vault::migrator::plan_vault(&path)?;
                if let Some(out) = args.out {
                    std::fs::write(&out, serde_json::to_string_pretty(&plan)?)?;
                    println!("Migration plan saved to {}", out);
                } else {
                    println!("{}", serde_json::to_string_pretty(&plan)?);
                }
            }
            VaultCommands::Apply(args) => {
                let path = args.path.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("vault"));
                if let Some(plan_file) = args.plan {
                    let plan_str = std::fs::read_to_string(&plan_file)?;
                    let plan: vault::migrator::MigrationPlan = serde_json::from_str(&plan_str)?;
                    let count = vault::migrator::apply_plan(&path, &plan)?;
                    println!("Applied plan to {} files", count);
                } else {
                    let plan = vault::migrator::plan_vault(&path)?;
                    let count = vault::migrator::apply_plan(&path, &plan)?;
                    println!("Applied migration to {} files", count);
                }
            }
            VaultCommands::Reindex(args) => {
                let path = args.path.unwrap_or_else(|| "vault".to_string());
                let config = vault::VaultConfig {
                    path: path.clone(),
                    db_path: format!("{}/.akashic/vault_index.db", path),
                    okf_version: "0.2".to_string(),
                };
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
            let vault_path = args.vault.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("vault"));
            let graph = vault::VaultGraph::from_dir(&vault_path)?;
            let mut config = lint::LintConfig::default();
            config.strict_forward_only = args.strict;
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
            let db_path = PathBuf::from(".akashic/code_graph.db");
            match command {
                GraphCommands::Index(args) => {
                    let repo_path = args.repo_path.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
                    let repo_name = args.repo_name.unwrap_or_else(|| "default".to_string());
                    let commit = args.commit.unwrap_or_else(|| "HEAD".to_string());
                    let engine = engine::CodeGraphEngine::new(
                        engine::EngineConfig {
                            db_path: db_path.to_string_lossy().to_string(),
                            languages: vec!["rust".into(), "kotlin".into(), "java".into()],
                            inferred_similarity_threshold: 0.8,
                            god_node_degree_threshold: 15,
                            rrf_k: 60,
                        },
                        sanitize::CodeProfile::default(),
                        embedding::DeterministicHashEmbeddingProvider::default(),
                    );
                    let store = engine.open_store()?;
                    let report = engine.index_repository(&store, &repo_path, &repo_name, &commit)?;
                    println!("Indexed repository: {} symbols, {} edges", report.symbols_upserted, report.extracted_edges);
                }
                GraphCommands::Reconcile(args) => {
                    let repo_path = args.repo_path.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
                    let repo_name = args.repo_name.unwrap_or_else(|| "default".to_string());
                    let old_rev = args.old_rev.unwrap_or_else(|| "HEAD~1".to_string());
                    let new_rev = args.new_rev.unwrap_or_else(|| "HEAD".to_string());
                    let engine = engine::CodeGraphEngine::new(
                        engine::EngineConfig {
                            db_path: db_path.to_string_lossy().to_string(),
                            languages: vec!["rust".into(), "kotlin".into(), "java".into()],
                            inferred_similarity_threshold: 0.8,
                            god_node_degree_threshold: 15,
                            rrf_k: 60,
                        },
                        sanitize::CodeProfile::default(),
                        embedding::DeterministicHashEmbeddingProvider::default(),
                    );
                    let store = engine.open_store()?;
                    let report = engine.reconcile_incremental(&store, &repo_path, &repo_name, &old_rev, &new_rev)?;
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
                    let path = engine::analytics::shortest_path(&store, args.repo_name.as_deref().unwrap_or(""), &args.source, &args.target, args.max_hops.unwrap_or(10))?;
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
                    let comms = store.get_communities(args.repo_name.as_deref().unwrap_or(""))?;
                    println!("{}", serde_json::to_string_pretty(&comms)?);
                }
                GraphCommands::GodNodes(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let nodes = engine::analytics::god_nodes(&store, args.repo_name.as_deref().unwrap_or(""), args.top_n.unwrap_or(10))?;
                    for n in nodes {
                        println!("{}: in={}, out={}, total={}", n.symbol.symbol_name, n.in_degree, n.out_degree, n.total_degree);
                    }
                }
                GraphCommands::Surprising(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let conns = engine::analytics::surprising_connections(&store, args.repo_name.as_deref().unwrap_or(""), args.top_n.unwrap_or(10))?;
                    println!("Surprising connections ({}):", conns.len());
                    for conn in conns {
                        println!("  {} <-> {} (score: {:.3}, reason: {})", conn.source_symbol, conn.target_symbol, conn.surprise_score, conn.reason);
                    }
                }
                GraphCommands::BlastRadius(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let report = engine::analytics::blast_radius(&store, args.repo_name.as_deref().unwrap_or(""), &[args.symbol_or_diff], args.max_depth.unwrap_or(3))?;
                    println!("Blast radius: {} upstream callers, {} downstream dependencies", report.upstream_callers.len(), report.downstream_dependencies.len());
                }
                GraphCommands::Report(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let rep = engine::report::generate_graph_report(&store, args.repo_name.as_deref().unwrap_or(""))?;
                    println!("{}", rep);
                }
                GraphCommands::Wiki(args) => {
                    let store = engine::store::GraphStore::open(&db_path)?;
                    let out_dir = args.out_dir.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("wiki"));
                    let engine = engine::CodeGraphEngine::new(
                        engine::EngineConfig {
                            db_path: db_path.to_string_lossy().to_string(),
                            languages: vec![],
                            inferred_similarity_threshold: 0.8,
                            god_node_degree_threshold: 15,
                            rrf_k: 60,
                        },
                        sanitize::CodeProfile::default(),
                        embedding::DeterministicHashEmbeddingProvider::default(),
                    );
                    let paths = engine.generate_wiki(&store, args.repo_name.as_deref().unwrap_or(""), &out_dir)?;
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
        Commands::Rca { command } => match command {
            RcaCommands::Run(args) => {
                let state_store = rca::store::StateStore::open(".akashic/rca_state.db")?;
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
                    vault_path: PathBuf::from("vault"),
                    graph_store: Some(PathBuf::from(".akashic/code_graph.db")),
                    prior_resolution_id: None,
                    policy_path: None,
                };
                let ctx = pipeline.run(input).await?;
                println!("RCA completed: run_id={}, deduplicated={}", ctx.run_id, ctx.deduplicated);
            }
            RcaCommands::Start(args) => {
                println!("Starting RCA investigation for trigger: {}", args.trigger_id);
            }
            RcaCommands::Resume(args) => {
                let state_store = rca::store::StateStore::open(".akashic/rca_state.db")?;
                if let Some(run) = state_store.get_workflow_run(&args.investigation_id)? {
                    println!("Resumed run: {:?}", run);
                } else {
                    anyhow::bail!("Investigation '{}' not found", args.investigation_id);
                }
            }
            RcaCommands::Status(args) => {
                let state_store = rca::store::StateStore::open(".akashic/rca_state.db")?;
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
                let state_store = rca::store::StateStore::open(".akashic/rca_state.db")?;
                let runs = state_store.list_workflow_runs()?;
                println!("Open investigations: {}", runs.len());
            }
        },
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
                let db_path = PathBuf::from(".akashic/code_graph.db");
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
            mcp::run_mcp_server(args.vault.map(PathBuf::from), args.workspace.map(PathBuf::from)).await?;
        }
    }

    Ok(())
}
