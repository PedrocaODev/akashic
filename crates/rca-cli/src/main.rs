use clap::{Parser, Subcommand};
use rca_conductor::{
    CliSubprocessRunner, MockStage4Runner, Pipeline, PipelineInput, Stage4Runner, StateStore,
};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "rca-cli", version, about = "Root Cause Analysis Orchestrator CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start a new RCA investigation pipeline session
    Start {
        #[arg(long)]
        jira_key: String,

        #[arg(long)]
        app_id: String,

        #[arg(long)]
        device: String,

        #[arg(long)]
        android_version: String,

        #[arg(long)]
        log_file: PathBuf,

        #[arg(long, default_value = "./rca_conductor.db")]
        db_path: PathBuf,

        #[arg(long)]
        vault_path: PathBuf,

        #[arg(long)]
        alexandria_db: Option<PathBuf>,

        #[arg(long)]
        session_id: Option<String>,

        #[arg(long)]
        prior_resolution_id: Option<String>,

        #[arg(long, default_value_t = false)]
        mock_llm: bool,
    },

    /// Resume an existing RCA investigation pipeline session
    Resume {
        #[arg(long)]
        session_id: String,

        #[arg(long, default_value = "./rca_conductor.db")]
        db_path: PathBuf,

        #[arg(long)]
        vault_path: PathBuf,

        #[arg(long)]
        alexandria_db: Option<PathBuf>,

        #[arg(long, default_value_t = false)]
        mock_llm: bool,
    },

    /// Inspect the status and checkpoints of an RCA session
    Status {
        #[arg(long)]
        session_id: String,

        #[arg(long, default_value = "./rca_conductor.db")]
        db_path: PathBuf,
    },

    /// List all known RCA sessions
    List {
        #[arg(long, default_value = "./rca_conductor.db")]
        db_path: PathBuf,
    },
}

async fn run_pipeline<R: Stage4Runner>(
    state_store: StateStore,
    runner: R,
    input: PipelineInput,
) -> Result<(), Box<dyn std::error::Error>> {
    let pipeline = Pipeline::new(state_store, runner);
    let result = pipeline.run(input).await?;
    println!(
        "Session {} finished with stage={} deduplicated={}",
        result.session_id,
        if result.deduplicated {
            "DEDUPLICATED"
        } else {
            "RESOLVED"
        },
        result.deduplicated
    );
    if let Some(ref sig) = result.crash_signature {
        println!("  Crash signature: {}", sig);
    }
    if let Some(ref inc) = result.incident_node_id {
        println!("  Incident node: {}", inc);
    }
    if let Some(ref res) = result.resolution_node_id {
        println!("  Resolution node: {}", res);
    }
    Ok(())
}

async fn resume_pipeline<R: Stage4Runner>(
    state_store: StateStore,
    runner: R,
    session_id: &str,
    vault_path: &std::path::Path,
    alexandria_db: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let pipeline = Pipeline::new(state_store, runner);
    let result = pipeline
        .resume_session(session_id, Some(vault_path), alexandria_db)
        .await?;
    println!(
        "Resumed session {} completed successfully (stage={})",
        result.session_id,
        if result.deduplicated {
            "DEDUPLICATED"
        } else {
            "RESOLVED"
        }
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start {
            jira_key,
            app_id,
            device,
            android_version,
            log_file,
            db_path,
            vault_path,
            alexandria_db,
            session_id,
            prior_resolution_id,
            mock_llm,
        } => {
            let raw_logs = std::fs::read_to_string(&log_file).map_err(|e| {
                format!("Failed to read log file '{}': {}", log_file.display(), e)
            })?;

            let state_store = StateStore::open(&db_path)?;
            let input = PipelineInput {
                session_id,
                jira_key,
                app_id,
                device,
                android_version,
                raw_logs,
                vault_path,
                alexandria_db,
                prior_resolution_id,
                policy_path: None,
            };

            if mock_llm {
                run_pipeline(state_store, MockStage4Runner::new(), input).await?;
            } else {
                run_pipeline(state_store, CliSubprocessRunner::new(), input).await?;
            }
        }

        Commands::Resume {
            session_id,
            db_path,
            vault_path,
            alexandria_db,
            mock_llm,
        } => {
            let state_store = StateStore::open(&db_path)?;
            if mock_llm {
                resume_pipeline(
                    state_store,
                    MockStage4Runner::new(),
                    &session_id,
                    &vault_path,
                    alexandria_db.as_deref(),
                )
                .await?;
            } else {
                resume_pipeline(
                    state_store,
                    CliSubprocessRunner::new(),
                    &session_id,
                    &vault_path,
                    alexandria_db.as_deref(),
                )
                .await?;
            }
        }

        Commands::Status {
            session_id,
            db_path,
        } => {
            let state_store = StateStore::open(&db_path)?;
            if let Some(session) = state_store.get_session(&session_id)? {
                println!("Session ID:       {}", session.session_id);
                println!("Jira Key:         {}", session.jira_key);
                println!("Stage:            {}", session.stage);
                println!("Status:           {}", session.status);
                println!(
                    "Crash Signature:  {}",
                    session.crash_signature.as_deref().unwrap_or("<none>")
                );
                println!("Created At:       {}", session.created_at);
                println!("Updated At:       {}", session.updated_at);

                let checkpoints = state_store.get_checkpoints(&session_id)?;
                println!("Checkpoints ({}):", checkpoints.len());
                for cp in checkpoints {
                    println!(
                        "  [{}] Stage: {:<20} Recorded At: {}",
                        cp.checkpoint_id, cp.stage, cp.recorded_at
                    );
                }
            } else {
                eprintln!("Session '{}' not found in {}", session_id, db_path.display());
                std::process::exit(1);
            }
        }

        Commands::List { db_path } => {
            let state_store = StateStore::open(&db_path)?;
            let sessions = state_store.list_sessions()?;
            if sessions.is_empty() {
                println!("No sessions found in {}", db_path.display());
            } else {
                println!(
                    "{:<36} {:<15} {:<20} {:<12}",
                    "SESSION ID", "JIRA KEY", "STAGE", "STATUS"
                );
                println!("{}", "-".repeat(88));
                for s in sessions {
                    println!(
                        "{:<36} {:<15} {:<20} {:<12}",
                        s.session_id, s.jira_key, s.stage, s.status
                    );
                }
            }
        }
    }

    Ok(())
}
