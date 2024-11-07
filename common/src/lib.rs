pub mod algos;
pub mod config;
pub mod merkle;
pub mod net;
pub mod result;
pub mod topics;
pub mod tx;
pub mod tx_event;

#[cfg(feature = "db")]
pub mod db;

use alloy_rlp::encode;
use getset::Getters;
use k256::ecdsa::SigningKey;
use libp2p::{
    gossipsub::{self, MessageId, PublishError, SubscriptionError},
    identity, mdns, noise,
    swarm::NetworkBehaviour,
    tcp, yamux, Swarm,
};
use merkle::hash_leaf;
use sha3::Keccak256;
use std::{error::Error, io, time::Duration};
use topics::Topic;
use tracing::info;
use tx::{Address, Tx};
use tx_event::TxEvent;

#[derive(NetworkBehaviour, Getters)]
#[getset(get = "pub")]
/// A custom libp2p [network behavior](libp2p::swarm::NetworkBehaviour) used by OpenRank nodes.
pub struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
}

impl MyBehaviour {
    pub fn gossipsub_subscribe(
        &mut self, topic: &gossipsub::IdentTopic,
    ) -> Result<bool, SubscriptionError> {
        self.gossipsub.subscribe(topic)
    }

    pub fn gossipsub_publish(
        &mut self, topic: gossipsub::IdentTopic, data: Vec<u8>,
    ) -> Result<MessageId, PublishError> {
        self.gossipsub.publish(topic, data)
    }

    pub fn gossipsub_add_peer(&mut self, peer_id: &libp2p::PeerId) {
        self.gossipsub.add_explicit_peer(peer_id);
    }

    pub fn gossipsub_remove_peer(&mut self, peer_id: &libp2p::PeerId) {
        self.gossipsub.remove_explicit_peer(peer_id);
    }
}

/// Builds a libp2p swarm with the custom behaviour.
pub async fn build_node(keypair: identity::Keypair) -> Result<Swarm<MyBehaviour>, Box<dyn Error>> {
    let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
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
    swarm: &mut Swarm<MyBehaviour>, body: tx::Body, topic: Topic,
) -> Result<MessageId, PublishError> {
    info!("PUBLISH: {}", topic.clone());
    let tx = Tx::default_with(body);
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
pub fn address_from_sk(sk: &SigningKey) -> Address {
    // TODO: Update to a new method that correctly matches the Ethereum address format
    let vk = sk.verifying_key();
    let uncompressed_point = vk.to_encoded_point(false);
    let vk_bytes = uncompressed_point.as_bytes();

    let hash = hash_leaf::<Keccak256>(vk_bytes[1..].to_vec());
    let mut address_bytes = [0u8; 20];
    address_bytes.copy_from_slice(&hash.inner()[12..]);

    Address::from_slice(&address_bytes)
}

#[cfg(test)]
mod test {
    use crate::*;

    #[test]
    fn test_address_from_sk() {
        // reference:
        //  https://github.com/ethereum/tests/blob/develop/BasicTests/keyaddrtest.json
        //  https://github.com/ethereum/execution-spec-tests/blob/main/src/ethereum_test_base_types/constants.py
        let test_vectors: Vec<(&str, &str)> = vec![
            (
                "c85ef7d79691fe79573b1a7064c19c1a9819ebdbd1faaab1a8ec92344438aaf4",
                "cd2a3d9f938e13cd947ec05abc7fe734df8dd826",
            ),
            (
                "c87f65ff3f271bf5dc8643484f66b200109caffe4bf98c4cb393dc35740b28c0",
                "13978aee95f38490e9769c39b2773ed763d9cd5f",
            ),
            (
                "45A915E4D060149EB4365960E6A7A45F334393093061116B197E3240065FF2D8",
                "a94f5374fce5edbc8e2a8697c15331677e6ebf0b",
            ),
            (
                "9E7645D0CFD9C3A04EB7A9DB59A4EB7D359F2E75C9164A9D6B9A7D54E1B6A36F",
                "8a0a19589531694250d570040a0c4b74576919b8",
            ),
        ];

        for (key_bytes_hex, expected_addr_hex) in test_vectors {
            let sk_bytes = hex::decode(key_bytes_hex).unwrap();
            let sk = SigningKey::from_slice(&sk_bytes).unwrap();
            let address = address_from_sk(&sk);
            assert_eq!(address.0.to_vec(), hex::decode(expected_addr_hex).unwrap());
        }
    }
}
