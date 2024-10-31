use openrank_sequencer as sequencer;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut seq_node = sequencer::Node::init().await?;
    seq_node.run().await
}
