use alloy_rlp::{encode, Decodable};
use dotenv::dotenv;
use futures::StreamExt;
use k256::ecdsa::SigningKey;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
    address_from_sk, broadcast_event, build_node, config,
    db::{Db, DbItem},
    topics::{Domain, Topic},
    tx_event::TxEvent,
    txs::{
        Address, CreateCommitment, CreateScores, JobRunAssignment, JobVerification, SeedUpdate,
        TrustUpdate, Tx, TxKind,
    },
    MyBehaviour, MyBehaviourEvent,
};
use runner::VerificationJobRunner;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::select;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod error;
mod runner;
use error::VerifierNodeError;

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
    /// The list of domains to perform verification jobs for.
    pub domains: Vec<Domain>,
    /// The whitelist for the Verifier.
    pub whitelist: Whitelist,
}

/// The Verifier node. It contains the Swarm, the Config, the DB, the JobRunner, and the SecretKey.
pub struct VerifierNode {
    swarm: Swarm<MyBehaviour>,
    config: Config,
    db: Db,
    job_runner: VerificationJobRunner,
    secret_key: SigningKey,
}

impl VerifierNode {
    /// Initializes the node:
    /// - Loads the config from config.toml.
    /// - Initializes the Swarm.
    /// - Initializes the DB.
    /// - Initializes the JobRunner.
    pub async fn init() -> Result<Self, Box<dyn Error>> {
        dotenv().ok();
        tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();

        let swarm = build_node().await?;
        info!("PEER_ID: {:?}", swarm.local_peer_id());

        let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
        let secret_key_bytes = hex::decode(secret_key_hex)?;
        let secret_key = SigningKey::from_slice(secret_key_bytes.as_slice())?;

        let config_loader = config::Loader::new("openrank-verifier")?;
        let config: Config = config_loader.load_or_create(include_str!("../config.toml"))?;
        let db = Db::new("./local-storage", &[&Tx::get_cf()])?;
        let domain_hashes = config.domains.iter().map(|x| x.to_hash()).collect();
        let job_runner = VerificationJobRunner::new(domain_hashes);

        Ok(Self { swarm, config, db, job_runner, secret_key })
    }

    /// Handles incoming gossipsub `event` given the `topics` this node is interested in.
    /// Handling includes TX validation, storage in local db, or optionally triggering a broadcast
    /// of postceding TX to the network.
    fn handle_gossipsub_events(
        &mut self, event: gossipsub::Event, topics: Vec<&Topic>, domains: Vec<Domain>,
    ) -> Result<(), VerifierNodeError> {
        if let gossipsub::Event::Message { propagation_source, message_id, message } = event {
            for topic in topics {
                match topic {
                    Topic::NamespaceTrustUpdate(namespace) => {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        if message.topic == topic_wrapper.hash() {
                            let tx_event = TxEvent::decode(&mut message.data.as_slice())
                                .map_err(VerifierNodeError::DecodeError)?;
                            let mut tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(VerifierNodeError::DecodeError)?;
                            if tx.kind() != TxKind::TrustUpdate {
                                return Err(VerifierNodeError::InvalidTxKind);
                            }
                            tx.verify_against(namespace.owner())
                                .map_err(VerifierNodeError::SignatureError)?;
                            // Add Tx to db
                            tx.set_sequence_number(message.sequence_number.unwrap_or_default());
                            self.db.put(tx.clone()).map_err(VerifierNodeError::DbError)?;
                            let trust_update = TrustUpdate::decode(&mut tx.body().as_slice())
                                .map_err(VerifierNodeError::DecodeError)?;
                            assert!(*namespace == trust_update.trust_id);
                            let domain =
                                domains.iter().find(|x| &x.trust_namespace() == namespace).ok_or(
                                    VerifierNodeError::DomainNotFound(namespace.clone().to_hex()),
                                )?;
                            self.job_runner
                                .update_trust(domain.clone(), trust_update.entries.clone())
                                .map_err(VerifierNodeError::ComputeInternalError)?;
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
                                .map_err(VerifierNodeError::DecodeError)?;
                            let tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(VerifierNodeError::DecodeError)?;
                            if tx.kind() != TxKind::SeedUpdate {
                                return Err(VerifierNodeError::InvalidTxKind);
                            }
                            tx.verify_against(namespace.owner())
                                .map_err(VerifierNodeError::SignatureError)?;
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(VerifierNodeError::DbError)?;
                            let seed_update = SeedUpdate::decode(&mut tx.body().as_slice())
                                .map_err(VerifierNodeError::DecodeError)?;
                            assert!(*namespace == seed_update.seed_id);
                            let domain =
                                domains.iter().find(|x| &x.trust_namespace() == namespace).ok_or(
                                    VerifierNodeError::DomainNotFound(namespace.clone().to_hex()),
                                )?;
                            self.job_runner
                                .update_seed(domain.clone(), seed_update.entries.clone())
                                .map_err(VerifierNodeError::ComputeInternalError)?;
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
                                .map_err(VerifierNodeError::DecodeError)?;
                            let tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(VerifierNodeError::DecodeError)?;
                            if tx.kind() != TxKind::JobRunAssignment {
                                return Err(VerifierNodeError::InvalidTxKind);
                            }
                            let address = tx.verify().map_err(VerifierNodeError::SignatureError)?;
                            assert!(self.config.whitelist.block_builder.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(VerifierNodeError::DbError)?;
                            // Not checking if this node is assigned for the job
                            let job_run_assignment =
                                JobRunAssignment::decode(&mut tx.body().as_slice())
                                    .map_err(VerifierNodeError::DecodeError)?;
                            let computer_address = address_from_sk(&self.secret_key);
                            assert_eq!(computer_address, job_run_assignment.assigned_verifier_node);
                            assert!(self
                                .config
                                .whitelist
                                .computer
                                .contains(&job_run_assignment.assigned_compute_node));

                            let domain = domains.iter().find(|x| &x.to_hash() == domain_id).ok_or(
                                VerifierNodeError::DomainNotFound(domain_id.clone().to_hex()),
                            )?;
                            self.job_runner
                                .update_assigment(domain.clone(), tx.hash())
                                .map_err(VerifierNodeError::ComputeInternalError)?;
                            let res = self
                                .job_runner
                                .check_finished_jobs(domain.clone())
                                .map_err(VerifierNodeError::ComputeInternalError)?;
                            for (tx_hash, verification_res) in res {
                                let verification_res =
                                    JobVerification::new(tx_hash, verification_res);
                                let mut tx = Tx::default_with(
                                    TxKind::JobVerification,
                                    encode(verification_res),
                                );
                                tx.sign(&self.secret_key)
                                    .map_err(VerifierNodeError::SignatureError)?;
                                broadcast_event(
                                    &mut self.swarm,
                                    tx,
                                    Topic::DomainVerification(domain_id.clone()),
                                )
                                .map_err(|e| VerifierNodeError::P2PError(e.to_string()))?;
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
                                .map_err(VerifierNodeError::DecodeError)?;
                            let tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(VerifierNodeError::DecodeError)?;
                            if tx.kind() != TxKind::CreateScores {
                                return Err(VerifierNodeError::InvalidTxKind);
                            }
                            let address = tx.verify().map_err(VerifierNodeError::SignatureError)?;
                            assert!(self.config.whitelist.computer.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(VerifierNodeError::DbError)?;
                            let create_scores = CreateScores::decode(&mut tx.body().as_slice())
                                .map_err(VerifierNodeError::DecodeError)?;

                            let domain = domains.iter().find(|x| &x.to_hash() == domain_id).ok_or(
                                VerifierNodeError::DomainNotFound(domain_id.clone().to_hex()),
                            )?;
                            self.job_runner
                                .update_scores(domain.clone(), tx.hash(), create_scores.clone())
                                .map_err(VerifierNodeError::ComputeInternalError)?;
                            let res = self
                                .job_runner
                                .check_finished_jobs(domain.clone())
                                .map_err(VerifierNodeError::ComputeInternalError)?;
                            for (tx_hash, verification_res) in res {
                                let verification_res =
                                    JobVerification::new(tx_hash, verification_res);
                                let mut tx = Tx::default_with(
                                    TxKind::JobVerification,
                                    encode(verification_res),
                                );
                                tx.sign(&self.secret_key)
                                    .map_err(VerifierNodeError::SignatureError)?;
                                broadcast_event(
                                    &mut self.swarm,
                                    tx,
                                    Topic::DomainVerification(domain_id.clone()),
                                )
                                .map_err(|e| VerifierNodeError::P2PError(e.to_string()))?;
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
                                .map_err(VerifierNodeError::DecodeError)?;
                            let tx = Tx::decode(&mut tx_event.data().as_slice())
                                .map_err(VerifierNodeError::DecodeError)?;
                            if tx.kind() != TxKind::CreateCommitment {
                                return Err(VerifierNodeError::InvalidTxKind);
                            }
                            let address = tx.verify().map_err(VerifierNodeError::SignatureError)?;
                            assert!(self.config.whitelist.computer.contains(&address));
                            // Add Tx to db
                            self.db.put(tx.clone()).map_err(VerifierNodeError::DbError)?;
                            let create_commitment =
                                CreateCommitment::decode(&mut tx.body().as_slice())
                                    .map_err(VerifierNodeError::DecodeError)?;

                            let domain = domains.iter().find(|x| &x.to_hash() == domain_id).ok_or(
                                VerifierNodeError::DomainNotFound(domain_id.clone().to_hex()),
                            )?;
                            self.job_runner.update_commitment(create_commitment.clone());
                            let res = self
                                .job_runner
                                .check_finished_jobs(domain.clone())
                                .map_err(VerifierNodeError::ComputeInternalError)?;
                            for (tx_hash, verification_res) in res {
                                let verification_res =
                                    JobVerification::new(tx_hash, verification_res);
                                let mut tx = Tx::default_with(
                                    TxKind::JobVerification,
                                    encode(verification_res),
                                );
                                tx.sign(&self.secret_key)
                                    .map_err(VerifierNodeError::SignatureError)?;
                                broadcast_event(
                                    &mut self.swarm,
                                    tx,
                                    Topic::DomainVerification(domain_id.clone()),
                                )
                                .map_err(|e| VerifierNodeError::P2PError(e.to_string()))?;
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

    /// Recover JobRunner state from DB.
    ///
    /// - Loads all the TXs from the DB.
    /// - Takes TrustUpdate and SeedUpdate transactions.
    /// - Updates JobRunner using functions update_trust, update_seed.
    pub fn node_recovery(&mut self) -> Result<(), VerifierNodeError> {
        // collect all trust update and seed update txs
        let mut txs = Vec::new();
        let mut trust_update_txs: Vec<Tx> = self
            .db
            .read_from_end(TxKind::TrustUpdate.into(), None)
            .map_err(VerifierNodeError::DbError)?;
        txs.append(&mut trust_update_txs);
        drop(trust_update_txs);

        let mut seed_update_txs: Vec<Tx> = self
            .db
            .read_from_end(TxKind::SeedUpdate.into(), None)
            .map_err(VerifierNodeError::DbError)?;
        txs.append(&mut seed_update_txs);
        drop(seed_update_txs);

        // sort txs by sequence_number
        txs.sort_unstable_by_key(|tx| tx.sequence_number());

        // update job runner
        for tx in txs {
            match tx.kind() {
                TxKind::TrustUpdate => {
                    let trust_update = TrustUpdate::decode(&mut tx.body().as_slice())
                        .map_err(VerifierNodeError::DecodeError)?;
                    let namespace = trust_update.trust_id;
                    let domain = self
                        .config
                        .domains
                        .iter()
                        .find(|x| x.trust_namespace() == namespace)
                        .ok_or(VerifierNodeError::DomainNotFound(
                            namespace.clone().to_hex(),
                        ))?;
                    self.job_runner
                        .update_trust(domain.clone(), trust_update.entries.clone())
                        .map_err(VerifierNodeError::ComputeInternalError)?;
                },
                TxKind::SeedUpdate => {
                    let seed_update = SeedUpdate::decode(&mut tx.body().as_slice())
                        .map_err(VerifierNodeError::DecodeError)?;
                    let namespace = seed_update.seed_id;
                    let domain = self
                        .config
                        .domains
                        .iter()
                        .find(|x| x.seed_namespace() == namespace)
                        .ok_or(VerifierNodeError::DomainNotFound(
                            namespace.clone().to_hex(),
                        ))?;
                    self.job_runner
                        .update_seed(domain.clone(), seed_update.entries.clone())
                        .map_err(VerifierNodeError::ComputeInternalError)?;
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
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
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

        // Listen on all interfaces and whatever port the OS assigns
        self.swarm.listen_on("/ip4/0.0.0.0/udp/11001/quic-v1".parse()?)?;
        self.swarm.listen_on("/ip4/0.0.0.0/tcp/11001".parse()?)?;

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
