use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = akashic_core::Cli::parse();
    println!("Executing akashic command: {:?}", cli.command);
    Ok(())
}
