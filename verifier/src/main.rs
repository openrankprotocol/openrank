use openrank_verifier;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	openrank_verifier::run().await
}
