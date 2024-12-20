use alloy_rlp::Decodable;
use coordinator::JobCoordinator;
use dotenv::dotenv;
use futures::StreamExt;
use getset::Getters;
use k256::ecdsa;
use k256::ecdsa::SigningKey;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
    broadcast_event, build_node, config,
    db::{self, Db, DbItem},
    net,
    topics::{Domain, Topic},
    tx::{self, compute, consts, Address, Tx},
    tx_event::TxEvent,
    MyBehaviour, MyBehaviourEvent,
};
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};
use tokio::select;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod coordinator;

#[derive(Debug)]
/// Errors that can arise while using the block builder node.
pub enum Error {
    /// The decode error. This can arise when decoding a transaction.
    Decode(alloy_rlp::Error),
    /// The database error. The error can occur when interacting with the database.
    Db(db::Error),
    /// The p2p error. This can arise when sending or receiving messages over the p2p network.
    P2P(String),
    /// The signature error. This can arise when verifying a transaction signature.
    Signature(ecdsa::Error),
    /// The invalid tx kind error. This can arise when the transaction kind is not valid.
    InvalidTxKind,
}

impl StdError for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::Decode(err) => err.fmt(f),
            Self::Db(err) => err.fmt(f),
            Self::P2P(err) => write!(f, "p2p error: {}", err),
            Self::Signature(err) => err.fmt(f),
            Self::InvalidTxKind => write!(f, "InvalidTxKind"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// The whitelist for the Block Builder.
struct Whitelist {
    /// The list of addresses that are allowed to be computers.
    computer: Vec<Address>,
    /// The list of addresses that are allowed to be verifiers.
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
}

/// The Block Builder node. It contains the Swarm, the Config, the DB, the SecretKey, and the ComputeRunner.
#[derive(Getters)]
#[getset(get = "pub")]
pub struct Node {
    swarm: Swarm<MyBehaviour>,
    config: Config,
    db: Db,
    secret_key: SigningKey,
    coordinator: JobCoordinator,
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
        tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();

        let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
        let secret_key_bytes = hex::decode(secret_key_hex)?;
        let secret_key = SigningKey::from_slice(secret_key_bytes.as_slice())?;

        let config_loader = config::Loader::new("openrank-block-builder")?;
        let config: Config = config_loader.load_or_create(include_str!("../config.toml"))?;
        let db = Db::new(
            &config.database,
            &[&Tx::get_cf(), &compute::Result::get_cf(), &compute::ResultReference::get_cf()],
        )?;

        let swarm = build_node(net::load_keypair(config.p2p().keypair(), &config_loader)?).await?;
        info!("PEER_ID: {:?}", swarm.local_peer_id());

        let coordinator = JobCoordinator::new();

        Ok(Self { swarm, config, db, secret_key, coordinator })
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
                info!(
                    "TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
                    message.topic.as_str(),
                );
                match topic {
                    Topic::NamespaceTrustUpdate(namespace) => {
                        let tx_event =
                            TxEvent::decode(&mut message.data.as_slice()).map_err(Error::Decode)?;
                        let mut tx =
                            Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                        if let tx::Body::TrustUpdate(_) = tx.body() {
                            tx.verify_against(namespace.owner()).map_err(Error::Signature)?;
                            // Add Tx to db
                            tx.set_sequence_number(message.sequence_number.unwrap_or_default());
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                        } else {
                            return Err(Error::InvalidTxKind);
                        }
                    },
                    Topic::NamespaceSeedUpdate(namespace) => {
                        let tx_event =
                            TxEvent::decode(&mut message.data.as_slice()).map_err(Error::Decode)?;
                        let mut tx =
                            Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                        if let tx::Body::SeedUpdate(_) = tx.body() {
                            tx.verify_against(namespace.owner()).map_err(Error::Signature)?;
                            // Add Tx to db
                            tx.set_sequence_number(message.sequence_number.unwrap_or_default());
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                        } else {
                            return Err(Error::InvalidTxKind);
                        }
                    },
                    Topic::DomainRequest(domain_id) => {
                        let tx_event =
                            TxEvent::decode(&mut message.data.as_slice()).map_err(Error::Decode)?;
                        let tx =
                            Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                        if let tx::Body::ComputeRequest(compute_request) = tx.body() {
                            let address = tx.verify().map_err(Error::Signature)?;
                            assert!(self.config.whitelist.users.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            assert_eq!(compute_request.domain_id(), domain_id);

                            let assignment_topic = Topic::DomainAssignent(*domain_id);
                            let computer = self.config.whitelist.computer[0];
                            let verifier = self.config.whitelist.verifier[0];
                            let compute_assignment =
                                compute::Assignment::new(tx.hash(), computer, verifier);
                            let mut tx =
                                Tx::default_with(tx::Body::ComputeAssignment(compute_assignment));
                            tx.sign(&self.secret_key).map_err(Error::Signature)?;
                            broadcast_event(&mut self.swarm, tx.clone(), assignment_topic)
                                .map_err(|e| Error::P2P(e.to_string()))?;
                            // After broadcasting ComputeAssignment, save to db
                            self.db.put(tx).map_err(Error::Db)?;
                        } else {
                            return Err(Error::InvalidTxKind);
                        }
                    },
                    Topic::DomainCommitment(_) => {
                        let tx_event =
                            TxEvent::decode(&mut message.data.as_slice()).map_err(Error::Decode)?;
                        let tx =
                            Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                        if let tx::Body::ComputeCommitment(commitment) = tx.body() {
                            let address = tx.verify().map_err(Error::Signature)?;
                            assert!(self.config.whitelist.computer.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;

                            let assignment_tx_key = Tx::construct_full_key(
                                consts::COMPUTE_ASSIGNMENT,
                                commitment.assignment_tx_hash().clone(),
                            );
                            let assignment_tx: Tx =
                                self.db.get(assignment_tx_key).map_err(Error::Db)?;
                            let assignment_body = match assignment_tx.body() {
                                tx::Body::ComputeAssignment(assignment_body) => assignment_body,
                                _ => return Err(Error::InvalidTxKind),
                            };
                            let request_tx_key = Tx::construct_full_key(
                                consts::COMPUTE_REQUEST,
                                assignment_body.request_tx_hash().clone(),
                            );
                            let request: Tx = self.db.get(request_tx_key).map_err(Error::Db)?;
                            if let Err(db::Error::NotFound) =
                                self.db.get::<compute::ResultReference>(
                                    assignment_body.request_tx_hash().to_bytes(),
                                )
                            {
                                let mut result =
                                    compute::Result::new(tx.hash(), Vec::new(), request.hash());
                                self.coordinator.add_job_result(&mut result);
                                self.db.put(result.clone()).map_err(Error::Db)?;
                                let reference = compute::ResultReference::new(
                                    assignment_body.request_tx_hash().clone(),
                                    result.seq_number().unwrap(),
                                );
                                self.db.put(reference).map_err(Error::Db)?;
                            }
                        } else {
                            return Err(Error::InvalidTxKind);
                        }
                    },
                    Topic::DomainScores(_) => {
                        let tx_event =
                            TxEvent::decode(&mut message.data.as_slice()).map_err(Error::Decode)?;
                        let tx =
                            Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                        if let tx::Body::ComputeScores(_) = tx.body() {
                            let address = tx.verify().map_err(Error::Signature)?;
                            assert!(self.config.whitelist.computer.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                        } else {
                            return Err(Error::InvalidTxKind);
                        }
                    },
                    Topic::DomainVerification(_) => {
                        let tx_event =
                            TxEvent::decode(&mut message.data.as_slice()).map_err(Error::Decode)?;
                        let tx =
                            Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                        if let tx::Body::ComputeVerification(compute_verification) = tx.body() {
                            let address = tx.verify().map_err(Error::Signature)?;
                            assert!(self.config.whitelist.verifier.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;

                            let assignment_tx_key = Tx::construct_full_key(
                                consts::COMPUTE_ASSIGNMENT,
                                compute_verification.assignment_tx_hash().clone(),
                            );
                            let assignment_tx: Tx =
                                self.db.get(assignment_tx_key).map_err(Error::Db)?;
                            let assignment_body = match assignment_tx.body() {
                                tx::Body::ComputeAssignment(assignment_body) => assignment_body,
                                _ => return Err(Error::InvalidTxKind),
                            };
                            let result_reference: compute::ResultReference = self
                                .db
                                .get(assignment_body.request_tx_hash().to_bytes())
                                .map_err(Error::Db)?;
                            let compute_result_key =
                                compute::Result::construct_full_key(*result_reference.seq_number());
                            let mut result: compute::Result =
                                self.db.get(compute_result_key).map_err(Error::Db)?;
                            result.append_verification_tx_hash(tx.hash());
                            self.coordinator.add_job_result(&mut result);
                            self.db.put(result).map_err(Error::Db)?;
                        } else {
                            return Err(Error::InvalidTxKind);
                        }
                    },
                    _ => {},
                }
            }
        }

        Ok(())
    }

    /// Node recovery method. Used for loading results from the DB into the memory, for future indexing.
    pub fn node_recovery(&mut self) -> Result<(), Error> {
        let job_results: Vec<compute::Result> =
            self.db.get_range_from_start(consts::COMPUTE_RESULT, None, None).map_err(Error::Db)?;
        for mut job_result in job_results {
            self.coordinator.add_job_result(&mut job_result);
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

        let topics_trust_updates: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|domain| Topic::NamespaceTrustUpdate(domain.trust_namespace()))
            .collect();
        let topics_seed_updates: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|domain| Topic::NamespaceSeedUpdate(domain.seed_namespace()))
            .collect();
        let topics_requests: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(Topic::DomainRequest)
            .collect();
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

        for topic in topics_verification
            .iter()
            .chain(&topics_commitment)
            .chain(&topics_scores)
            .chain(&topics_requests)
            .chain(&topics_trust_updates)
            .chain(&topics_seed_updates)
        {
            // Create a Gossipsub topic
            let topic = gossipsub::IdentTopic::new(topic.clone());
            // subscribes to our topic
            self.swarm.behaviour_mut().gossipsub_subscribe(&topic)?;
        }

        // Kick it off
        loop {
            select! {
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
                    SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(event)) => {
                        let iter_chain = topics_requests.iter().chain(&topics_commitment).chain(&topics_scores).chain(&topics_verification).chain(&topics_trust_updates).chain(&topics_seed_updates);
                        let res = self.handle_gossipsub_events(event, iter_chain.collect());
                        if let Err(e) = res {
                            error!("{e:?}");
                        }
                    },
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("Local node is listening on {address}");
                    },
                    e => info!("NEW_EVENT: {:?}", e),
                }
            }
        }
    }
}
