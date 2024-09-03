use openrank_block_builder::{self, BlockBuilderNode};
use openrank_sequencer::{self, SequencerNode};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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
    Ok(())
}
