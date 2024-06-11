use openrank_computer;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	openrank_computer::run().await
}
