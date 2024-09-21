use alloy_rlp::{encode, Decodable};
use dotenv::dotenv;
use futures::StreamExt;
use k256::ecdsa::SigningKey;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
    broadcast_event, build_node,
    db::{Db, DbError, DbItem},
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
}

fn handle_gossipsub_events(
    mut swarm: &mut Swarm<MyBehaviour>, db: &Db, event: gossipsub::Event, topics: Vec<&Topic>,
    whitelist: Whitelist, sk: &SigningKey,
) -> Result<(), BlockBuilderNodeError> {
    if let gossipsub::Event::Message { message_id, message, propagation_source } = event {
        for topic in topics {
            match topic {
                Topic::DomainRequest(domain_id) => {
                    let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                    if message.topic == topic_wrapper.hash() {
                        let tx_event = TxEvent::decode(&mut message.data.as_slice())
                            .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;
                        let tx = Tx::decode(&mut tx_event.data().as_slice())
                            .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;
                        if tx.kind() != TxKind::JobRunRequest {
                            return Err(BlockBuilderNodeError::InvalidTxKind);
                        }
                        let address =
                            tx.verify().map_err(|e| BlockBuilderNodeError::SignatureError(e))?;
                        assert!(whitelist.users.contains(&address));
                        // Add Tx to db
                        db.put(tx.clone()).map_err(|e| BlockBuilderNodeError::DbError(e))?;
                        let job_run_request = JobRunRequest::decode(&mut tx.body().as_slice())
                            .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;
                        assert_eq!(&job_run_request.domain_id, domain_id);

                        let assignment_topic = Topic::DomainAssignent(domain_id.clone());
                        let computer = whitelist.computer[0].clone();
                        let verifier = whitelist.verifier[0].clone();
                        let job_assignment = JobRunAssignment::new(tx.hash(), computer, verifier);
                        let mut tx =
                            Tx::default_with(TxKind::JobRunAssignment, encode(job_assignment));
                        tx.sign(sk).map_err(|e| BlockBuilderNodeError::SignatureError(e))?;
                        db.put(tx.clone()).map_err(|e| BlockBuilderNodeError::DbError(e))?;
                        broadcast_event(&mut swarm, tx, assignment_topic)
                            .map_err(|e| BlockBuilderNodeError::P2PError(e.to_string()))?;
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
                            .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;
                        let tx = Tx::decode(&mut tx_event.data().as_slice())
                            .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;
                        if tx.kind() != TxKind::CreateCommitment {
                            return Err(BlockBuilderNodeError::InvalidTxKind);
                        }
                        let address =
                            tx.verify().map_err(|e| BlockBuilderNodeError::SignatureError(e))?;
                        assert!(whitelist.computer.contains(&address));
                        // Add Tx to db
                        db.put(tx.clone()).map_err(|e| BlockBuilderNodeError::DbError(e))?;

                        let commitment = CreateCommitment::decode(&mut tx.body().as_slice())
                            .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;

                        let assignment_tx_key = Tx::construct_full_key(
                            TxKind::JobRunAssignment,
                            commitment.job_run_assignment_tx_hash,
                        );
                        let assignment_tx: Tx = db
                            .get(assignment_tx_key)
                            .map_err(|e| BlockBuilderNodeError::DbError(e))?;
                        let assignment_body =
                            JobRunAssignment::decode(&mut assignment_tx.body().as_slice())
                                .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;
                        let request_tx_key = Tx::construct_full_key(
                            TxKind::JobRunRequest,
                            assignment_body.job_run_request_tx_hash.clone(),
                        );
                        let request: Tx = db
                            .get(request_tx_key)
                            .map_err(|e| BlockBuilderNodeError::DbError(e))?;
                        let job_result_key =
                            JobResult::construct_full_key(assignment_body.job_run_request_tx_hash);
                        if let Err(DbError::NotFound) = db.get::<JobResult>(job_result_key) {
                            let result = JobResult::new(tx.hash(), Vec::new(), request.hash());
                            db.put(result).map_err(|e| BlockBuilderNodeError::DbError(e))?;
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
                            .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;
                        let tx = Tx::decode(&mut tx_event.data().as_slice())
                            .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;
                        if tx.kind() != TxKind::CreateScores {
                            return Err(BlockBuilderNodeError::InvalidTxKind);
                        }
                        let address =
                            tx.verify().map_err(|e| BlockBuilderNodeError::SignatureError(e))?;
                        assert!(whitelist.computer.contains(&address));
                        // Add Tx to db
                        db.put(tx.clone()).map_err(|e| BlockBuilderNodeError::DbError(e))?;
                        CreateScores::decode(&mut tx.body().as_slice())
                            .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;
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
                            .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;
                        let tx = Tx::decode(&mut tx_event.data().as_slice())
                            .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;
                        if tx.kind() != TxKind::JobVerification {
                            return Err(BlockBuilderNodeError::InvalidTxKind);
                        }
                        let address =
                            tx.verify().map_err(|e| BlockBuilderNodeError::SignatureError(e))?;
                        assert!(whitelist.verifier.contains(&address));
                        // Add Tx to db
                        db.put(tx.clone()).map_err(|e| BlockBuilderNodeError::DbError(e))?;
                        let job_verification =
                            JobVerification::decode(&mut tx.body().as_slice())
                                .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;

                        let assignment_tx_key = Tx::construct_full_key(
                            TxKind::JobRunAssignment,
                            job_verification.job_run_assignment_tx_hash,
                        );
                        let assignment_tx: Tx = db
                            .get(assignment_tx_key)
                            .map_err(|e| BlockBuilderNodeError::DbError(e))?;
                        let assignment_body =
                            JobRunAssignment::decode(&mut assignment_tx.body().as_slice())
                                .map_err(|e| BlockBuilderNodeError::DecodeError(e))?;
                        let job_result_key =
                            JobResult::construct_full_key(assignment_body.job_run_request_tx_hash);
                        let mut job_result: JobResult = db
                            .get(job_result_key)
                            .map_err(|e| BlockBuilderNodeError::DbError(e))?;
                        job_result.job_verification_tx_hashes.push(tx.hash());
                        db.put(job_result).map_err(|e| BlockBuilderNodeError::DbError(e))?;
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
        let swarm = build_node().await?;
        info!("PEER_ID: {:?}", swarm.local_peer_id());

        let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
        let secret_key_bytes = hex::decode(secret_key_hex)?;
        let secret_key = SigningKey::from_slice(secret_key_bytes.as_slice())?;

        let config: Config = toml::from_str(include_str!("../config.toml"))?;
        let db = Db::new("./local-storage", &[&Tx::get_cf(), &JobResult::get_cf()])?;

        Ok(Self { swarm, config, db, secret_key })
    }

    /// Runs the node:
    /// - Listens on all interfaces, on OS-assigned ephemeral ports.
    /// - Subscribes to all the topics.
    /// - Handles gossipsub events.
    /// - Handles mDNS events.
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        // Listen on all interfaces and whatever port the OS assigns
        self.swarm.listen_on("/ip4/0.0.0.0/udp/9000/quic-v1".parse()?)?;
        self.swarm.listen_on("/ip4/0.0.0.0/tcp/9000".parse()?)?;

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
                        let iter_chain = topics_requests.iter().chain(&topics_commitment).chain(&topics_scores).chain(&topics_verification);
                        let res = handle_gossipsub_events(
                            &mut self.swarm,
                            &self.db,
                            event,
                            iter_chain.collect(),
                            self.config.whitelist.clone(),
                            &self.secret_key
                        );
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
