use openrank_block_builder;
use openrank_sequencer;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let block_builder_task = tokio::spawn(async { openrank_block_builder::run().await.unwrap() });
	let sequencer_task = tokio::spawn(async { openrank_sequencer::run().await.unwrap() });
	let _ = tokio::join!(block_builder_task, sequencer_task);
	Ok(())
}
