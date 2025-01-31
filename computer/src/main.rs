use openrank_computer as computer;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut computer = computer::Node::init().await?;
    computer.node_recovery().await?;
    computer.run().await
}
