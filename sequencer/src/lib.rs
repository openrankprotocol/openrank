use alloy_rlp::{encode, Decodable};
use futures::StreamExt;
use karyon_jsonrpc::{rpc_impl, RPCError, Server};
use libp2p::{gossipsub, mdns, swarm::SwarmEvent};
use openrank_common::{
    build_node,
    db::{Db, DbItem},
    result::JobResult,
    topics::Topic,
    tx_event::TxEvent,
    txs::{
        Address, CreateCommitment, CreateScores, JobRunRequest, JobVerification, ScoreEntry,
        SeedUpdate, TrustUpdate, Tx, TxHash, TxKind,
    },
    MyBehaviourEvent,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{cmp::Ordering, error::Error, sync::Arc};
use tokio::{
    select,
    sync::mpsc::{self, Sender},
};
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Whitelist {
    pub users: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub whitelist: Whitelist,
}

pub struct Sequencer {
    sender: Sender<(Vec<u8>, Topic)>,
    whitelisted_users: Vec<Address>,
}

impl Sequencer {
    pub fn new(sender: Sender<(Vec<u8>, Topic)>, whitelisted_users: Vec<Address>) -> Self {
        Self { sender, whitelisted_users }
    }
}

#[rpc_impl]
impl Sequencer {
    async fn trust_update(&self, tx: Value) -> Result<Value, RPCError> {
        let tx_str = tx.as_str().ok_or(RPCError::ParseError(
            "Failed to parse TX data as string".to_string(),
        ))?;
        let tx_bytes = hex::decode(tx_str).map_err(|e| {
            error!("{}", e);
            RPCError::ParseError("Failed to parse TX data".to_string())
        })?;

        let tx = Tx::decode(&mut tx_bytes.as_slice())
            .map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;
        if tx.kind() != TxKind::TrustUpdate {
            return Err(RPCError::InvalidRequest("Invalid tx kind"));
        }
        let address = tx
            .verify()
            .map_err(|_| RPCError::ParseError("Failed to verify TX Signature".to_string()))?;
        if !self.whitelisted_users.contains(&address) {
            return Err(RPCError::InvalidRequest("Invalid TX signer"));
        }
        let body: TrustUpdate = TrustUpdate::decode(&mut tx.body().as_slice())
            .map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;

        // Build Tx Event
        // TODO: Replace with DA call
        let tx_event = TxEvent::default_with_data(tx_bytes);
        let channel_message = (
            encode(tx_event.clone()),
            Topic::NamespaceTrustUpdate(body.trust_id),
        );
        self.sender.send(channel_message).await.map_err(|_| RPCError::InternalError)?;

        let tx_event_value = serde_json::to_value(tx_event)?;
        Ok(tx_event_value)
    }

    async fn seed_update(&self, tx: Value) -> Result<Value, RPCError> {
        let tx_str = tx.as_str().ok_or(RPCError::ParseError(
            "Failed to parse TX data as string".to_string(),
        ))?;
        let tx_bytes = hex::decode(tx_str).map_err(|e| {
            error!("{}", e);
            RPCError::ParseError("Failed to parse TX data".to_string())
        })?;

        let tx = Tx::decode(&mut tx_bytes.as_slice())
            .map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;
        if tx.kind() != TxKind::SeedUpdate {
            return Err(RPCError::InvalidRequest("Invalid tx kind"));
        }
        let address = tx
            .verify()
            .map_err(|_| RPCError::ParseError("Failed to verify TX Signature".to_string()))?;
        if !self.whitelisted_users.contains(&address) {
            return Err(RPCError::InvalidRequest("Invalid TX signature"));
        }
        let body = SeedUpdate::decode(&mut tx.body().as_slice())
            .map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;

        // Build Tx Event
        // TODO: Replace with DA call
        let tx_event = TxEvent::default_with_data(tx_bytes);
        let channel_message = (
            encode(tx_event.clone()),
            Topic::NamespaceSeedUpdate(body.seed_id),
        );
        self.sender.send(channel_message).await.map_err(|_| RPCError::InternalError)?;

        let tx_event_value = serde_json::to_value(tx_event)?;
        Ok(tx_event_value)
    }

    async fn job_run_request(&self, tx: Value) -> Result<Value, RPCError> {
        let tx_str = tx.as_str().ok_or(RPCError::ParseError(
            "Failed to parse TX data as string".to_string(),
        ))?;
        let tx_bytes = hex::decode(tx_str).map_err(|e| {
            error!("{}", e);
            RPCError::ParseError("Failed to parse TX data".to_string())
        })?;

        let tx = Tx::decode(&mut tx_bytes.as_slice()).map_err(|e| {
            error!("{}", e);
            RPCError::ParseError("Failed to parse TX data".to_string())
        })?;
        if tx.kind() != TxKind::JobRunRequest {
            return Err(RPCError::InvalidRequest("Invalid tx kind"));
        }
        let address = tx
            .verify()
            .map_err(|_| RPCError::ParseError("Failed to verify TX Signature".to_string()))?;
        if !self.whitelisted_users.contains(&address) {
            return Err(RPCError::InvalidRequest("Invalid tx signature"));
        }
        let body = JobRunRequest::decode(&mut tx.body().as_slice())
            .map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;

        // Build Tx Event
        // TODO: Replace with DA call
        let tx_event = TxEvent::default_with_data(tx_bytes);
        let channel_message = (
            encode(tx_event.clone()),
            Topic::DomainRequest(body.domain_id),
        );
        self.sender.send(channel_message).await.map_err(|_| RPCError::InternalError)?;

        let tx_event_value = serde_json::to_value(tx_event)?;
        Ok(tx_event_value)
    }

    async fn get_results(&self, job_run_tx_hash: Value) -> Result<Value, RPCError> {
        let tx_hash_str = job_run_tx_hash.as_str().ok_or(RPCError::ParseError(
            "Failed to parse TX hash data as string".to_string(),
        ))?;
        let tx_hash_bytes = hex::decode(tx_hash_str).map_err(|e| {
            error!("{}", e);
            RPCError::ParseError("Failed to parse TX data".to_string())
        })?;
        let tx_hash = TxHash::from_bytes(tx_hash_bytes);
        let db = Db::new_read_only("./local-storage", &[&Tx::get_cf(), &JobResult::get_cf()])
            .map_err(|e| {
                error!("{}", e);
                RPCError::InternalError
            })?;

        let key = JobResult::construct_full_key(tx_hash);
        let result = db.get::<JobResult>(key).map_err(|e| {
            error!("{}", e);
            RPCError::InternalError
        })?;
        let key =
            Tx::construct_full_key(TxKind::CreateCommitment, result.create_commitment_tx_hash);
        let tx = db.get::<Tx>(key).map_err(|e| {
            error!("{}", e);
            RPCError::InternalError
        })?;
        let commitment = CreateCommitment::decode(&mut tx.body().as_slice()).map_err(|e| {
            error!("{}", e);
            RPCError::InternalError
        })?;
        let create_scores_tx: Vec<Tx> = {
            let mut create_scores_tx = Vec::new();
            for tx_hash in commitment.scores_tx_hashes.into_iter() {
                let key = Tx::construct_full_key(TxKind::CreateScores, tx_hash);
                let tx = db.get::<Tx>(key).map_err(|e| {
                    error!("{}", e);
                    RPCError::InternalError
                })?;
                create_scores_tx.push(tx);
            }
            create_scores_tx
        };
        let create_scores: Vec<CreateScores> = {
            let mut create_scores = Vec::new();
            for tx in create_scores_tx.into_iter() {
                create_scores.push(CreateScores::decode(&mut tx.body().as_slice()).map_err(
                    |e| {
                        error!("{}", e);
                        RPCError::InternalError
                    },
                )?);
            }
            create_scores
        };
        let mut score_entries: Vec<ScoreEntry> =
            create_scores.into_iter().map(|x| x.entries).flatten().collect();
        score_entries.sort_by(|a, b| match a.value.partial_cmp(&b.value) {
            Some(ordering) => ordering,
            None => {
                if a.value.is_nan() && b.value.is_nan() {
                    Ordering::Equal
                } else if a.value.is_nan() {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            },
        });

        let verificarion_results_tx: Vec<Tx> = {
            let mut verification_resutls_tx = Vec::new();
            for tx_hash in result.job_verification_tx_hashes.iter() {
                let key = Tx::construct_full_key(TxKind::JobVerification, tx_hash.clone());
                let tx = db.get::<Tx>(key).map_err(|e| {
                    error!("{}", e);
                    RPCError::InternalError
                })?;
                verification_resutls_tx.push(tx);
            }
            verification_resutls_tx
        };
        let verification_results: Vec<JobVerification> = {
            let mut verification_results = Vec::new();
            for tx in verificarion_results_tx.into_iter() {
                let result = JobVerification::decode(&mut tx.body().as_slice()).map_err(|e| {
                    error!("{}", e);
                    RPCError::InternalError
                })?;
                verification_results.push(result);
            }
            verification_results
        };
        let verification_results_bools: Vec<bool> =
            verification_results.into_iter().map(|x| x.verification_result).collect();

        let result =
            serde_json::to_value((verification_results_bools, score_entries)).map_err(|e| {
                error!("{}", e);
                RPCError::InternalError
            })?;
        Ok(result)
    }
}

pub async fn run() -> Result<(), Box<dyn Error>> {
    let config: Config = toml::from_str(include_str!("../config.toml"))?;
    let mut swarm = build_node().await?;
    info!("PEER_ID: {:?}", swarm.local_peer_id());

    // Listen on all interfaces and whatever port the OS assigns
    swarm.listen_on("/ip4/0.0.0.0/udp/8000/quic-v1".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/8000".parse()?)?;

    let (sender, mut receiver) = mpsc::channel(100);
    let sequencer = Arc::new(Sequencer::new(sender.clone(), config.whitelist.users));
    let server = Server::builder("tcp://127.0.0.1:60000")?.service(sequencer).build().await?;
    server.start();

    // Kick it off
    loop {
        select! {
            sibling = receiver.recv() => {
                if let Some((data, topic)) = sibling {
                    let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                    info!("PUBLISH: {:?}", topic.clone());
                    if let Err(e) =
                       swarm.behaviour_mut().gossipsub.publish(topic_wrapper, data)
                    {
                       error!("Publish error: {e:?}");
                    }
                }
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, _multiaddr) in list {
                        info!("mDNS discovered a new peer: {peer_id}");
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _multiaddr) in list {
                        info!("mDNS discover peer has expired: {peer_id}");
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
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
