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
		CreateCommitment, CreateScores, JobRunRequest, JobVerification, ScoreEntry, SeedUpdate,
		TrustUpdate, Tx, TxHash, TxKind,
	},
	MyBehaviourEvent,
};
use serde_json::Value;
use std::{error::Error, sync::Arc};
use tokio::{
	select,
	sync::mpsc::{self, Sender},
};
use tracing::{error, info};

pub struct Sequencer {
	sender: Sender<(Vec<u8>, Topic)>,
}

impl Sequencer {
	pub fn new(sender: Sender<(Vec<u8>, Topic)>) -> Self {
		Self { sender }
	}
}

#[rpc_impl]
impl Sequencer {
	async fn trust_update(&self, tx: Value) -> Result<Value, RPCError> {
		let tx_bytes = hex::decode(tx.as_str().unwrap()).map_err(|e| {
			error!("{}", e);
			RPCError::ParseError("Failed to parse TX data".to_string())
		})?;

		let tx = Tx::decode(&mut tx_bytes.as_slice())
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;
		if tx.kind() != TxKind::TrustUpdate {
			return Err(RPCError::InvalidRequest("Invalid tx kind"));
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
		let tx_bytes = hex::decode(tx.as_str().unwrap())
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;

		let tx = Tx::decode(&mut tx_bytes.as_slice())
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;
		if tx.kind() != TxKind::SeedUpdate {
			return Err(RPCError::InvalidRequest("Invalid tx kind"));
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
		let tx_bytes = hex::decode(tx.as_str().unwrap()).map_err(|e| {
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
		let tx_hash_bytes = hex::decode(job_run_tx_hash.as_str().unwrap()).map_err(|e| {
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
		let result = db.get::<JobResult>(key).unwrap();
		let key =
			Tx::construct_full_key(TxKind::CreateCommitment, result.create_commitment_tx_hash);
		let tx = db.get::<Tx>(key).unwrap();
		let commitment = CreateCommitment::decode(&mut tx.body().as_slice()).unwrap();
		let create_scores_tx: Vec<Tx> = commitment
			.scores_tx_hashes
			.into_iter()
			.map(|x| {
				let key = Tx::construct_full_key(TxKind::CreateScores, x);
				db.get::<Tx>(key).unwrap()
			})
			.collect();
		let create_scores: Vec<CreateScores> = create_scores_tx
			.into_iter()
			.map(|tx| CreateScores::decode(&mut tx.body().as_slice()).unwrap())
			.collect();
		let mut score_entries: Vec<ScoreEntry> =
			create_scores.into_iter().map(|x| x.entries).flatten().collect();
		score_entries.sort_by(|a, b| a.value.partial_cmp(&b.value).unwrap());

		let verificarion_results_tx: Vec<Tx> = result
			.job_verification_tx_hashes
			.iter()
			.map(|x| {
				let key = Tx::construct_full_key(TxKind::JobVerification, x.clone());
				db.get::<Tx>(key).unwrap()
			})
			.collect();
		let verification_results: Vec<JobVerification> = verificarion_results_tx
			.into_iter()
			.map(|tx| JobVerification::decode(&mut tx.body().as_slice()).unwrap())
			.collect();
		let verification_results_bools: Vec<bool> =
			verification_results.into_iter().map(|x| x.verification_result).collect();

		Ok(serde_json::to_value((verification_results_bools, score_entries)).unwrap())
	}
}

pub async fn run() -> Result<(), Box<dyn Error>> {
	let mut swarm = build_node().await?;
	info!("PEER_ID: {:?}", swarm.local_peer_id());

	// Listen on all interfaces and whatever port the OS assigns
	swarm.listen_on("/ip4/0.0.0.0/udp/8000/quic-v1".parse()?)?;
	swarm.listen_on("/ip4/0.0.0.0/tcp/8000".parse()?)?;

	let (sender, mut receiver) = mpsc::channel(100);
	let sequencer = Arc::new(Sequencer::new(sender.clone()));
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
