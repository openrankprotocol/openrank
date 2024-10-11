use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut computer = openrank_computer::Node::init().await?;
    computer.node_recovery()?;
    computer.run().await
}
