use openrank_verifier as verifier;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut vn = verifier::Node::init().await?;
    vn.node_recovery()?;
    vn.run().await
}
