use openrank_block_builder;
use openrank_sequencer;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let block_builder_task = tokio::spawn(async {
		match openrank_block_builder::run().await {
			Ok(_) => (),
			Err(e) => {
				eprintln!("{}", e);
				std::process::exit(1);
			},
		}
	});
	let sequencer_task = tokio::spawn(async {
		match openrank_sequencer::run().await {
			Ok(_) => (),
			Err(e) => {
				eprintln!("{}", e);
				std::process::exit(1);
			},
		}
	});
	let _ = tokio::join!(block_builder_task, sequencer_task);
	Ok(())
}
