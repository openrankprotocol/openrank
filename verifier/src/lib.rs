use alloy_rlp::Decodable;
use dotenv::dotenv;
use futures::StreamExt;
use getset::Getters;
use jsonrpsee::{server::Server, RpcModule};
use k256::ecdsa;
use k256::ecdsa::SigningKey;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
    address_from_sk, broadcast_event, build_node, config,
    db::{self, Db, DbItem},
    net,
    topics::{Domain, Topic},
    tx::{self, compute, consts, Address, Tx},
    tx_event::TxEvent,
    MyBehaviour, MyBehaviourEvent,
};
use rpc::{RpcServer, VerifierServer};
use serde::{Deserialize, Serialize};
use std::{error::Error as StdError, sync::{Arc, Mutex}};
use std::fmt::{Display, Formatter, Result as FmtResult};
use tokio::select;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use openrank_common::runners::verification_runner::{self as runner, VerificationRunner};

mod rpc;

#[derive(Debug)]
/// Errors that can arise while using the verifier node.
pub enum Error {
    /// The decode error. This can arise when decoding a transaction.
    Decode(alloy_rlp::Error),
    /// The database error. The database error can occur when interacting with the database.
    Db(db::Error),
    /// The domain not found error. This can arise when the domain is not found in the config.
    DomainNotFound(String),
    /// The p2p error. This can arise when sending or receiving messages over the p2p network.
    P2P(String),
    /// The compute internal error. This can arise when there is an internal error in the verification runner.
    Runner(runner::Error),
    /// The signature error. This can arise when verifying a transaction signature.
    Signature(ecdsa::Error),
    /// The invalid tx kind error.
    InvalidTxKind,
    /// Cannot acquire lock of VerificationRunner
    VerificationRunnerLockError(String),
}

impl StdError for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::Decode(err) => err.fmt(f),
            Self::Db(err) => err.fmt(f),
            Self::DomainNotFound(domain) => write!(f, "Domain not found: {}", domain),
            Self::P2P(err) => write!(f, "p2p error: {}", err),
            Self::Runner(err) => write!(f, "internal error: {}", err),
            Self::Signature(err) => err.fmt(f),
            Self::InvalidTxKind => write!(f, "InvalidTxKind"),
            Self::VerificationRunnerLockError(err) => write!(f, "VerificationRunnerLockError: {}", err),
        }
    }
}

impl From<runner::Error> for Error {
    fn from(val: runner::Error) -> Self {
        Error::Runner(val)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// The whitelist for the Verifier.
struct Whitelist {
    /// The list of addresses that are allowed to be block builders.
    block_builder: Vec<Address>,
    /// The list of addresses that are allowed to be computers.
    computer: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// The configuration for the Verifier.
struct Config {
    /// The list of domains to perform verification for.
    domains: Vec<Domain>,
    /// The whitelist for the Verifier.
    whitelist: Whitelist,
    database: db::Config,
    p2p: net::Config,
    rpc: net::RpcConfig,
}

#[derive(Getters)]
#[getset(get = "pub")]
/// The Verifier node. It contains the Swarm, the Config, the DB, the VerificationRunner, and the SecretKey.
pub struct Node {
    swarm: Swarm<MyBehaviour>,
    config: Config,
    db: Db,
    verification_runner: Arc<Mutex<VerificationRunner>>,
    secret_key: SigningKey,
    rpc: RpcModule<VerifierServer>,
}

impl Node {
    /// Initializes the node:
    /// - Loads the config from config.toml.
    /// - Initializes the Swarm.
    /// - Initializes the DB.
    /// - Initializes the VerificationRunner.
    pub async fn init() -> Result<Self, Box<dyn std::error::Error>> {
        dotenv().ok();
        tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();

        let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
        let secret_key_bytes = hex::decode(secret_key_hex)?;
        let secret_key = SigningKey::from_slice(secret_key_bytes.as_slice())?;

        let config_loader = config::Loader::new("openrank-verifier")?;
        let config: Config = config_loader.load_or_create(include_str!("../config.toml"))?;
        let db = Db::new(&config.database, &[&Tx::get_cf()])?;
        
        let verification_runner = VerificationRunner::new(&config.domains);

        let verification_runner_arc_mutex = Arc::new(Mutex::new(verification_runner));
        let verifier_server = VerifierServer::new(verification_runner_arc_mutex.clone());
        let rpc = verifier_server.into_rpc();

        let swarm = build_node(net::load_keypair(config.p2p().keypair(), &config_loader)?).await?;
        info!("PEER_ID: {:?}", swarm.local_peer_id());

        Ok(Self { swarm, config, db, verification_runner: verification_runner_arc_mutex, secret_key, rpc, })
    }

    /// Handles incoming gossipsub `event` given the `topics` this node is interested in.
    /// Handling includes TX validation, storage in local db, or optionally triggering a broadcast
    /// of postceding TX to the network.
    fn handle_gossipsub_events(
        &mut self, event: gossipsub::Event, topics: Vec<&Topic>, domains: Vec<Domain>,
    ) -> Result<(), Error> {
        if let gossipsub::Event::Message { propagation_source, message_id, message } = event {
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
                        if let tx::Body::TrustUpdate(trust_update) = tx.body().clone() {
                            tx.verify_against(namespace.owner()).map_err(Error::Signature)?;
                            // Add Tx to db
                            tx.set_sequence_number(message.sequence_number.unwrap_or_default());
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            assert!(namespace == trust_update.trust_id());
                            let domain = domains
                                .iter()
                                .find(|x| &x.trust_namespace() == namespace)
                                .ok_or(Error::DomainNotFound(namespace.clone().to_hex()))?;
                            let mut verification_runner_mut = self.verification_runner.lock().unwrap();
                                verification_runner_mut.update_trust(domain.clone(), trust_update.entries().clone())
                                .map_err(Error::Runner)?;
                        } else {
                            return Err(Error::InvalidTxKind);
                        }
                    },
                    Topic::NamespaceSeedUpdate(namespace) => {
                        let tx_event =
                            TxEvent::decode(&mut message.data.as_slice()).map_err(Error::Decode)?;
                        let tx =
                            Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                        if let tx::Body::SeedUpdate(seed_update) = tx.body() {
                            tx.verify_against(namespace.owner()).map_err(Error::Signature)?;
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            assert!(namespace == seed_update.seed_id());
                            let domain = domains
                                .iter()
                                .find(|x| &x.trust_namespace() == namespace)
                                .ok_or(Error::DomainNotFound(namespace.clone().to_hex()))?;
                            let mut verification_runner_mut = self.verification_runner.lock().map_err(|e| Error::VerificationRunnerLockError(e.to_string()))?;
                            verification_runner_mut
                                .update_seed(domain.clone(), seed_update.entries().clone())
                                .map_err(Error::Runner)?;
                        } else {
                            return Err(Error::InvalidTxKind);
                        }
                    },
                    Topic::DomainAssignent(domain_id) => {
                        let tx_event =
                            TxEvent::decode(&mut message.data.as_slice()).map_err(Error::Decode)?;
                        let tx =
                            Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                        if let tx::Body::ComputeAssignment(compute_assignment) = tx.body() {
                            let address = tx.verify().map_err(Error::Signature)?;
                            assert!(self.config.whitelist.block_builder.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            let computer_address = address_from_sk(&self.secret_key);
                            assert_eq!(
                                computer_address,
                                *compute_assignment.assigned_verifier_node()
                            );
                            assert!(self
                                .config
                                .whitelist
                                .computer
                                .contains(compute_assignment.assigned_compute_node()));

                            let domain = domains
                                .iter()
                                .find(|x| &x.to_hash() == domain_id)
                                .ok_or(Error::DomainNotFound((*domain_id).to_hex()))?;
                            let mut verification_runner_mut = self.verification_runner.lock().map_err(|e| Error::VerificationRunnerLockError(e.to_string()))?;
                                verification_runner_mut
                                .update_assigment(domain.clone(), tx.hash())
                                .map_err(Error::Runner)?;
                            let res = verification_runner_mut
                                .check_finished_assignments(domain.clone())
                                .map_err(Error::Runner)?;
                            for (tx_hash, verification_res) in res {
                                let verification_res =
                                    compute::Verification::new(tx_hash, verification_res);
                                let mut tx = Tx::default_with(tx::Body::ComputeVerification(
                                    verification_res,
                                ));
                                tx.sign(&self.secret_key).map_err(Error::Signature)?;
                                broadcast_event(
                                    &mut self.swarm,
                                    tx,
                                    Topic::DomainVerification(*domain_id),
                                )
                                .map_err(|e| Error::P2P(e.to_string()))?;
                            }
                        } else {
                            return Err(Error::InvalidTxKind);
                        }
                    },
                    Topic::DomainScores(domain_id) => {
                        let tx_event =
                            TxEvent::decode(&mut message.data.as_slice()).map_err(Error::Decode)?;
                        let tx =
                            Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                        if let tx::Body::ComputeScores(compute_scores) = tx.body() {
                            let address = tx.verify().map_err(Error::Signature)?;
                            assert!(self.config.whitelist.computer.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            let domain = domains
                                .iter()
                                .find(|x| &x.to_hash() == domain_id)
                                .ok_or(Error::DomainNotFound((*domain_id).to_hex()))?;
                            let mut verification_runner_mut = self.verification_runner.lock().map_err(|e| Error::VerificationRunnerLockError(e.to_string()))?;
                            verification_runner_mut
                                .update_scores(domain.clone(), tx.hash(), compute_scores.clone())
                                .map_err(Error::Runner)?;
                            let res = verification_runner_mut
                                .check_finished_assignments(domain.clone())
                                .map_err(Error::Runner)?;
                            for (tx_hash, verification_res) in res {
                                let verification_res =
                                    compute::Verification::new(tx_hash, verification_res);
                                let mut tx = Tx::default_with(tx::Body::ComputeVerification(
                                    verification_res,
                                ));
                                tx.sign(&self.secret_key).map_err(Error::Signature)?;
                                broadcast_event(
                                    &mut self.swarm,
                                    tx,
                                    Topic::DomainVerification(*domain_id),
                                )
                                .map_err(|e| Error::P2P(e.to_string()))?;
                            }
                        } else {
                            return Err(Error::InvalidTxKind);
                        }
                    },
                    Topic::DomainCommitment(domain_id) => {
                        let tx_event =
                            TxEvent::decode(&mut message.data.as_slice()).map_err(Error::Decode)?;
                        let tx =
                            Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                        if let tx::Body::ComputeCommitment(compute_commitment) = tx.body() {
                            let address = tx.verify().map_err(Error::Signature)?;
                            assert!(self.config.whitelist.computer.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            let domain = domains
                                .iter()
                                .find(|x| &x.to_hash() == domain_id)
                                .ok_or(Error::DomainNotFound(domain_id.to_hex()))?;
                            let mut verification_runner_mut = self.verification_runner.lock().map_err(|e| Error::VerificationRunnerLockError(e.to_string()))?;
                            verification_runner_mut.update_commitment(compute_commitment.clone());
                            let res = verification_runner_mut
                                .check_finished_assignments(domain.clone())
                                .map_err(Error::Runner)?;
                            for (tx_hash, verification_res) in res {
                                let verification_res =
                                    compute::Verification::new(tx_hash, verification_res);
                                let mut tx = Tx::default_with(tx::Body::ComputeVerification(
                                    verification_res,
                                ));
                                tx.sign(&self.secret_key).map_err(Error::Signature)?;
                                broadcast_event(
                                    &mut self.swarm,
                                    tx,
                                    Topic::DomainVerification(*domain_id),
                                )
                                .map_err(|e| Error::P2P(e.to_string()))?;
                            }
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

    /// Recover VerificationRunner state from DB.
    ///
    /// - Load all the TXs from the DB
    /// - Just take TrustUpdate and SeedUpdate transactions
    /// - Update VerificationRunner using functions update_trust, update_seed
    pub fn node_recovery(&mut self) -> Result<(), Error> {
        // collect all trust update and seed update txs
        let mut txs = Vec::new();
        let mut trust_update_txs: Vec<Tx> =
            self.db.get_range_from_start(consts::TRUST_UPDATE, None, None).map_err(Error::Db)?;
        txs.append(&mut trust_update_txs);
        drop(trust_update_txs);

        let mut seed_update_txs: Vec<Tx> =
            self.db.get_range_from_start(consts::SEED_UPDATE, None, None).map_err(Error::Db)?;
        txs.append(&mut seed_update_txs);
        drop(seed_update_txs);

        // sort txs by sequence_number
        txs.sort_unstable_by_key(|tx| tx.get_sequence_number());

        // update verification runner
        let mut verification_runner_mut = self.verification_runner.lock().map_err(|e| Error::VerificationRunnerLockError(e.to_string()))?;
        for tx in txs {
            match tx.body() {
                tx::Body::TrustUpdate(trust_update) => {
                    let namespace = trust_update.trust_id().clone();
                    let domain = self
                        .config
                        .domains
                        .iter()
                        .find(|x| x.trust_namespace() == namespace)
                        .ok_or(Error::DomainNotFound(namespace.clone().to_hex()))?;
                    verification_runner_mut
                        .update_trust(domain.clone(), trust_update.entries().clone())
                        .map_err(Error::Runner)?;
                },
                tx::Body::SeedUpdate(seed_update) => {
                    let namespace = seed_update.seed_id().clone();
                    let domain = self
                        .config
                        .domains
                        .iter()
                        .find(|x| x.seed_namespace() == namespace)
                        .ok_or(Error::DomainNotFound(namespace.clone().to_hex()))?;
                    verification_runner_mut
                        .update_seed(domain.clone(), seed_update.entries().clone())
                        .map_err(Error::Runner)?;
                },
                _ => (),
            }
        }

        Ok(())
    }

    /// Runs the node:
    /// - Listens on all interfaces and whatever port the OS assigns.
    /// - subscribe to all the topics.
    /// - Handles gossipsub events.
    /// - Handles mDNS events.
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let topics_trust_update: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.trust_namespace())
            .map(|namespace| Topic::NamespaceTrustUpdate(namespace.clone()))
            .collect();
        let topics_seed_update: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.seed_namespace())
            .map(|namespace| Topic::NamespaceSeedUpdate(namespace.clone()))
            .collect();
        let topics_assignment: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(Topic::DomainAssignent)
            .collect();
        let topics_scores: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(Topic::DomainScores)
            .collect();
        let topics_commitment: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(Topic::DomainCommitment)
            .collect();

        let iter_chain = topics_assignment
            .iter()
            .chain(&topics_trust_update)
            .chain(&topics_seed_update)
            .chain(&topics_scores)
            .chain(&topics_commitment)
            .chain(&[Topic::ProposedBlock, Topic::FinalisedBlock]);
        for topic in iter_chain.clone() {
            // Create a Gossipsub topic
            let topic = gossipsub::IdentTopic::new(topic.clone());
            // subscribes to our topic
            self.swarm.behaviour_mut().gossipsub_subscribe(&topic)?;
        }

        net::listen_on(&mut self.swarm, self.config.p2p().listen_on())?;

        // spawn a rpc server
        let server = Server::builder().build(self.config.rpc().address()).await?;
        let handle = server.start(self.rpc.clone());
        tokio::spawn(handle.stopped());

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
                        let res = self.handle_gossipsub_events(
                            event,
                            iter_chain.clone().collect(),
                            self.config.domains.clone(),
                        );
                        if let Err(e) = res {
                            error!("Failed to handle gossipsub event: {e:?}");
                            continue;
                        }
                    },
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("Local node is listening on {address}");
                    }
                    e => info!("{:?}", e),
                }
            }
        }
    }
}
