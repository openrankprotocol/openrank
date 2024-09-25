use clap::{Parser, Subcommand};
use openrank_block_builder::{self, BlockBuilderNode};
use openrank_sequencer::{self, SequencerNode};
use smart_contract_client::JobManagerClient;
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
            let smc = JobManagerClient::init()?;
            let res = smc.run().await;
            if let Err(e) = res {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        },
        None => {
            let mut bb = BlockBuilderNode::init().await?;
            let mut sequencer = SequencerNode::init().await?;
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
