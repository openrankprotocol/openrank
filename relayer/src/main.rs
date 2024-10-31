use api::server::serve;
use dotenv::dotenv;
use openrank_common::{config, db};
use openrank_relayer::{self, SQLRelayer};
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;

pub mod api;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The configuration for the Relayer.
pub struct Config {
    pub database: db::Config,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let is_reindex = args.contains(&"reindex".to_string());

    let config_loader = config::Loader::new("openrank-relayer")?;
    let config: Config = config_loader.load_or_create(include_str!("../config.toml"))?;
    let mut relayer = SQLRelayer::init(config.database, is_reindex).await;

    let serve_job = tokio::spawn(async move { serve().await });
    let relayer_job = tokio::spawn(async move { relayer.start().await });
    let (serve_res, relayer_res) = tokio::join!(serve_job, relayer_job);

    serve_res?;
    relayer_res?;

    Ok(())
}
