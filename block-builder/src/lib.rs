#![deny(warnings)]

use alloy_rlp::{encode, Decodable};
use dotenv::dotenv;
use futures::StreamExt;
use k256::ecdsa::SigningKey;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
    broadcast_event, build_node, config,
    db::{self, Db, DbError, DbItem},
    net,
    result::JobResult,
    topics::{Domain, Topic},
    tx_event::TxEvent,
    txs::{
        Address, CreateCommitment, CreateScores, JobRunAssignment, JobRunRequest, JobVerification,
        Tx, TxKind,
    },
    MyBehaviour, MyBehaviourEvent,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::select;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod error;
use error::BlockBuilderNodeError;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The whitelist for the Block Builder.
pub struct Whitelist {
    /// The list of addresses that are allowed to be computers.
    pub computer: Vec<Address>,
    /// The list of addresses that are allowed to be verifiers.
    pub verifier: Vec<Address>,
    /// The list of addresses that are allowed to broadcast transactions.
    pub users: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The configuration for the Block Builder.
pub struct Config {
    /// The list of domains to process job run requests for.
    pub domains: Vec<Domain>,
    /// The whitelist for the Block Builder.
    pub whitelist: Whitelist,
    pub database: db::Config,
    pub p2p: net::Config,
}

/// The Block Builder node. It contains the Swarm, the Config, the DB, the SecretKey, and the JobRunner.
pub struct BlockBuilderNode {
    swarm: Swarm<MyBehaviour>,
    config: Config,
    db: Db,
    secret_key: SigningKey,
}

impl BlockBuilderNode {
    /// Initializes the node:
    /// - Loads the config from config.toml.
    /// - Initializes the Swarm.
    /// - Initializes the DB.
    /// - Initializes the JobRunner.
    /// - Initializes the Secret Key.
    pub async fn init() -> Result<Self, Box<dyn Error>> {
        dotenv().ok();
        tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();

        let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
        let secret_key_bytes = hex::decode(secret_key_hex)?;
        let secret_key = SigningKey::from_slice(secret_key_bytes.as_slice())?;

        let config_loader = config::Loader::new("openrank-block-builder")?;
        let config: Config = config_loader.load_or_create(include_str!("../config.toml"))?;
        let db = Db::new(&config.database, &[&Tx::get_cf(), &JobResult::get_cf()])?;

        let swarm = build_node(net::load_keypair(&config.p2p.keypair, &config_loader)?).await?;
        info!("PEER_ID: {:?}", swarm.local_peer_id());

        Ok(Self { swarm, config, db, secret_key })
    }

    /// Handles incoming gossipsub `event` given the `topics` this node is interested in.
    /// Handling includes TX validation, storage in local db, or optionally triggering a broadcast
    /// of postceding TX to the network.
    fn handle_gossipsub_events(
        &mut self, event: gossipsub::Event, topics: Vec<&Topic>,
    ) -> Result<(), BlockBuilderNodeError> {
        if let gossipsub::Event::Message { message_id, message, propagation_source } = event {
            for topic in topics {
                match topic {
                    Topic::NamespaceTrustUpdate(namespace) => {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        if message.topic == topic_wrapper.hash() {
                            let tx_event = TxEvent::decode(&mut message.data.as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            let mut tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            if tx.kind() != TxKind::TrustUpdate {
                                return Err(BlockBuilderNodeError::InvalidTxKind);
                            }
                            tx.verify_against(namespace.owner())
                                .map_err(BlockBuilderNodeError::SignatureError)?;
                            // Add Tx to db
                            tx.set_sequence_number(message.sequence_number.unwrap_or_default());
                            self.db.put(tx.clone()).map_err(BlockBuilderNodeError::DbError)?;
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
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            let mut tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            if tx.kind() != TxKind::SeedUpdate {
                                return Err(BlockBuilderNodeError::InvalidTxKind);
                            }
                            tx.verify_against(namespace.owner())
                                .map_err(BlockBuilderNodeError::SignatureError)?;
                            // Add Tx to db
                            tx.set_sequence_number(message.sequence_number.unwrap_or_default());
                            self.db.put(tx.clone()).map_err(BlockBuilderNodeError::DbError)?;
                            info!(
                                "TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
                                message.topic.as_str(),
                            );
                        }
                    },
                    Topic::DomainRequest(domain_id) => {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        if message.topic == topic_wrapper.hash() {
                            let tx_event = TxEvent::decode(&mut message.data.as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            let tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            if tx.kind() != TxKind::JobRunRequest {
                                return Err(BlockBuilderNodeError::InvalidTxKind);
                            }
                            let address =
                                tx.verify().map_err(BlockBuilderNodeError::SignatureError)?;
                            assert!(self.config.whitelist.users.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(BlockBuilderNodeError::DbError)?;
                            let job_run_request = JobRunRequest::decode(&mut tx.body().as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            assert_eq!(&job_run_request.domain_id, domain_id);

                            let assignment_topic = Topic::DomainAssignent(domain_id.clone());
                            let computer = self.config.whitelist.computer[0].clone();
                            let verifier = self.config.whitelist.verifier[0].clone();
                            let job_assignment =
                                JobRunAssignment::new(tx.hash(), computer, verifier);
                            let mut tx =
                                Tx::default_with(TxKind::JobRunAssignment, encode(job_assignment));
                            tx.sign(&self.secret_key)
                                .map_err(BlockBuilderNodeError::SignatureError)?;
                            self.db.put(tx.clone()).map_err(BlockBuilderNodeError::DbError)?;
                            broadcast_event(&mut self.swarm, tx.clone(), assignment_topic)
                                .map_err(|e| BlockBuilderNodeError::P2PError(e.to_string()))?;
                            // After broadcasting JobAssignment, save to db
                            self.db.put(tx).map_err(BlockBuilderNodeError::DbError)?;
                            info!(
                                "TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
                                message.topic.as_str(),
                            );
                        }
                    },
                    Topic::DomainCommitment(_) => {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        if message.topic == topic_wrapper.hash() {
                            let tx_event = TxEvent::decode(&mut message.data.as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            let tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            if tx.kind() != TxKind::CreateCommitment {
                                return Err(BlockBuilderNodeError::InvalidTxKind);
                            }
                            let address =
                                tx.verify().map_err(BlockBuilderNodeError::SignatureError)?;
                            assert!(self.config.whitelist.computer.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(BlockBuilderNodeError::DbError)?;

                            let commitment = CreateCommitment::decode(&mut tx.body().as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;

                            let assignment_tx_key = Tx::construct_full_key(
                                TxKind::JobRunAssignment,
                                commitment.job_run_assignment_tx_hash,
                            );
                            let assignment_tx: Tx = self
                                .db
                                .get(assignment_tx_key)
                                .map_err(BlockBuilderNodeError::DbError)?;
                            let assignment_body =
                                JobRunAssignment::decode(&mut assignment_tx.body().as_slice())
                                    .map_err(BlockBuilderNodeError::DecodeError)?;
                            let request_tx_key = Tx::construct_full_key(
                                TxKind::JobRunRequest,
                                assignment_body.job_run_request_tx_hash.clone(),
                            );
                            let request: Tx = self
                                .db
                                .get(request_tx_key)
                                .map_err(BlockBuilderNodeError::DbError)?;
                            let job_result_key = JobResult::construct_full_key(
                                assignment_body.job_run_request_tx_hash,
                            );
                            if let Err(DbError::NotFound) = self.db.get::<JobResult>(job_result_key)
                            {
                                let result = JobResult::new(tx.hash(), Vec::new(), request.hash());
                                self.db.put(result).map_err(BlockBuilderNodeError::DbError)?;
                            }
                            info!(
                                "TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
                                message.topic.as_str(),
                            );
                        }
                    },
                    Topic::DomainScores(_) => {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        if message.topic == topic_wrapper.hash() {
                            let tx_event = TxEvent::decode(&mut message.data.as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            let tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            if tx.kind() != TxKind::CreateScores {
                                return Err(BlockBuilderNodeError::InvalidTxKind);
                            }
                            let address =
                                tx.verify().map_err(BlockBuilderNodeError::SignatureError)?;
                            assert!(self.config.whitelist.computer.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(BlockBuilderNodeError::DbError)?;
                            CreateScores::decode(&mut tx.body().as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            info!(
                                "TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
                                message.topic.as_str(),
                            );
                        }
                    },
                    Topic::DomainVerification(_) => {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        if message.topic == topic_wrapper.hash() {
                            let tx_event = TxEvent::decode(&mut message.data.as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            let tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(BlockBuilderNodeError::DecodeError)?;
                            if tx.kind() != TxKind::JobVerification {
                                return Err(BlockBuilderNodeError::InvalidTxKind);
                            }
                            let address =
                                tx.verify().map_err(BlockBuilderNodeError::SignatureError)?;
                            assert!(self.config.whitelist.verifier.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(BlockBuilderNodeError::DbError)?;
                            let job_verification =
                                JobVerification::decode(&mut tx.body().as_slice())
                                    .map_err(BlockBuilderNodeError::DecodeError)?;

                            let assignment_tx_key = Tx::construct_full_key(
                                TxKind::JobRunAssignment,
                                job_verification.job_run_assignment_tx_hash,
                            );
                            let assignment_tx: Tx = self
                                .db
                                .get(assignment_tx_key)
                                .map_err(BlockBuilderNodeError::DbError)?;
                            let assignment_body =
                                JobRunAssignment::decode(&mut assignment_tx.body().as_slice())
                                    .map_err(BlockBuilderNodeError::DecodeError)?;
                            let job_result_key = JobResult::construct_full_key(
                                assignment_body.job_run_request_tx_hash,
                            );
                            let mut job_result: JobResult = self
                                .db
                                .get(job_result_key)
                                .map_err(BlockBuilderNodeError::DbError)?;
                            job_result.job_verification_tx_hashes.push(tx.hash());
                            self.db.put(job_result).map_err(BlockBuilderNodeError::DbError)?;
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

    /// Runs the node:
    /// - Listens on all interfaces, on OS-assigned ephemeral ports.
    /// - Subscribes to all the topics.
    /// - Handles gossipsub events.
    /// - Handles mDNS events.
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        net::listen_on(&mut self.swarm, &self.config.p2p.listen_on)?;

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
            .map(|domain| Topic::NamespaceTrustUpdate(domain.trust_namespace()))
            .collect();
        let topics_requests: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(|domain_hash| Topic::DomainRequest(domain_hash.clone()))
            .collect();
        let topics_commitment: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(|domain_hash| Topic::DomainCommitment(domain_hash.clone()))
            .collect();
        let topics_scores: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(|domain_hash| Topic::DomainScores(domain_hash.clone()))
            .collect();
        let topics_verification: Vec<Topic> = self
            .config
            .domains
            .clone()
            .into_iter()
            .map(|x| x.to_hash())
            .map(|domain_hash| Topic::DomainVerification(domain_hash.clone()))
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
            self.swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
        }

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
