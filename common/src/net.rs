use libp2p::core::transport::ListenerId;
use libp2p::{swarm, Swarm, TransportError};
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("bad listen address {0:?}: {1}")]
    BadListenAddr(String, libp2p::multiaddr::Error),
    #[error("cannot listen on {0:?}: {1}")]
    CannotListenOn(String, TransportError<std::io::Error>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub listen_on: Vec<String>,
}

pub fn listen_on<TBehavior: swarm::NetworkBehaviour>(
    swarm: &mut Swarm<TBehavior>, addrs: &[impl AsRef<str>],
) -> Result<Vec<ListenerId>, Error> {
    use Error::*;
    addrs
        .iter()
        .map(|addr| {
            let addr = addr.as_ref();
            swarm
                .listen_on(addr.parse().map_err(|e| BadListenAddr(addr.into(), e))?)
                .map_err(|e| CannotListenOn(addr.into(), e))
        })
        .collect()
}
