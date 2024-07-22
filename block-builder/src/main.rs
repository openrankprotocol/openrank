use openrank_block_builder;
use openrank_sequencer;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	tokio::spawn(async { openrank_block_builder::run().await.unwrap() });
	tokio::spawn(async { openrank_sequencer::run().await.unwrap() });
	Ok(())
}
