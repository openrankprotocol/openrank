use alloy_rlp::{encode, Decodable};
use dotenv::dotenv;
use futures::StreamExt;
use k256::ecdsa::SigningKey;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
    address_from_sk, broadcast_event, build_node,
    db::{Db, DbItem},
    topics::{Domain, Topic},
    tx_event::TxEvent,
    txs::{
        job::{ComputeAssignment, ComputeCommitment},
        trust::{SeedUpdate, TrustUpdate},
        Address, Tx, TxHash, TxKind,
    },
    MyBehaviour, MyBehaviourEvent,
};
use runner::ComputeRunner;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::select;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod error;
mod runner;
use error::ComputeNodeError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Whitelist {
    block_builder: Vec<Address>,
    verifier: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub domains: Vec<Domain>,
    pub whitelist: Whitelist,
}

fn handle_gossipsub_events(
    swarm: &mut Swarm<MyBehaviour>, job_runner: &mut ComputeRunner, db: &Db,
    event: gossipsub::Event, topics: Vec<&Topic>, domains: Vec<Domain>, sk: &SigningKey,
    whitelist: &Whitelist,
) -> Result<(), ComputeNodeError> {
    if let gossipsub::Event::Message { propagation_source, message_id, message } = event {
        for topic in topics {
            match topic {
                Topic::NamespaceTrustUpdate(namespace) => {
                    let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                    if message.topic == topic_wrapper.hash() {
                        let tx_event = TxEvent::decode(&mut message.data.as_slice())
                            .map_err(|e| ComputeNodeError::DecodeError(e))?;
                        let mut tx = Tx::decode(&mut tx_event.data().as_slice())
                            .map_err(|e| ComputeNodeError::DecodeError(e))?;
                        if tx.kind() != TxKind::TrustUpdate {
                            return Err(ComputeNodeError::InvalidTxKind);
                        }
                        tx.verify_against(namespace.owner())
                            .map_err(|e| ComputeNodeError::SignatureError(e))?;
                        // Add Tx to db
                        tx.set_sequence_number(message.sequence_number.unwrap_or_default());
                        db.put(tx.clone()).map_err(|e| ComputeNodeError::DbError(e))?;
                        let trust_update = TrustUpdate::decode(&mut tx.body().as_slice())
                            .map_err(|e| ComputeNodeError::DecodeError(e))?;
                        assert!(*namespace == trust_update.trust_id);
                        let domain = domains
                            .iter()
                            .find(|x| &x.trust_namespace() == namespace)
                            .ok_or(ComputeNodeError::DomainNotFound(namespace.clone().to_hex()))?;
                        job_runner
                            .update_trust(domain.clone(), trust_update.entries.clone())
                            .map_err(Into::into)?;
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
                            .map_err(|e| ComputeNodeError::DecodeError(e))?;
                        let tx = Tx::decode(&mut tx_event.data().as_slice())
                            .map_err(|e| ComputeNodeError::DecodeError(e))?;
                        if tx.kind() != TxKind::SeedUpdate {
                            return Err(ComputeNodeError::InvalidTxKind);
                        }
                        tx.verify_against(namespace.owner())
                            .map_err(|e| ComputeNodeError::SignatureError(e))?;
                        // Add Tx to db
                        db.put(tx.clone()).map_err(|e| ComputeNodeError::DbError(e))?;
                        let seed_update = SeedUpdate::decode(&mut tx.body().as_slice())
                            .map_err(|e| ComputeNodeError::DecodeError(e))?;
                        assert!(*namespace == seed_update.seed_id);
                        let domain = domains
                            .iter()
                            .find(|x| &x.trust_namespace() == namespace)
                            .ok_or(ComputeNodeError::DomainNotFound(namespace.clone().to_hex()))?;
                        job_runner
                            .update_seed(domain.clone(), seed_update.entries.clone())
                            .map_err(Into::into)?;
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
                            .map_err(|e| ComputeNodeError::DecodeError(e))?;
                        let tx = Tx::decode(&mut tx_event.data().as_slice())
                            .map_err(|e| ComputeNodeError::DecodeError(e))?;
                        if tx.kind() != TxKind::ComputeAssignment {
                            return Err(ComputeNodeError::InvalidTxKind);
                        }
                        let address =
                            tx.verify().map_err(|e| ComputeNodeError::SignatureError(e))?;
                        assert!(whitelist.block_builder.contains(&address));
                        // Add Tx to db
                        db.put(tx.clone()).map_err(|e| ComputeNodeError::DbError(e))?;
                        // Not checking if we are assigned for the job, for now
                        let job_run_assignment =
                            ComputeAssignment::decode(&mut tx.body().as_slice())
                                .map_err(|e| ComputeNodeError::DecodeError(e))?;
                        let computer_address = address_from_sk(sk);
                        assert_eq!(computer_address, job_run_assignment.assigned_compute_node);
                        assert!(whitelist
                            .verifier
                            .contains(&job_run_assignment.assigned_verifier_node));

                        let domain = domains
                            .iter()
                            .find(|x| &x.to_hash() == domain_id)
                            .ok_or(ComputeNodeError::DomainNotFound(domain_id.clone().to_hex()))?;
                        job_runner.compute(domain.clone()).map_err(Into::into)?;
                        job_runner.create_compute_tree(domain.clone()).map_err(Into::into)?;
                        let job_scores =
                            job_runner.get_job_scores(domain.clone()).map_err(Into::into)?;
                        let (lt_root, compute_root) =
                            job_runner.get_root_hashes(domain.clone()).map_err(Into::into)?;

                        let job_scores_tx_res: Result<Vec<Tx>, ComputeNodeError> = job_scores
                            .iter()
                            .map(|tx_body| Tx::default_with(TxKind::ComputeScores, encode(tx_body)))
                            .map(|mut tx| {
                                tx.sign(sk).map_err(|e| ComputeNodeError::SignatureError(e))?;
                                Ok(tx)
                            })
                            .collect();
                        let job_scores_tx = job_scores_tx_res?;
                        let job_scores_tx_hashes: Vec<TxHash> =
                            job_scores_tx.iter().map(|x| x.hash()).collect();
                        let job_scores_topic = Topic::DomainScores(domain_id.clone());
                        let commitment_topic = Topic::DomainCommitment(domain_id.clone());
                        let job_commitment = ComputeCommitment::new(
                            tx.hash(),
                            lt_root,
                            compute_root,
                            job_scores_tx_hashes,
                        );
                        let mut job_commitment_tx =
                            Tx::default_with(TxKind::ComputeCommitment, encode(job_commitment));
                        job_commitment_tx
                            .sign(sk)
                            .map_err(|e| ComputeNodeError::SignatureError(e))?;
                        for scores in job_scores_tx {
                            broadcast_event(swarm, scores, job_scores_topic.clone())
                                .map_err(|e| ComputeNodeError::P2PError(e.to_string()))?;
                        }
                        broadcast_event(swarm, job_commitment_tx, commitment_topic)
                            .map_err(|e| ComputeNodeError::P2PError(e.to_string()))?;
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

pub struct ComputerNode {
    swarm: Swarm<MyBehaviour>,
    config: Config,
    db: Db,
    job_runner: ComputeRunner,
    secret_key: SigningKey,
}

impl ComputerNode {
    pub async fn init() -> Result<Self, Box<dyn Error>> {
        dotenv().ok();
        tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();
        let swarm = build_node().await?;
        info!("PEER_ID: {:?}", swarm.local_peer_id());

        let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
        let secret_key_bytes = hex::decode(secret_key_hex)?;
        let secret_key = SigningKey::from_slice(secret_key_bytes.as_slice())?;

        let config: Config = toml::from_str(include_str!("../config.toml"))?;
        let db = Db::new("./local-storage", &[&Tx::get_cf()])?;

        let domain_hashes = config.domains.iter().map(|x| x.to_hash()).collect();
        let job_runner = ComputeRunner::new(domain_hashes);

        Ok(Self { swarm, config, db, job_runner, secret_key })
    }

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

        let iter_chain = topics_assignment
            .iter()
            .chain(topics_trust_update.iter())
            .chain(topics_seed_update.iter())
            .chain(&[Topic::ProposedBlock, Topic::FinalisedBlock]);
        for topic in iter_chain.clone() {
            // Create a Gossipsub topic
            let topic = gossipsub::IdentTopic::new(topic.clone());
            // subscribes to our topic
            self.swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
        }

        // Listen on all interfaces and whatever port the OS assigns
        self.swarm.listen_on("/ip4/0.0.0.0/udp/10000/quic-v1".parse()?)?;
        self.swarm.listen_on("/ip4/0.0.0.0/tcp/10000".parse()?)?;

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
                        let res = handle_gossipsub_events(
                            &mut self.swarm,
                            &mut self.job_runner,
                            &self.db, event,
                            iter_chain.clone().collect(),
                            self.config.domains.clone(),
                            &self.secret_key,
                            &self.config.whitelist,
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
    pub fn node_recovery(&mut self) -> Result<(), ComputeNodeError> {
        // collect all trust update and seed update txs
        let mut txs = Vec::new();
        let mut trust_update_txs: Vec<Tx> = self
            .db
            .read_from_end(TxKind::TrustUpdate.into(), None)
            .map_err(|e| ComputeNodeError::DbError(e))?;
        txs.append(&mut trust_update_txs);
        drop(trust_update_txs);

        let mut seed_update_txs: Vec<Tx> = self
            .db
            .read_from_end(TxKind::SeedUpdate.into(), None)
            .map_err(|e| ComputeNodeError::DbError(e))?;
        txs.append(&mut seed_update_txs);
        drop(seed_update_txs);

        // sort txs by sequence_number
        txs.sort_unstable_by_key(|tx| tx.sequence_number());

        // update job runner
        for tx in txs {
            match tx.kind() {
                TxKind::TrustUpdate => {
                    let trust_update = TrustUpdate::decode(&mut tx.body().as_slice())
                        .map_err(|e| ComputeNodeError::DecodeError(e))?;
                    let namespace = trust_update.trust_id;
                    let domain = self
                        .config
                        .domains
                        .iter()
                        .find(|x| &x.trust_namespace() == &namespace)
                        .ok_or(ComputeNodeError::DomainNotFound(namespace.clone().to_hex()))?;
                    self.job_runner
                        .update_trust(domain.clone(), trust_update.entries.clone())
                        .map_err(|e| ComputeNodeError::ComputeInternalError(e))?;
                },
                TxKind::SeedUpdate => {
                    let seed_update = SeedUpdate::decode(&mut tx.body().as_slice())
                        .map_err(|e| ComputeNodeError::DecodeError(e))?;
                    let namespace = seed_update.seed_id;
                    let domain = self
                        .config
                        .domains
                        .iter()
                        .find(|x| &x.seed_namespace() == &namespace)
                        .ok_or(ComputeNodeError::DomainNotFound(namespace.clone().to_hex()))?;
                    self.job_runner
                        .update_seed(domain.clone(), seed_update.entries.clone())
                        .map_err(|e| ComputeNodeError::ComputeInternalError(e))?;
                },
                _ => (),
            }
        }

        Ok(())
    }
}
