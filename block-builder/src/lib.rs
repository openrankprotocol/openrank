use alloy_rlp::Decodable;
use dotenv::dotenv;
use futures::StreamExt;
use getset::Getters;
use jsonrpsee::{server::Server, RpcModule};
use k256::ecdsa;
use k256::ecdsa::SigningKey;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
    broadcast_event, build_node, config,
    db::{self, Db, DbItem},
    logs::setup_tracing,
    net,
    topics::{Domain, Topic},
    tx::{
        self,
        compute::{self, RequestSequence, ResultReference},
        consts, Address, Tx, TxSequence,
    },
    tx_event::TxEvent,
    MyBehaviour, MyBehaviourEvent,
};
use sequencer::{
    rpc::{RpcServer, SequencerServer},
    seq::Sequencer,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::{error::Error as StdError, sync::Arc};
use tokio::{
    select,
    sync::mpsc::{self, Receiver},
};
use tracing::{debug, error, info};

mod sequencer;

const DB_REFRESH_INTERVAL: u64 = 10; // seconds

#[derive(thiserror::Error, Debug)]
/// Errors that can arise while using the block builder node.
pub enum Error {
    /// The decode error. This can arise when decoding a transaction.
    #[error("Decode Error: {0}")]
    Decode(alloy_rlp::Error),
    /// The database error. The error can occur when interacting with the database.
    #[error("Db Error: {0}")]
    Db(db::Error),
    /// The p2p error. This can arise when sending or receiving messages over the p2p network.
    #[error("P2P error: {0}")]
    P2P(String),
    /// The signature error. This can arise when verifying a transaction signature.
    #[error("Signature error: {0}")]
    Signature(ecdsa::Error),
    /// The invalid tx kind error. This can arise when the transaction kind is not valid.
    #[error("Invalid TX kind")]
    InvalidTxKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// The whitelist for the Block Builder.
struct Whitelist {
    /// The list of addresses that are allowed to be computers.
    #[serde(alias = "computers")]
    computer: Vec<Address>,
    /// The list of addresses that are allowed to be verifiers.
    #[serde(alias = "verifiers")]
    verifier: Vec<Address>,
    /// The list of addresses that are allowed to broadcast transactions.
    users: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// The configuration for the Block Builder.
struct Config {
    /// The list of domains to process ComputeRequest TXs for.
    domains: Vec<Domain>,
    /// The whitelist for the Block Builder.
    whitelist: Whitelist,
    database: db::Config,
    p2p: net::Config,
    rpc: net::RpcConfig,
}

/// The Block Builder node. It contains the Swarm, the Config, the DB, the SecretKey, and the ComputeRunner.
#[derive(Getters)]
#[getset(get = "pub")]
pub struct Node {
    swarm: Swarm<MyBehaviour>,
    config: Config,
    primary_db: Arc<Db>,
    secondary_db: Arc<Db>,
    secret_key: SigningKey,
    receiver: Receiver<(Vec<u8>, Topic)>,
    sequencer: Sequencer,
    rpc: RpcModule<SequencerServer>,
}

impl Node {
    /// Initializes the node:
    /// - Loads the config from config.toml.
    /// - Initializes the Swarm.
    /// - Initializes the DB.
    /// - Initializes the ComputeRunner.
    /// - Initializes the Secret Key.
    pub async fn init() -> Result<Self, Box<dyn StdError>> {
        dotenv().ok();
        setup_tracing();

        let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
        let secret_key_bytes = hex::decode(secret_key_hex)?;
        let secret_key = SigningKey::from_slice(secret_key_bytes.as_slice())?;

        let config_loader = config::Loader::new("openrank-block-builder")?;
        let config: Config = config_loader.load_or_create(include_str!("../config.toml"))?;

        let cfs = vec![
            Tx::get_cf(),
            ResultReference::get_cf(),
            RequestSequence::get_cf(),
            TxSequence::get_cf(),
        ];
        let primary_db = Db::new(&config.database, &cfs)?;
        let primary_db = Arc::new(primary_db);
        let secondary_db = Db::new_secondary(&config.database, cfs)?;
        let secondary_db = Arc::new(secondary_db);

        let swarm = build_node(net::load_keypair(config.p2p().keypair(), &config_loader)?).await?;
        info!("PEER_ID: {:?}", swarm.local_peer_id());

        let (sender, receiver) = mpsc::channel(100);
        let seq_server =
            SequencerServer::new(sender, config.whitelist.users.clone(), secondary_db.clone());
        let rpc = seq_server.into_rpc();

        let sequencer = Sequencer::new(primary_db.clone())?;

        Ok(Self { swarm, config, primary_db, secondary_db, secret_key, rpc, receiver, sequencer })
    }

    fn handle_gossipsub_data(&mut self, data: Vec<u8>, topic: &Topic) -> Result<(), Error> {
        match topic {
            Topic::NamespaceTrustUpdate(namespace) => {
                let tx_event = TxEvent::decode(&mut data.as_slice()).map_err(Error::Decode)?;
                let tx = Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                if let tx::Body::TrustUpdate(_) = tx.body() {
                    info!("NAMESPACE_TRUST_UPDATE: {}", namespace);

                    tx.verify_against(namespace.owner()).map_err(Error::Signature)?;
                    self.primary_db.put(tx.clone()).map_err(Error::Db)?;
                    broadcast_event(&mut self.swarm, tx.clone(), topic.clone())
                        .map_err(|e| Error::P2P(e.to_string()))?;
                } else {
                    return Err(Error::InvalidTxKind);
                }
            },
            Topic::NamespaceSeedUpdate(namespace) => {
                let tx_event = TxEvent::decode(&mut data.as_slice()).map_err(Error::Decode)?;
                let tx = Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                if let tx::Body::SeedUpdate(_) = tx.body() {
                    info!("NAMESPACE_SEED_UPDATE: {}", namespace);

                    tx.verify_against(namespace.owner()).map_err(Error::Signature)?;
                    self.primary_db.put(tx.clone()).map_err(Error::Db)?;
                    broadcast_event(&mut self.swarm, tx.clone(), topic.clone())
                        .map_err(|e| Error::P2P(e.to_string()))?;
                } else {
                    return Err(Error::InvalidTxKind);
                }
            },
            Topic::DomainRequest(domain_id) => {
                let tx_event = TxEvent::decode(&mut data.as_slice()).map_err(Error::Decode)?;
                let tx = Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                if let tx::Body::ComputeRequest(compute_request) = tx.body() {
                    info!("DOMAIN_REQUEST_EVENT: {}", domain_id);

                    let address = tx.verify().map_err(Error::Signature)?;
                    assert!(self.config.whitelist.users.contains(&address));
                    // Add Tx to db
                    self.primary_db.put(tx.clone()).map_err(Error::Db)?;
                    assert_eq!(compute_request.domain_id(), domain_id);

                    let assignment_topic = Topic::DomainAssignent(*domain_id);
                    let computer = self.config.whitelist.computer[0];
                    let verifiers = self.config.whitelist.verifier.clone();
                    let compute_assignment =
                        compute::Assignment::new(tx.hash(), computer, verifiers);
                    let mut tx = Tx::default_with(tx::Body::ComputeAssignment(compute_assignment));
                    tx.sign(&self.secret_key).map_err(Error::Signature)?;
                    broadcast_event(&mut self.swarm, tx.clone(), assignment_topic)
                        .map_err(|e| Error::P2P(e.to_string()))?;
                    // After broadcasting ComputeAssignment, save to db
                    self.primary_db.put(tx).map_err(Error::Db)?;
                } else {
                    return Err(Error::InvalidTxKind);
                }
            },
            Topic::DomainCommitment(domain_id) => {
                let tx_event = TxEvent::decode(&mut data.as_slice()).map_err(Error::Decode)?;
                let tx = Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                if let tx::Body::ComputeCommitment(commitment) = tx.body() {
                    info!("DOMAIN_COMMITMENT_EVENT: {}", domain_id);

                    let address = tx.verify().map_err(Error::Signature)?;
                    assert!(self.config.whitelist.computer.contains(&address));
                    // Add Tx to db
                    self.primary_db.put(tx.clone()).map_err(Error::Db)?;

                    let assignment_tx_key = Tx::construct_full_key(
                        consts::COMPUTE_ASSIGNMENT,
                        commitment.assignment_tx_hash().clone(),
                    );
                    let assignment_tx: Tx =
                        self.primary_db.get(assignment_tx_key).map_err(Error::Db)?;
                    let assignment_body = match assignment_tx.body() {
                        tx::Body::ComputeAssignment(assignment_body) => assignment_body,
                        _ => return Err(Error::InvalidTxKind),
                    };
                    let request_tx_key = Tx::construct_full_key(
                        consts::COMPUTE_REQUEST,
                        assignment_body.request_tx_hash().clone(),
                    );
                    let request: Tx = self.primary_db.get(request_tx_key).map_err(Error::Db)?;
                    if let Err(db::Error::NotFound) =
                        self.primary_db.get::<compute::ResultReference>(
                            assignment_body.request_tx_hash().to_bytes(),
                        )
                    {
                        let result = compute::Result::new(tx.hash(), Vec::new(), request.hash());
                        let tx = Tx::default_with(tx::Body::ComputeResult(result));
                        self.primary_db.put(tx.clone()).map_err(Error::Db)?;
                        let reference = compute::ResultReference::new(
                            assignment_body.request_tx_hash().clone(),
                            tx.hash(),
                        );
                        self.primary_db.put(tx).map_err(Error::Db)?;
                        self.primary_db.put(reference).map_err(Error::Db)?;
                    }
                } else {
                    return Err(Error::InvalidTxKind);
                }
            },
            Topic::DomainScores(domain_id) => {
                let tx_event = TxEvent::decode(&mut data.as_slice()).map_err(Error::Decode)?;
                let tx = Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                if let tx::Body::ComputeScores(_) = tx.body() {
                    info!("DOMAIN_SCORES_EVENT: {}", domain_id);

                    let address = tx.verify().map_err(Error::Signature)?;
                    assert!(self.config.whitelist.computer.contains(&address));
                    // Add Tx to db
                    self.primary_db.put(tx.clone()).map_err(Error::Db)?;
                } else {
                    return Err(Error::InvalidTxKind);
                }
            },
            Topic::DomainVerification(domain_id) => {
                let tx_event = TxEvent::decode(&mut data.as_slice()).map_err(Error::Decode)?;
                let tx = Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                if let tx::Body::ComputeVerification(compute_verification) = tx.body() {
                    info!("DOMAIN_VERIFICATION_EVENT: {}", domain_id);

                    let address = tx.verify().map_err(Error::Signature)?;
                    assert!(self.config.whitelist.verifier.contains(&address));
                    // Add Tx to db
                    self.primary_db.put(tx.clone()).map_err(Error::Db)?;

                    let assignment_tx_key = Tx::construct_full_key(
                        consts::COMPUTE_ASSIGNMENT,
                        compute_verification.assignment_tx_hash().clone(),
                    );
                    let assignment_tx: Tx =
                        self.primary_db.get(assignment_tx_key).map_err(Error::Db)?;
                    let assignment_body = match assignment_tx.body() {
                        tx::Body::ComputeAssignment(assignment_body) => assignment_body,
                        _ => return Err(Error::InvalidTxKind),
                    };
                    let mut result_reference: compute::ResultReference = self
                        .primary_db
                        .get(assignment_body.request_tx_hash().to_bytes())
                        .map_err(Error::Db)?;
                    let compute_result_key = Tx::construct_full_key(
                        consts::COMPUTE_RESULT,
                        result_reference.compute_result_tx_hash().clone(),
                    );
                    let result_tx: Tx =
                        self.primary_db.get(compute_result_key).map_err(Error::Db)?;
                    let mut result = match result_tx.body() {
                        tx::Body::ComputeResult(result_body) => result_body.clone(),
                        _ => return Err(Error::InvalidTxKind),
                    };
                    result.append_verification_tx_hash(tx.hash());
                    let result_tx = Tx::default_with(tx::Body::ComputeResult(result));
                    // Tx Hash is now updated
                    result_reference.set_compute_result_tx_hash(result_tx.hash());
                    self.primary_db.put(result_tx).map_err(Error::Db)?;
                    self.primary_db.put(result_reference).map_err(Error::Db)?;
                } else {
                    return Err(Error::InvalidTxKind);
                }
            },
            _ => {},
        }

        Ok(())
    }

    /// Handles incoming gossipsub `event` given the `topics` this node is interested in.
    /// Handling includes TX validation, storage in local db, or optionally triggering a broadcast
    /// of postceding TX to the network.
    fn handle_gossipsub_events(
        &mut self, event: gossipsub::Event, topics: Vec<&Topic>,
    ) -> Result<(), Error> {
        if let gossipsub::Event::Message { message_id, message, propagation_source } = event {
            for topic in topics {
                let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                if message.topic != topic_wrapper.hash() {
                    continue;
                }
                debug!(
                    "TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
                    message.topic.as_str(),
                );
                self.handle_gossipsub_data(message.data.clone(), topic)?;
            }
        }

        Ok(())
    }

    /// Runs the node:
    /// - Listens on all interfaces, on OS-assigned ephemeral ports.
    /// - Subscribes to all the topics.
    /// - Handles gossipsub events.
    /// - Handles mDNS events.
    pub async fn run(&mut self) -> Result<(), Box<dyn StdError>> {
        net::listen_on(&mut self.swarm, self.config.p2p().listen_on())?;
        let server = Server::builder().build(self.config.rpc().address()).await?;
        let handle = server.start(self.rpc.clone());
        tokio::spawn(handle.stopped());

        let topics_commitment: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(Topic::DomainCommitment)
            .collect();
        let topics_scores: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(Topic::DomainScores)
            .collect();
        let topics_verification: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(Topic::DomainVerification)
            .collect();

        for topic in topics_verification.iter().chain(&topics_commitment).chain(&topics_scores) {
            // Create a Gossipsub topic
            let topic = gossipsub::IdentTopic::new(topic.clone());
            // subscribes to our topic
            self.swarm.behaviour_mut().gossipsub_subscribe(&topic)?;
        }

        // spawn a task to refresh the DB every 10 seconds
        let db_handler = self.secondary_db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(DB_REFRESH_INTERVAL));
            loop {
                interval.tick().await;
                match db_handler.refresh() {
                    Ok(_) => {
                        info!("DB_REFRESH");
                    },
                    Err(e) => {
                        error!("DB_REFRESH_ERROR: {e:?}");
                    },
                }
            }
        });

        // Kick it off
        loop {
            select! {
                sibling = self.receiver.recv() => {
                    if let Some((data, topic)) = sibling {
                        let res = self.handle_gossipsub_data(data.clone(), &topic);
                        if let Err(e) = res {
                            error!("{e:?}");
                        }

                        let tx_event_res = TxEvent::decode(&mut data.as_ref());
                        let tx_event = match tx_event_res {
                            Ok(tx_event) => tx_event,
                            Err(e) => {
                                error!("TX_EVENT_DECODING_ERROR: {e:?}");
                                continue;
                            }
                        };
                        let tx_res = Tx::decode(&mut tx_event.data().as_ref());
                        let tx = match tx_res {
                            Ok(tx) => tx,
                            Err(e) => {
                                error!("TX_EVENT_DECODING_ERROR: {e:?}");
                                continue;
                            }
                        };
                        let seq_tx_res = self.sequencer.sequence_tx(tx);
                        let seq_tx = match seq_tx_res {
                            Ok(seq_tx) => seq_tx,
                            Err(e) => {
                                error!("TX_SEQUENCING_ERROR: {e:?}");
                                continue;
                            }
                        };
                        info!("TX_SEQUENCED: {}", seq_tx.hash());
                    }
                }
                event = self.swarm.select_next_some() => match event {
                    SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                        for (peer_id, _multiaddr) in list {
                            info!("mDNS_PEER_DISCOVERY: {peer_id}");
                            self.swarm.behaviour_mut().gossipsub_add_peer(&peer_id);
                        }
                    },
                    SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                        for (peer_id, _multiaddr) in list {
                            info!("mDNS_PEER_EXPIRE: {peer_id}");
                            self.swarm.behaviour_mut().gossipsub_remove_peer(&peer_id);
                        }
                    },
                    SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(event)) => {
                        let iter_chain = topics_commitment.iter()
                            .chain(&topics_scores)
                            .chain(&topics_verification);
                        let res = self.handle_gossipsub_events(event, iter_chain.collect());
                        if let Err(e) = res {
                            error!("{e:?}");
                        }
                    },
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("LISTEN_ON {address}");
                    },
                    e => debug!("{:?}", e),
                }
            }
        }
    }
}
