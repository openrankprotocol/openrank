use clap::{Parser, Subcommand};
use openrank_smart_contract_client::ComputeManagerClient;
use std::error::Error;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    PostOnChain,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::PostOnChain) => {
            let smc = ComputeManagerClient::init()?;
            let res = smc.run().await;
            if let Err(e) = res {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        },
        None => {
            let mut bb = openrank_block_builder::Node::init().await?;
            let mut sequencer = openrank_sequencer::Node::init().await?;
            let block_builder_task = tokio::spawn(async move {
                let res = bb.run().await;
                if let Err(e) = res {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            });
            let sequencer_task = tokio::spawn(async move {
                let res = sequencer.run().await;
                if let Err(e) = res {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            });
            let _ = tokio::join!(block_builder_task, sequencer_task);
        },
    }
    Ok(())
}
