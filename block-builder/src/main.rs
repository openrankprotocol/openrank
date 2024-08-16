use openrank_block_builder;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	openrank_block_builder::run().await
}
