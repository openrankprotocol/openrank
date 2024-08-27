use openrank_computer::ComputerNode;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let mut computer = ComputerNode::init().await?;
	computer.node_recovery()?;
	computer.run().await
}
