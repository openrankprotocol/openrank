use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::server::Server;
use jsonrpsee::types::ErrorObjectOwned;
use serde::Serialize;
use std::net::SocketAddr;

pub trait Config: Clone {
    type Hash: Send + Clone + Serialize + Sync + 'static;
}

#[derive(Clone, Serialize)]
struct SequencerConfig;

impl Config for SequencerConfig {
    type Hash = [u8; 32];
}

#[rpc(server, namespace = "sequencer")]
pub trait Rpc<T: Config> {
    #[method(name = "trust_update")]
    fn trust_update(&self) -> Result<T::Hash, ErrorObjectOwned>;
}

pub struct SequencerServer;

#[async_trait]
impl RpcServer<SequencerConfig> for SequencerServer {
    fn trust_update(&self) -> Result<[u8; 32], ErrorObjectOwned> {
        Ok([0u8; 32])
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .expect("setting default subscriber failed");

    let server_addr = run_server().await;
    println!("server_addr: {}", server_addr);
}

async fn run_server() -> SocketAddr {
    let server = Server::builder().build("127.0.0.1:0").await.unwrap();

    let addr = server.local_addr().unwrap();
    let handle = server.start(SequencerServer.into_rpc());

    // In this example we don't care about doing shutdown so let's it run forever.
    // You may use the `ServerHandle` to shut it down or manage it yourself.
    tokio::spawn(handle.stopped());

    addr
}
