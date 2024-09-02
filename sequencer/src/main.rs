use openrank_sequencer::{self, SequencerNode};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut seq_node = SequencerNode::init().await?;
    seq_node.run().await
}
