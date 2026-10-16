use clap::{Parser, Subcommand};
use rca_conductor::{AnalysisSession, RunStatus};

#[derive(Parser, Debug)]
#[command(name = "rca-cli", version, about = "Root Cause Analysis CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start a new RCA session
    Start {
        #[arg(short, long)]
        target: String,
        #[arg(short, long)]
        session_id: Option<String>,
    },
    /// Inspect the status of an ongoing session
    Status {
        #[arg(short, long)]
        session_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { target, session_id } => {
            let id = session_id.unwrap_or_else(|| "sess-default".to_string());
            let session = AnalysisSession::new(&id, &target, 0);
            println!(
                "Session {} initiated for target {} with status {:?}",
                session.session_id, session.target, session.status
            );
        }
        Commands::Status { session_id } => {
            println!("Querying status for session: {}", session_id);
            let session = AnalysisSession::new(&session_id, "unknown", 0);
            println!("Status: {:?}", RunStatus::Pending);
            let _ = session;
        }
    }

    Ok(())
}
