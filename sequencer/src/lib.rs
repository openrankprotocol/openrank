use alloy_rlp::{encode, Decodable};
use futures::StreamExt;
use getset::Getters;
use jsonrpsee::{server::Server, RpcModule};
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
    build_node, config,
    db::{self, Db, DbItem, CHECKPOINTS_CF},
    net,
    topics::Topic,
    tx::{compute, Address, Tx, TxSequence, TxTimestamp},
    tx_event::TxEvent,
    MyBehaviour, MyBehaviourEvent,
};
use rpc::{RpcServer, SequencerServer};
use seq::Sequencer;
use serde::{Deserialize, Serialize};
use std::{error::Error, sync::Arc, time::Duration};
use tokio::{
    select,
    sync::mpsc::{self, Receiver},
};
use tracing::{error, info};

mod rpc;
mod seq;

const DB_REFRESH_INTERVAL: u64 = 10; // seconds

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// The whitelist for the Sequencer.
struct Whitelist {
    /// The list of addresses that are allowed to call the Sequencer.
    users: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// The configuration for the Sequencer.
struct Config {
    /// The whitelist for the Sequencer.
    whitelist: Whitelist,
    database: db::Config,
    p2p: net::Config,
    rpc: net::RpcConfig,
}

#[derive(Getters)]
#[getset(get = "pub")]
/// The Sequencer node. It contains the Swarm, the Server, and the Receiver.
pub struct Node {
    config: Config,
    swarm: Swarm<MyBehaviour>,
    rpc: RpcModule<SequencerServer>,
    secondary_db: Arc<Db>,
    receiver: Receiver<(Vec<u8>, Topic)>,
    sequencer: Sequencer,
}

impl Node {
    /// Initialize the node:
    /// - Load the config from config.toml
    /// - Initialize the Swarm
    /// - Initialize the DB
    /// - Initialize the Sequencer JsonRPC server
    pub async fn init() -> Result<Self, Box<dyn Error>> {
        let config_loader = config::Loader::new("openrank-sequencer")?;
        let config: Config = config_loader.load_or_create(include_str!("../config.toml"))?;
        let primary_db = Db::new(
            &config.database,
            &[&TxSequence::get_cf(), &TxTimestamp::get_cf(), CHECKPOINTS_CF],
        )?;
        let secondary_db = Db::new_secondary(
            &config.database,
            &[&Tx::get_cf(), &compute::Result::get_cf(), &compute::ResultReference::get_cf()],
        )?;
        let (sender, receiver) = mpsc::channel(100);
        let secondary_db = Arc::new(secondary_db);
        let seq_server =
            SequencerServer::new(sender, config.whitelist.users.clone(), secondary_db.clone());
        let rpc = seq_server.into_rpc();

        let swarm = build_node(net::load_keypair(config.p2p().keypair(), &config_loader)?).await?;
        info!("PEER_ID: {:?}", swarm.local_peer_id());

        let sequencer = Sequencer::new(primary_db)?;

        Ok(Self { swarm, config, rpc, secondary_db, receiver, sequencer })
    }

    /// Run the node:
    /// - Listen on all interfaces and whatever port the OS assigns
    /// - Subscribe to all the topics
    /// - Handle gossipsub events
    /// - Handle mDNS events
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        net::listen_on(&mut self.swarm, self.config.p2p().listen_on())?;
        let server = Server::builder().build(self.config.rpc().address()).await?;
        let handle = server.start(self.rpc.clone());
        tokio::spawn(handle.stopped());

        // spawn a task to refresh the DB every 10 seconds
        let db_handler = self.secondary_db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(DB_REFRESH_INTERVAL));
            loop {
                interval.tick().await;
                match db_handler.refresh() {
                    Ok(_) => {
                        info!("DB refreshed");
                    },
                    Err(e) => {
                        error!("DB refresh error: {e:?}");
                    },
                }
            }
        });

        // Kick it off
        loop {
            select! {
                sibling = self.receiver.recv() => {
                    if let Some((data, topic)) = sibling {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        let tx_event_res = TxEvent::decode(&mut data.as_ref());
                        let tx_event = match tx_event_res {
                            Ok(tx_event) => tx_event,
                            Err(e) => {
                                error!("TxEvent decoding error: {e:?}");
                                continue;
                            }
                        };
                        let tx_res = Tx::decode(&mut tx_event.data().as_ref());
                        let tx = match tx_res {
                            Ok(tx) => tx,
                            Err(e) => {
                                error!("Tx decoding error: {e:?}");
                                continue;
                            }
                        };
                        let seq_tx_res = self.sequencer.sequence_tx(tx);
                        let seq_tx = match seq_tx_res {
                            Ok(seq_tx) => seq_tx,
                            Err(e) => {
                                error!("Tx sequencing error: {e:?}");
                                continue;
                            }
                        };
                        let new_tx_event = TxEvent::default_with_data(encode(seq_tx));
                        info!("PUBLISH: {:?}", topic.clone());
                        if let Err(e) =
                           self.swarm.behaviour_mut().gossipsub_publish(topic_wrapper, encode(new_tx_event))
                        {
                           error!("Publish error: {e:?}");
                        }
                    }
                }
                event = self.swarm.select_next_some() => match event {
                    SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                        for (peer_id, _multiaddr) in list {
                            info!("mDNS discovered a new peer: {peer_id}");
                            self.swarm.behaviour_mut().gossipsub_add_peer(&peer_id);
                        }
                    },
                    SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                        for (peer_id, _multiaddr) in list {
                            info!("mDNS discover peer has expired: {peer_id}");
                            self.swarm.behaviour_mut().gossipsub_remove_peer(&peer_id);
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
