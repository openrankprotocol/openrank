use api::server::serve;
use dotenv::dotenv;
use env_logger;
use openrank_relayer::{self, SQLRelayer};
use std::env;
use std::error::Error;

pub mod api;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let is_reindex = args.contains(&"reindex".to_string());

    let dbs = vec![
        ("../block-builder/local-storage", "tx"),
        //("../block-builder/local-storage", "metadata")
    ];
    let mut relayer = SQLRelayer::init(dbs, is_reindex).await;

    let server_task = tokio::spawn(async {
        serve().await;
    });

    relayer.start().await;

    Ok(())
}
