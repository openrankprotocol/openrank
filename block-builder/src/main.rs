use clap::{Parser, Subcommand};
use openrank_block_builder as block_builder;
use openrank_sequencer as sequencer;
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
            let mut bb = block_builder::Node::init().await?;
            let mut sequencer = sequencer::Node::init().await?;

            // Recover the state from the local DB
            bb.node_recovery()?;

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
