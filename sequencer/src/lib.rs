use alloy_rlp::{encode, Decodable};
use futures::StreamExt;
use karyon_jsonrpc::{rpc_impl, RPCError, Server};
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
    build_node, config,
    db::{self, Db, DbItem},
    net,
    result::GetResultsQuery,
    topics::Topic,
    tx::{self, compute, trust::ScoreEntry, Address, Tx},
    tx_event::TxEvent,
    MyBehaviour, MyBehaviourEvent,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{cmp::Ordering, error::Error, sync::Arc};
use tokio::{
    select,
    sync::mpsc::{self, Receiver, Sender},
};
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The whitelist for the Sequencer.
pub struct Whitelist {
    /// The list of addresses that are allowed to call the Sequencer.
    pub users: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The configuration for the Sequencer.
pub struct Config {
    /// The whitelist for the Sequencer.
    pub whitelist: Whitelist,
    pub database: db::Config,
    pub p2p: net::Config,
}

/// The Sequencer node. It contains the sender, the whitelisted users, and the database connection.
pub struct Sequencer {
    sender: Sender<(Vec<u8>, Topic)>,
    whitelisted_users: Vec<Address>,
    db: Db,
}

impl Sequencer {
    pub fn new(sender: Sender<(Vec<u8>, Topic)>, whitelisted_users: Vec<Address>, db: Db) -> Self {
        Self { sender, whitelisted_users, db }
    }
}

#[rpc_impl]
impl Sequencer {
    /// Handles incoming `TrustUpdate` transactions from the network,
    /// and forward them to the network for processing.
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
        if let tx::Body::TrustUpdate(body) = tx.body() {
            let address = tx
                .verify()
                .map_err(|_| RPCError::ParseError("Failed to verify TX Signature".to_string()))?;
            if !self.whitelisted_users.contains(&address) {
                return Err(RPCError::InvalidRequest("Invalid TX signer"));
            }
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
        } else {
            return Err(RPCError::InvalidRequest("Invalid tx kind"));
        }
    }

    /// Handles incoming `SeedUpdate` transactions from the network,
    /// and forward them to the network node for processing.
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
        if let tx::Body::SeedUpdate(body) = tx.body() {
            let address = tx
                .verify()
                .map_err(|_| RPCError::ParseError("Failed to verify TX Signature".to_string()))?;
            if !self.whitelisted_users.contains(&address) {
                return Err(RPCError::InvalidRequest("Invalid TX signature"));
            }

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
        } else {
            return Err(RPCError::InvalidRequest("Invalid tx kind"));
        }
    }

    /// Handles incoming `ComputeRequest` transactions from the network,
    /// and forward them to the network node for processing
    async fn compute_request(&self, tx: Value) -> Result<Value, RPCError> {
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
        if let tx::Body::ComputeRequest(body) = tx.body() {
            let address = tx
                .verify()
                .map_err(|_| RPCError::ParseError("Failed to verify TX Signature".to_string()))?;
            if !self.whitelisted_users.contains(&address) {
                return Err(RPCError::InvalidRequest("Invalid tx signature"));
            }

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
        } else {
            return Err(RPCError::InvalidRequest("Invalid tx kind"));
        }
    }

    /// Gets the results(EigenTrust scores) of the `ComputeRequest` with the ComputeRequest TX hash,
    /// along with start and size parameters.
    async fn get_results(&self, get_results_query: Value) -> Result<Value, RPCError> {
        self.db.refresh().map_err(|e| {
            error!("{}", e);
            RPCError::InternalError
        })?;
        let query: GetResultsQuery = serde_json::from_value(get_results_query)
            .map_err(|e| RPCError::ParseError(e.to_string()))?;

        let key = compute::Result::construct_full_key(query.compute_request_tx_hash);
        let result = self.db.get::<compute::Result>(key).map_err(|e| {
            error!("{}", e);
            RPCError::InternalError
        })?;
        let key = Tx::construct_full_key("compute_commitment", result.compute_commitment_tx_hash);
        let tx = self.db.get::<Tx>(key).map_err(|e| {
            error!("{}", e);
            RPCError::InternalError
        })?;
        let commitment = match tx.body() {
            tx::Body::ComputeCommitment(commitment) => commitment,
            _ => return Err(RPCError::InternalError),
        };
        let compute_scores_tx: Vec<Tx> = {
            let mut compute_scores_tx = Vec::new();
            for tx_hash in commitment.scores_tx_hashes.into_iter() {
                let key = Tx::construct_full_key("compute_scores", tx_hash);
                let tx = self.db.get::<Tx>(key).map_err(|e| {
                    error!("{}", e);
                    RPCError::InternalError
                })?;
                compute_scores_tx.push(tx);
            }
            compute_scores_tx
        };
        let compute_scores: Vec<compute::Scores> = {
            let mut compute_scores = Vec::new();
            for tx in compute_scores_tx.into_iter() {
                match tx.body() {
                    tx::Body::ComputeScores(scores) => {
                        compute_scores.push(scores);
                    },
                    _ => {
                        return Err(RPCError::InternalError);
                    },
                }
            }
            compute_scores
        };
        let mut score_entries: Vec<ScoreEntry> =
            compute_scores.into_iter().flat_map(|x| x.entries).collect();
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
        score_entries.reverse();
        let score_entries: Vec<ScoreEntry> = score_entries
            .split_at(query.start as usize)
            .1
            .iter()
            .take(query.size as usize)
            .cloned()
            .collect();

        let verificarion_results_tx: Vec<Tx> = {
            let mut verification_resutls_tx = Vec::new();
            for tx_hash in result.compute_verification_tx_hashes.iter() {
                let key = Tx::construct_full_key("compute_verification", tx_hash.clone());
                let tx = self.db.get::<Tx>(key).map_err(|e| {
                    error!("{}", e);
                    RPCError::InternalError
                })?;
                verification_resutls_tx.push(tx);
            }
            verification_resutls_tx
        };
        let verification_results: Vec<compute::Verification> = {
            let mut verification_results = Vec::new();
            for tx in verificarion_results_tx.into_iter() {
                match tx.body() {
                    tx::Body::ComputeVerification(result) => {
                        verification_results.push(result);
                    },
                    _ => return Err(RPCError::InternalError),
                };
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

/// The Sequencer node. It contains the Swarm, the Server, and the Receiver.
pub struct Node {
    config: Config,
    swarm: Swarm<MyBehaviour>,
    server: Arc<Server>,
    receiver: Receiver<(Vec<u8>, Topic)>,
}

impl Node {
    /// Initialize the node:
    /// - Load the config from config.toml
    /// - Initialize the Swarm
    /// - Initialize the DB
    /// - Initialize the Sequencer JsonRPC server
    pub async fn init() -> Result<Self, Box<dyn Error>> {
        let config_loader = config::Loader::new("openrank-sequencer")?;
        let config: Config = config_loader.load_or_create(include_str!("../config.toml"))?;
        let db = Db::new_secondary(
            &config.database,
            &[&Tx::get_cf(), &compute::Result::get_cf()],
        )?;
        let (sender, receiver) = mpsc::channel(100);
        let sequencer = Arc::new(Sequencer::new(
            sender.clone(),
            config.whitelist.users.clone(),
            db,
        ));
        let server = Server::builder("tcp://127.0.0.1:60000")?.service(sequencer).build().await?;

        let swarm = build_node(net::load_keypair(&config.p2p.keypair, &config_loader)?).await?;
        info!("PEER_ID: {:?}", swarm.local_peer_id());

        Ok(Self { swarm, config, server, receiver })
    }

    /// Run the node:
    /// - Listen on all interfaces and whatever port the OS assigns
    /// - Subscribe to all the topics
    /// - Handle gossipsub events
    /// - Handle mDNS events
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        net::listen_on(&mut self.swarm, &self.config.p2p.listen_on)?;
        self.server.start();

        // Kick it off
        loop {
            select! {
                sibling = self.receiver.recv() => {
                    if let Some((data, topic)) = sibling {
                        let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
                        info!("PUBLISH: {:?}", topic.clone());
                        if let Err(e) =
                           self.swarm.behaviour_mut().gossipsub.publish(topic_wrapper, data)
                        {
                           error!("Publish error: {e:?}");
                        }
                    }
                }
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
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("Local node is listening on {address}");
                    },
                    e => info!("{:?}", e),
                }
            }
        }
    }
}
