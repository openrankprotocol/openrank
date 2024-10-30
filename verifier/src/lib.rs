use alloy_rlp::{encode, Decodable};
use dotenv::dotenv;
use futures::StreamExt;
use k256::ecdsa;
use k256::ecdsa::SigningKey;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
    address_from_sk, broadcast_event, build_node, config,
    db::{self, Db, DbItem},
    net,
    topics::{Domain, Topic},
    tx_event::TxEvent,
    txs::{
        compute,
        trust::{SeedUpdate, TrustUpdate},
        Address, Kind, Tx,
    },
    MyBehaviour, MyBehaviourEvent,
};
use runner::VerificationRunner;
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};
use tokio::select;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod runner;

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
        }
    }
}

impl From<runner::Error> for Error {
    fn from(val: runner::Error) -> Self {
        Error::Runner(val)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The whitelist for the Verifier.
pub struct Whitelist {
    /// The list of addresses that are allowed to be block builders.
    block_builder: Vec<Address>,
    /// The list of addresses that are allowed to be computers.
    computer: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The configuration for the Verifier.
pub struct Config {
    /// The list of domains to perform verification for.
    pub domains: Vec<Domain>,
    /// The whitelist for the Verifier.
    pub whitelist: Whitelist,
    pub database: db::Config,
    pub p2p: net::Config,
}

/// The Verifier node. It contains the Swarm, the Config, the DB, the VerificationRunner, and the SecretKey.
pub struct Node {
    swarm: Swarm<MyBehaviour>,
    config: Config,
    db: Db,
    verification_runner: VerificationRunner,
    secret_key: SigningKey,
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
        let domain_hashes = config.domains.iter().map(|x| x.to_hash()).collect();
        let verification_runner = VerificationRunner::new(domain_hashes);

        let swarm = build_node(net::load_keypair(&config.p2p.keypair, &config_loader)?).await?;
        info!("PEER_ID: {:?}", swarm.local_peer_id());

        Ok(Self { swarm, config, db, verification_runner, secret_key })
    }

    /// Handles incoming gossipsub `event` given the `topics` this node is interested in.
    /// Handling includes TX validation, storage in local db, or optionally triggering a broadcast
    /// of postceding TX to the network.
    fn handle_gossipsub_events(
        &mut self, event: gossipsub::Event, topics: Vec<&Topic>, domains: Vec<Domain>,
    ) -> Result<(), Error> {
        if let gossipsub::Event::Message { propagation_source, message_id, message } = event {
            for topic in topics {
                match topic {
                    Topic::NamespaceTrustUpdate(namespace) => {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        if message.topic == topic_wrapper.hash() {
                            let tx_event = TxEvent::decode(&mut message.data.as_slice())
                                .map_err(Error::Decode)?;
                            let mut tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(Error::Decode)?;
                            if *tx.kind() != Kind::TrustUpdate {
                                return Err(Error::InvalidTxKind);
                            }
                            tx.verify_against(namespace.owner()).map_err(Error::Signature)?;
                            // Add Tx to db
                            tx.set_sequence_number(message.sequence_number.unwrap_or_default());
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            let trust_update = TrustUpdate::decode(&mut tx.body().as_slice())
                                .map_err(Error::Decode)?;
                            assert!(*namespace == trust_update.trust_id);
                            let domain = domains
                                .iter()
                                .find(|x| &x.trust_namespace() == namespace)
                                .ok_or(Error::DomainNotFound(namespace.clone().to_hex()))?;
                            self.verification_runner
                                .update_trust(domain.clone(), trust_update.entries.clone())
                                .map_err(Error::Runner)?;
                            info!(
                                "TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
                                message.topic.as_str(),
                            );
                        }
                    },
                    Topic::NamespaceSeedUpdate(namespace) => {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        if message.topic == topic_wrapper.hash() {
                            let tx_event = TxEvent::decode(&mut message.data.as_slice())
                                .map_err(Error::Decode)?;
                            let tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(Error::Decode)?;
                            if *tx.kind() != Kind::SeedUpdate {
                                return Err(Error::InvalidTxKind);
                            }
                            tx.verify_against(namespace.owner()).map_err(Error::Signature)?;
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            let seed_update = SeedUpdate::decode(&mut tx.body().as_slice())
                                .map_err(Error::Decode)?;
                            assert!(*namespace == seed_update.seed_id);
                            let domain = domains
                                .iter()
                                .find(|x| &x.trust_namespace() == namespace)
                                .ok_or(Error::DomainNotFound(namespace.clone().to_hex()))?;
                            self.verification_runner
                                .update_seed(domain.clone(), seed_update.entries.clone())
                                .map_err(Error::Runner)?;
                            info!(
                                "TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
                                message.topic.as_str(),
                            );
                        }
                    },
                    Topic::DomainAssignent(domain_id) => {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        if message.topic == topic_wrapper.hash() {
                            let tx_event = TxEvent::decode(&mut message.data.as_slice())
                                .map_err(Error::Decode)?;
                            let tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(Error::Decode)?;
                            if *tx.kind() != Kind::ComputeAssignment {
                                return Err(Error::InvalidTxKind);
                            }
                            let address = tx.verify().map_err(Error::Signature)?;
                            assert!(self.config.whitelist.block_builder.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            // Not checking if this node is assigned for the compute
                            let compute_assignment =
                                compute::Assignment::decode(&mut tx.body().as_slice())
                                    .map_err(Error::Decode)?;
                            let computer_address = address_from_sk(&self.secret_key);
                            assert_eq!(computer_address, compute_assignment.assigned_verifier_node);
                            assert!(self
                                .config
                                .whitelist
                                .computer
                                .contains(&compute_assignment.assigned_compute_node));

                            let domain = domains
                                .iter()
                                .find(|x| &x.to_hash() == domain_id)
                                .ok_or(Error::DomainNotFound(domain_id.clone().to_hex()))?;
                            self.verification_runner
                                .update_assigment(domain.clone(), tx.hash())
                                .map_err(Error::Runner)?;
                            let res = self
                                .verification_runner
                                .check_finished_assignments(domain.clone())
                                .map_err(Error::Runner)?;
                            for (tx_hash, verification_res) in res {
                                let verification_res =
                                    compute::Verification::new(tx_hash, verification_res);
                                let mut tx = Tx::default_with(
                                    Kind::ComputeVerification,
                                    encode(verification_res),
                                );
                                tx.sign(&self.secret_key).map_err(Error::Signature)?;
                                broadcast_event(
                                    &mut self.swarm,
                                    tx,
                                    Topic::DomainVerification(domain_id.clone()),
                                )
                                .map_err(|e| Error::P2P(e.to_string()))?;
                            }
                            info!(
                                "TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
                                message.topic.as_str(),
                            );
                        }
                    },
                    Topic::DomainScores(domain_id) => {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        if message.topic == topic_wrapper.hash() {
                            let tx_event = TxEvent::decode(&mut message.data.as_slice())
                                .map_err(Error::Decode)?;
                            let tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(Error::Decode)?;
                            if *tx.kind() != Kind::ComputeScores {
                                return Err(Error::InvalidTxKind);
                            }
                            let address = tx.verify().map_err(Error::Signature)?;
                            assert!(self.config.whitelist.computer.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            let compute_scores = compute::Scores::decode(&mut tx.body().as_slice())
                                .map_err(Error::Decode)?;

                            let domain = domains
                                .iter()
                                .find(|x| &x.to_hash() == domain_id)
                                .ok_or(Error::DomainNotFound(domain_id.clone().to_hex()))?;
                            self.verification_runner
                                .update_scores(domain.clone(), tx.hash(), compute_scores.clone())
                                .map_err(Error::Runner)?;
                            let res = self
                                .verification_runner
                                .check_finished_assignments(domain.clone())
                                .map_err(Error::Runner)?;
                            for (tx_hash, verification_res) in res {
                                let verification_res =
                                    compute::Verification::new(tx_hash, verification_res);
                                let mut tx = Tx::default_with(
                                    Kind::ComputeVerification,
                                    encode(verification_res),
                                );
                                tx.sign(&self.secret_key).map_err(Error::Signature)?;
                                broadcast_event(
                                    &mut self.swarm,
                                    tx,
                                    Topic::DomainVerification(domain_id.clone()),
                                )
                                .map_err(|e| Error::P2P(e.to_string()))?;
                            }
                            info!(
                                "TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
                                message.topic.as_str(),
                            );
                        }
                    },
                    Topic::DomainCommitment(domain_id) => {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        if message.topic == topic_wrapper.hash() {
                            let tx_event = TxEvent::decode(&mut message.data.as_slice())
                                .map_err(Error::Decode)?;
                            let tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(Error::Decode)?;
                            if *tx.kind() != Kind::ComputeCommitment {
                                return Err(Error::InvalidTxKind);
                            }
                            let address = tx.verify().map_err(Error::Signature)?;
                            assert!(self.config.whitelist.computer.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(Error::Db)?;
                            let compute_commitment =
                                compute::Commitment::decode(&mut tx.body().as_slice())
                                    .map_err(Error::Decode)?;

                            let domain = domains
                                .iter()
                                .find(|x| &x.to_hash() == domain_id)
                                .ok_or(Error::DomainNotFound(domain_id.clone().to_hex()))?;
                            self.verification_runner.update_commitment(compute_commitment.clone());
                            let res = self
                                .verification_runner
                                .check_finished_assignments(domain.clone())
                                .map_err(Error::Runner)?;
                            for (tx_hash, verification_res) in res {
                                let verification_res =
                                    compute::Verification::new(tx_hash, verification_res);
                                let mut tx = Tx::default_with(
                                    Kind::ComputeVerification,
                                    encode(verification_res),
                                );
                                tx.sign(&self.secret_key).map_err(Error::Signature)?;
                                broadcast_event(
                                    &mut self.swarm,
                                    tx,
                                    Topic::DomainVerification(domain_id.clone()),
                                )
                                .map_err(|e| Error::P2P(e.to_string()))?;
                            }
                            info!(
                                "TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
                                message.topic.as_str(),
                            );
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
            self.db.read_from_end(Kind::TrustUpdate.into(), None).map_err(Error::Db)?;
        txs.append(&mut trust_update_txs);
        drop(trust_update_txs);

        let mut seed_update_txs: Vec<Tx> =
            self.db.read_from_end(Kind::SeedUpdate.into(), None).map_err(Error::Db)?;
        txs.append(&mut seed_update_txs);
        drop(seed_update_txs);

        // sort txs by sequence_number
        txs.sort_unstable_by_key(|tx| tx.sequence_number());

        // update verification runner
        for tx in txs {
            match tx.kind() {
                Kind::TrustUpdate => {
                    let trust_update =
                        TrustUpdate::decode(&mut tx.body().as_slice()).map_err(Error::Decode)?;
                    let namespace = trust_update.trust_id;
                    let domain = self
                        .config
                        .domains
                        .iter()
                        .find(|x| x.trust_namespace() == namespace)
                        .ok_or(Error::DomainNotFound(namespace.clone().to_hex()))?;
                    self.verification_runner
                        .update_trust(domain.clone(), trust_update.entries.clone())
                        .map_err(Error::Runner)?;
                },
                Kind::SeedUpdate => {
                    let seed_update =
                        SeedUpdate::decode(&mut tx.body().as_slice()).map_err(Error::Decode)?;
                    let namespace = seed_update.seed_id;
                    let domain = self
                        .config
                        .domains
                        .iter()
                        .find(|x| x.seed_namespace() == namespace)
                        .ok_or(Error::DomainNotFound(namespace.clone().to_hex()))?;
                    self.verification_runner
                        .update_seed(domain.clone(), seed_update.entries.clone())
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
            .map(|domain_hash| Topic::DomainAssignent(domain_hash.clone()))
            .collect();
        let topics_scores: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(|domain_hash| Topic::DomainScores(domain_hash.clone()))
            .collect();
        let topics_commitment: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(|domain_hash| Topic::DomainCommitment(domain_hash.clone()))
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
            self.swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
        }

        net::listen_on(&mut self.swarm, &self.config.p2p.listen_on)?;

        // Kick it off
        loop {
            select! {
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
