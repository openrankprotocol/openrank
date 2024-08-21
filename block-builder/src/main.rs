use openrank_block_builder;
use openrank_sequencer;
use std::error::Error;
use tracing::error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let block_builder_task = tokio::spawn(async {
        match openrank_block_builder::run().await {
            Ok(_) => (),
            Err(e) => {
                error!("{}", e);
                std::process::exit(1);
            },
        }
    });
    let sequencer_task = tokio::spawn(async {
        match openrank_sequencer::run().await {
            Ok(_) => (),
            Err(e) => {
                error!("{}", e);
                std::process::exit(1);
            },
        }
    });
    let (res1, res2) = tokio::join!(block_builder_task, sequencer_task);
    if let Err(e) = res1 {
        error!("{}", e);
    }
    if let Err(e) = res2 {
        error!("{}", e);
    }
    Ok(())
}
