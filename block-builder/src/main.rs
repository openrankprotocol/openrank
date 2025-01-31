use openrank_block_builder as block_builder;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut bb = block_builder::Node::init().await?;

    let res = bb.run().await;
    if let Err(e) = res {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    Ok(())
}
