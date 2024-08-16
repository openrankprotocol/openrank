use openrank_sequencer;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	openrank_sequencer::run().await
}
