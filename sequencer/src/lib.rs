use futures::StreamExt;
use jsonrpsee::{server::Server, RpcModule};
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
    build_node, config,
    db::{self, Db, DbItem},
    net,
    result::JobResult,
    topics::Topic,
    txs::{Address, Tx},
    MyBehaviour, MyBehaviourEvent,
};
use rpc::{RpcServer, SequencerServer};
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::{
    select,
    sync::mpsc::{self, Receiver},
};
use tracing::{error, info};

mod rpc;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The whitelist for the Sequencer.
pub struct Whitelist {
    /// The list of addresses that are allowed to call the Sequencer.
    pub users: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The configuration for the Sequencer.
pub struct Config {
    /// The whitelist for the Sequencer.
    pub whitelist: Whitelist,
    pub database: db::Config,
    pub p2p: net::Config,
}

/// The Sequencer node. It contains the Swarm, the Server, and the Receiver.
pub struct SequencerNode {
    config: Config,
    swarm: Swarm<MyBehaviour>,
    rpc: RpcModule<SequencerServer>,
    receiver: Receiver<(Vec<u8>, Topic)>,
}

impl SequencerNode {
    /// Initialize the node:
    /// - Load the config from config.toml
    /// - Initialize the Swarm
    /// - Initialize the DB
    /// - Initialize the Sequencer JsonRPC server
    pub async fn init() -> Result<Self, Box<dyn Error>> {
        let config_loader = config::Loader::new("openrank-sequencer")?;
        let config: Config = config_loader.load_or_create(include_str!("../config.toml"))?;
        let db = Db::new_secondary(&config.database, &[&Tx::get_cf(), &JobResult::get_cf()])?;
        let (sender, receiver) = mpsc::channel(100);
        let seq_server = SequencerServer::new(sender, config.whitelist.users.clone(), db);
        let rpc = seq_server.into_rpc();

        let swarm = build_node(net::load_keypair(&config.p2p.keypair, &config_loader)?).await?;
        info!("PEER_ID: {:?}", swarm.local_peer_id());

        Ok(Self { swarm, config, rpc, receiver })
    }

    /// Run the node:
    /// - Listen on all interfaces and whatever port the OS assigns
    /// - Subscribe to all the topics
    /// - Handle gossipsub events
    /// - Handle mDNS events
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        net::listen_on(&mut self.swarm, &self.config.p2p.listen_on)?;
        let server = Server::builder().build("127.0.0.1:60000").await?;
        server.start(self.rpc.clone());

        // Kick it off
        loop {
            select! {
                sibling = self.receiver.recv() => {
                    if let Some((data, topic)) = sibling {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        info!("PUBLISH: {:?}", topic.clone());
                        if let Err(e) =
                           self.swarm.behaviour_mut().gossipsub.publish(topic_wrapper, data)
                        {
                           error!("Publish error: {e:?}");
                        }
                    }
                }
                event = self.swarm.select_next_some() => match event {
                    SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                        for (peer_id, _multiaddr) in list {
                            info!("mDNS discovered a new peer: {peer_id}");
                            self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                        }
                    },
                    SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                        for (peer_id, _multiaddr) in list {
                            info!("mDNS discover peer has expired: {peer_id}");
                            self.swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                        }
                    },
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("Local node is listening on {address}");
                    },
                    e => info!("{:?}", e),
                }
            }
        }
    }
}
