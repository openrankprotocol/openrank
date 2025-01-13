use alloy_rlp::Decodable;
use dotenv::dotenv;
use futures::StreamExt;
use getset::Getters;
use jsonrpsee::{server::Server, RpcModule};
use k256::ecdsa::{self, SigningKey};
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
    address_from_sk, broadcast_event, build_node, config,
    db::{self, Db, DbItem},
    net,
    topics::{Domain, Topic},
    tx::{compute, consts, Address, Body, Tx, TxHash},
    tx_event::TxEvent,
    MyBehaviour, MyBehaviourEvent,
};
use rpc::{RpcServer, ComputerServer};
use serde::{Deserialize, Serialize};
use std::{fmt::{Display, Formatter, Result as FmtResult}, sync::{Arc, Mutex}};
use tokio::select;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use openrank_common::runners::compute_runner::{self as runner, ComputeRunner};

mod rpc;

#[derive(Debug)]
/// Errors that can arise while using the computer node.
pub enum Error {
    /// The decode error. This can arise when decoding a transaction.
    Decode(alloy_rlp::Error),
    /// The database error. The database error can occur when interacting with the database.
    Db(db::Error),
    /// The domain not found error. This can arise when the domain is not found in the config.
    DomainNotFound(String),
    /// The p2p error. This can arise when sending or receiving messages over the p2p network.
    P2P(String),
    /// The compute internal error. This can arise when there is an internal error in the compute runner.
    Runner(runner::Error),
    /// The signature error. This can arise when verifying a transaction signature.
    Signature(ecdsa::Error),
    /// The invalid tx kind error.
    InvalidTxKind,
    /// Cannot acquire lock of ComputeRunner
    ComputeRunnerLockError(String),
}

impl std::error::Error for Error {}

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
            Self::ComputeRunnerLockError(err) => write!(f, "ComputeRunnerLockError: {}", err),
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
struct Whitelist {
    block_builder: Vec<Address>,
    verifier: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
struct Config {
    domains: Vec<Domain>,
    whitelist: Whitelist,
    database: db::Config,
    p2p: net::Config,
    rpc: net::RpcConfig,
}

#[derive(Getters)]
#[getset(get = "pub")]
pub struct Node {
    swarm: Swarm<MyBehaviour>,
    config: Config,
    db: Db,
    compute_runner: Arc<Mutex<ComputeRunner>>,
    secret_key: SigningKey,
    rpc: RpcModule<ComputerServer>
}

impl Node {
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
                        if let Body::TrustUpdate(trust_update) = tx.body().clone() {
                            tx.verify_against(namespace.owner()).map_err(Error::Signature)?;
                            // Add Tx to db
                            tx.set_sequence_number(message.sequence_number.unwrap_or_default());
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            assert!(namespace == trust_update.trust_id());
                            let domain = domains
                                .iter()
                                .find(|x| &x.trust_namespace() == namespace)
                                .ok_or(Error::DomainNotFound(namespace.clone().to_hex()))?;
                            let mut computer_runner_mut = self.compute_runner.lock().map_err(|e| Error::ComputeRunnerLockError(e.to_string()))?;
                            computer_runner_mut
                                .update_trust(domain.clone(), trust_update.entries().clone())
                                .map_err(Error::Runner)?;
                        } else {
                            return Err(Error::InvalidTxKind);
                        }
                    },
                    Topic::NamespaceSeedUpdate(namespace) => {
                        let tx_event =
                            TxEvent::decode(&mut message.data.as_slice()).map_err(Error::Decode)?;
                        let mut tx =
                            Tx::decode(&mut tx_event.data().as_slice()).map_err(Error::Decode)?;
                        if let Body::SeedUpdate(seed_update) = tx.body().clone() {
                            tx.verify_against(namespace.owner()).map_err(Error::Signature)?;
                            // Add Tx to db
                            tx.set_sequence_number(message.sequence_number.unwrap_or_default());
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            assert!(namespace == seed_update.seed_id());
                            let domain = domains
                                .iter()
                                .find(|x| &x.trust_namespace() == namespace)
                                .ok_or(Error::DomainNotFound(namespace.clone().to_hex()))?;
                            let mut computer_runner_mut = self.compute_runner.lock().map_err(|e| Error::ComputeRunnerLockError(e.to_string()))?;
                            computer_runner_mut
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
                        if let Body::ComputeAssignment(compute_assignment) = tx.body() {
                            let address = tx.verify().map_err(Error::Signature)?;
                            assert!(self.config.whitelist.block_builder.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            let computer_address = address_from_sk(&self.secret_key);
                            assert_eq!(
                                &computer_address,
                                compute_assignment.assigned_compute_node()
                            );
                            assert!(self
                                .config
                                .whitelist
                                .verifier
                                .contains(compute_assignment.assigned_verifier_node()));

                            let domain = domains
                                .iter()
                                .find(|x| &x.to_hash() == domain_id)
                                .ok_or(Error::DomainNotFound((*domain_id).to_hex()))?;
                            let mut computer_runner_mut = self.compute_runner.lock().map_err(|e| Error::ComputeRunnerLockError(e.to_string()))?;
                            computer_runner_mut.compute(domain.clone()).map_err(Error::Runner)?;
                            computer_runner_mut
                                .create_compute_tree(domain.clone())
                                .map_err(Error::Runner)?;
                            let compute_scores = computer_runner_mut
                                .get_compute_scores(domain.clone())
                                .map_err(Error::Runner)?;
                            let (lt_root, compute_root) = computer_runner_mut
                                .get_root_hashes(domain.clone())
                                .map_err(Error::Runner)?;

                            let compute_scores_tx_res: Result<Vec<Tx>, Error> = compute_scores
                                .iter()
                                .map(|tx_body| {
                                    Tx::default_with(Body::ComputeScores(tx_body.clone()))
                                })
                                .map(|mut tx| {
                                    tx.sign(&self.secret_key).map_err(Error::Signature)?;
                                    Ok(tx)
                                })
                                .collect();
                            let compute_scores_tx = compute_scores_tx_res?;
                            let compute_scores_tx_hashes: Vec<TxHash> =
                                compute_scores_tx.iter().map(|x| x.hash()).collect();
                            let compute_scores_topic = Topic::DomainScores(*domain_id);
                            let commitment_topic = Topic::DomainCommitment(*domain_id);
                            let compute_commitment = compute::Commitment::new(
                                tx.hash(),
                                lt_root,
                                compute_root,
                                compute_scores_tx_hashes,
                            );
                            let mut compute_commitment_tx =
                                Tx::default_with(Body::ComputeCommitment(compute_commitment));
                            compute_commitment_tx
                                .sign(&self.secret_key)
                                .map_err(Error::Signature)?;
                            for scores in compute_scores_tx {
                                broadcast_event(
                                    &mut self.swarm,
                                    scores,
                                    compute_scores_topic.clone(),
                                )
                                .map_err(|e| Error::P2P(e.to_string()))?;
                            }
                            broadcast_event(
                                &mut self.swarm, compute_commitment_tx, commitment_topic,
                            )
                            .map_err(|e| Error::P2P(e.to_string()))?;
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

    /// Initializes the node:
    /// - Loads the config from config.toml.
    /// - Initializes the Swarm.
    /// - Initializes the DB.
    /// - Initializes the ComputeRunner.
    /// - Initializes the Secret Key.
    pub async fn init() -> Result<Self, Box<dyn std::error::Error>> {
        dotenv().ok();
        tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();

        let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
        let secret_key_bytes = hex::decode(secret_key_hex)?;
        let secret_key = SigningKey::from_slice(secret_key_bytes.as_slice())?;

        let config_loader = config::Loader::new("openrank-computer")?;
        let config: Config = config_loader.load_or_create(include_str!("../config.toml"))?;
        let db = Db::new(&config.database, &[&Tx::get_cf()])?;

        let compute_runner = ComputeRunner::new(&config.domains);

        let compute_runner_arc_mutex = Arc::new(Mutex::new(compute_runner));
        let computer_server = ComputerServer::new(compute_runner_arc_mutex.clone());
        let rpc = computer_server.into_rpc();

        let swarm = build_node(net::load_keypair(config.p2p().keypair(), &config_loader)?).await?;
        info!("PEER_ID: {:?}", swarm.local_peer_id());

        Ok(Self { swarm, config, db, compute_runner: compute_runner_arc_mutex, secret_key, rpc })
    }

    /// Runs the node:
    /// - Listens on all interfaces, on OS-assigned ephemeral ports.
    /// - Subscribes to all the topics.
    /// - Handles gossipsub events.
    /// - Handles mDNS events.
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let topics_trust_update: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.trust_namespace())
            .map(Topic::NamespaceTrustUpdate)
            .collect();
        let topics_seed_update: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.seed_namespace())
            .map(Topic::NamespaceSeedUpdate)
            .collect();
        let topics_assignment: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(Topic::DomainAssignent)
            .collect();

        let iter_chain = topics_assignment
            .iter()
            .chain(topics_trust_update.iter())
            .chain(topics_seed_update.iter())
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
                            event, iter_chain.clone().collect(), self.config.domains.clone(),
                        );
                        if let Err(e) = res {
                            error!("Gossipsub error: {e:?}");
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

    /// Recover ComputeRunner state from DB.
    ///
    /// - Load all the TXs from the DB
    /// - Just take TrustUpdate and SeedUpdate transactions
    /// - Update ComputeRunner using functions update_trust, update_seed
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

        // update compute runner
        for tx in txs {
            match tx.body() {
                Body::TrustUpdate(trust_update) => {
                    let namespace = trust_update.trust_id().clone();
                    let domain = self
                        .config
                        .domains
                        .iter()
                        .find(|x| x.trust_namespace() == namespace)
                        .ok_or(Error::DomainNotFound(namespace.clone().to_hex()))?;
                    let mut computer_runner_mut = self.compute_runner.lock().map_err(|e| Error::ComputeRunnerLockError(e.to_string()))?;
                    computer_runner_mut
                        .update_trust(domain.clone(), trust_update.entries().clone())
                        .map_err(Error::Runner)?;
                },
                Body::SeedUpdate(seed_update) => {
                    let namespace = seed_update.seed_id().clone();
                    let domain = self
                        .config
                        .domains
                        .iter()
                        .find(|x| x.seed_namespace() == namespace)
                        .ok_or(Error::DomainNotFound(namespace.clone().to_hex()))?;
                    let mut computer_runner_mut = self.compute_runner.lock().map_err(|e| Error::ComputeRunnerLockError(e.to_string()))?;
                    computer_runner_mut
                        .update_seed(domain.clone(), seed_update.entries().clone())
                        .map_err(Error::Runner)?;
                },
                _ => (),
            }
        }

        Ok(())
    }
}
