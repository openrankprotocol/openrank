use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut seq_node = openrank_sequencer::Node::init().await?;
    seq_node.run().await
}
