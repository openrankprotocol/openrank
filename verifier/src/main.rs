use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut vn = openrank_verifier::Node::init().await?;
    vn.node_recovery()?;
    vn.run().await
}
