pub mod algos;
pub mod db;
pub mod merkle;
pub mod result;
pub mod topics;
pub mod tx_event;
pub mod txs;

use alloy_rlp::encode;
use k256::ecdsa::SigningKey;
use libp2p::{
    gossipsub::{self, MessageId, PublishError},
    mdns, noise,
    swarm::NetworkBehaviour,
    tcp, yamux, Swarm,
};
use merkle::hash_leaf;
use sha3::Keccak256;
use std::{error::Error, io, time::Duration};
use topics::Topic;
use tracing::info;
use tx_event::TxEvent;
use txs::{Address, Tx, TxKind};

#[derive(NetworkBehaviour)]
/// We create a custom network behaviour.
pub struct MyBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    // pub identify: identify::Behaviour,
}

/// Builds a libp2p swarm with the custom behaviour.
pub async fn build_node() -> Result<Swarm<MyBehaviour>, Box<dyn Error>> {
    let swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| {
            // Set a custom gossipsub configuration
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                // This is set to aid debugging by not cluttering the log space
                .heartbeat_interval(Duration::from_secs(15))
                // This sets the kind of message validation. The default is Strict (enforce message signing)
                .validation_mode(gossipsub::ValidationMode::Strict)
                .build()
                // Temporary hack because `build` does not return a proper `std::error::Error`.
                .map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?;

            // build a gossipsub network behaviour
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )?;

            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;

            // let identify = identify::Behaviour::new(identify::Config::new(
            // 	"openrank/1.0.0".to_string(),
            // 	key.public(),
            // ));

            Ok(MyBehaviour { gossipsub, mdns })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::MAX))
        .build();

    Ok(swarm)
}

/// Broadcasts a default transaction event to the given topic.
pub fn broadcast_default_event(
    swarm: &mut Swarm<MyBehaviour>, kind: TxKind, data: Vec<u8>, topic: Topic,
) -> Result<MessageId, PublishError> {
    info!("PUBLISH: {}", topic.clone());
    let tx = Tx::default_with(kind, data);
    let tx_event = TxEvent::default_with_data(encode(tx));
    let topic_wrapper = gossipsub::IdentTopic::new(topic);
    swarm.behaviour_mut().gossipsub.publish(topic_wrapper, encode(tx_event))
}

/// Broadcasts a transaction event to the given topic.
pub fn broadcast_event(
    swarm: &mut Swarm<MyBehaviour>, tx: Tx, topic: Topic,
) -> Result<MessageId, PublishError> {
    info!("PUBLISH: {}", topic.clone());
    let tx_event = TxEvent::default_with_data(encode(tx));
    let topic_wrapper = gossipsub::IdentTopic::new(topic);
    swarm.behaviour_mut().gossipsub.publish(topic_wrapper, encode(tx_event))
}

/// Generates an address from a signing key.
/// The address is the first 20 bytes of the keccak256 hash of the public key,
/// which is compatible with Ethereum addresses.
// TODO: Update to a new method that correctly matches the Ethereum address format
pub fn address_from_sk(sk: &SigningKey) -> Address {
    let vk = sk.verifying_key();
    let vk_bytes = vk.to_sec1_bytes();

    let hash = hash_leaf::<Keccak256>(vk_bytes.as_ref().to_vec());
    let mut address_bytes = [0u8; 20];
    address_bytes.copy_from_slice(&hash.0[..20]);

    Address(address_bytes)
}
