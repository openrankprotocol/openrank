use alloy_rlp::{encode, Decodable};
use futures::StreamExt;
use karyon_jsonrpc::{rpc_impl, RPCError, Server};
use libp2p::{gossipsub, mdns, swarm::SwarmEvent};
use openrank_common::{
	build_node,
	topics::{DomainHash, Topic},
	tx_event::TxEvent,
	txs::{Tx, TxKind},
	MyBehaviourEvent,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{error::Error, sync::Arc};
use tokio::{
	select,
	sync::mpsc::{self, Sender},
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DomainRequest {
	domain_id: u64,
	tx_data: String,
}

impl DomainRequest {
	pub fn new(domain_id: u64, tx_data: String) -> Self {
		Self { domain_id, tx_data }
	}
}

struct Sequencer {
	sender: Sender<(Vec<u8>, Topic)>,
}

impl Sequencer {
	fn new(sender: Sender<(Vec<u8>, Topic)>) -> Self {
		Self { sender }
	}
}

#[rpc_impl]
impl Sequencer {
	async fn trust_update(&self, tx: Value) -> Result<Value, RPCError> {
		let request: DomainRequest = serde_json::from_value(tx)?;
		let tx_data_decoded = hex::decode(request.tx_data)
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;

		// Build Tx Event
		// TODO: Replace with DA call
		let tx_event = TxEvent::default_with_data(tx_data_decoded.clone());

		let tx = Tx::decode(&mut tx_data_decoded.as_slice())
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;
		if tx.kind() != TxKind::TrustUpdate {
			return Err(RPCError::InvalidRequest("Invalid tx kind"));
		}
		let channel_message = (
			encode(tx_event.clone()),
			Topic::DomainTrustUpdate(DomainHash::from(request.domain_id)),
		);
		self.sender.send(channel_message).await.map_err(|_| RPCError::InternalError)?;
		let tx_event_value = serde_json::to_value(tx_event)?;
		Ok(tx_event_value)
	}

	async fn seed_update(&self, tx: Value) -> Result<Value, RPCError> {
		let request: DomainRequest = serde_json::from_value(tx)?;
		let tx_data_decoded = hex::decode(request.tx_data)
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;

		// Build Tx Event
		// TODO: Replace with DA call
		let tx_event = TxEvent::default_with_data(tx_data_decoded.clone());

		let tx = Tx::decode(&mut tx_data_decoded.as_slice())
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;
		if tx.kind() != TxKind::SeedUpdate {
			return Err(RPCError::InvalidRequest("Invalid tx kind"));
		}
		let channel_message = (
			encode(tx_event.clone()),
			Topic::DomainSeedUpdate(DomainHash::from(request.domain_id)),
		);
		self.sender.send(channel_message).await.map_err(|_| RPCError::InternalError)?;
		let tx_event_value = serde_json::to_value(tx_event)?;
		Ok(tx_event_value)
	}

	async fn job_run_request(&self, tx: Value) -> Result<Value, RPCError> {
		let request: DomainRequest = serde_json::from_value(tx)?;
		let tx_data_decoded = hex::decode(request.tx_data)
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;

		// Build Tx Event
		// TODO: Replace with DA call
		let tx_event = TxEvent::default_with_data(tx_data_decoded.clone());

		let tx = Tx::decode(&mut tx_data_decoded.as_slice())
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;
		if tx.kind() != TxKind::JobRunRequest {
			return Err(RPCError::InvalidRequest("Invalid tx kind"));
		}
		let channel_message = (
			encode(tx_event.clone()),
			Topic::DomainRequest(DomainHash::from(request.domain_id)),
		);
		self.sender.send(channel_message).await.map_err(|_| RPCError::InternalError)?;
		let tx_event_value = serde_json::to_value(tx_event)?;
		Ok(tx_event_value)
	}
}

pub async fn run() -> Result<(), Box<dyn Error>> {
	tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();

	let mut swarm = build_node().await?;
	info!("PEER_ID: {:?}", swarm.local_peer_id());

	// Listen on all interfaces and whatever port the OS assigns
	swarm.listen_on("/ip4/0.0.0.0/udp/8000/quic-v1".parse()?)?;
	swarm.listen_on("/ip4/0.0.0.0/tcp/8000".parse()?)?;

	info!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");

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
