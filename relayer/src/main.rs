use dotenv::dotenv;
use env_logger;
use openrank_relayer::{self, SQLRelayer};
use std::error::Error;

use api::server::serve;

pub mod api;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    env_logger::init();
    let dbs = vec![("../verifier/local-storage", "tx")];
    let mut relayer = SQLRelayer::init(dbs).await;
    let server_task = tokio::spawn(async {
        serve().await;
    });

    relayer.start().await;

    Ok(())
}
