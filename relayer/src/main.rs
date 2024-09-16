use dotenv::dotenv;
use env_logger;
use openrank_relayer::{self, SQLRelayer};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    env_logger::init();
    let mut relayer = SQLRelayer::init("../verifier/local-storage").await;
    relayer.start().await;

    Ok(())
}
