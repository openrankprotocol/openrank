use getset::Getters;
use libp2p::core::transport::ListenerId;
use libp2p::identity::{DecodingError, Keypair};
use libp2p::{swarm, Swarm, TransportError};
use serde::{Deserialize, Serialize};
use std::io;
use std::net::SocketAddr;
use tracing::info;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("bad listen address {0:?}: {1}")]
    BadListenAddr(String, libp2p::multiaddr::Error),
    #[error("cannot listen on {0:?}: {1}")]
    CannotListenOn(String, TransportError<io::Error>),
    #[error("cannot read key: {0:?}")]
    CannotReadKey(io::Error),
    #[error("cannot write key: {0:?}")]
    CannotWriteKey(io::Error),
    #[error("cannot decode key: {0:?}")]
    CannotDecodeKey(DecodingError),
    #[error("cannot encode key: {0:?}")]
    CannotEncodeKey(DecodingError),
}

/// Rpc configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct RpcConfig {
    address: SocketAddr,
}

/// Network configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct Config {
    listen_on: Vec<String>,
    keypair: Option<KeypairConfig>,
}

/// P2P keypair configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct KeypairConfig {
    /// Filename of the keypair.  Either absolute or relative to the config directory.
    /// The file contains binary protobuf representation of the keypair.
    file: Option<String>,
}

/// Default P2P keypair filename.
pub const DEFAULT_KEYPAIR_FILENAME: &str = "p2p-keypair.binpb";

/// Makes `swarm` listen on all `multiaddrs` and returns listener identifiers.
pub fn listen_on<TBehavior: swarm::NetworkBehaviour>(
    swarm: &mut Swarm<TBehavior>, multiaddrs: &[impl AsRef<str>],
) -> Result<Vec<ListenerId>, Error> {
    use Error::*;
    multiaddrs
        .iter()
        .map(|addr| {
            let addr = addr.as_ref();
            swarm.listen_on(addr.parse().map_err(|e| BadListenAddr(addr.into(), e))?).map_err(|e| {
                println!("{}", e);
                CannotListenOn(addr.into(), e)
            })
        })
        .collect()
}

pub fn load_keypair(
    config: &Option<KeypairConfig>, loader: &crate::config::Loader,
) -> Result<Keypair, Error> {
    use Error::*;
    let filename = config.as_ref().and_then(|c| c.file.as_ref());
    let path = loader.pathname(filename.map_or(DEFAULT_KEYPAIR_FILENAME, |s| s.as_str()));
    Ok(match std::fs::read(&path) {
        Ok(b) => Keypair::from_protobuf_encoding(&b).map_err(CannotDecodeKey)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            info!("GENERATING_P2P_KEYPAIR");
            let keypair = Keypair::generate_ed25519();
            let b = keypair.to_protobuf_encoding().map_err(CannotEncodeKey)?;
            std::fs::write(&path, b).map_err(CannotWriteKey)?;
            keypair
        },
        Err(e) => return Err(CannotReadKey(e)),
    })
}
