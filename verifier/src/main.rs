use openrank_verifier::{self, VerifierNode};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let mut vn = VerifierNode::init().await?;
	vn.node_recovery()?;
	vn.run().await
}
