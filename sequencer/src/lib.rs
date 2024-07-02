use alloy_rlp::{encode, Decodable};
use futures::StreamExt;
use karyon_jsonrpc::{rpc_impl, RPCError, Server};
use libp2p::{gossipsub, mdns, swarm::SwarmEvent};
use openrank_common::{
	build_node,
	topics::{Domain, DomainHash, Topic},
	tx_event::TxEvent,
	txs::{Address, JobRunRequest, SeedUpdate, TrustUpdate, Tx, TxKind},
	MyBehaviourEvent,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{error::Error, sync::Arc};
use tokio::{
	io::{self, AsyncBufReadExt},
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
			Topic::DomainRequest(DomainHash::from(request.domain_id)),
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
			Topic::DomainSeedUpdate(DomainHash::from(request.domain_id)),
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
	let server = Server::builder("tcp://127.0.0.1:60000")
		.expect("create new server builder")
		.service(sequencer)
		.build()
		.await?;
	server.start();

	// Read full lines from stdin
	let mut stdin = io::BufReader::new(io::stdin()).lines();

	let domains = vec![Domain::new(
		Address::default(),
		"1".to_string(),
		Address::default(),
		"1".to_string(),
		0,
	)];
	let topics_request: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainRequest(domain_hash.clone()))
		.collect();
	let topics_trust_update: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainTrustUpdate(domain_hash.clone()))
		.collect();
	let topics_seed_update: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainSeedUpdate(domain_hash.clone()))
		.collect();

	// Kick it off
	loop {
		select! {
			sibling = receiver.recv() => {
				let (data, topic) = sibling.unwrap();
				let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
				if let Err(e) =
				   swarm.behaviour_mut().gossipsub.publish(topic_wrapper, data)
				{
				   error!("Publish error: {e:?}");
				}
			}
			inp = stdin.next_line() => {
				match inp {
					Ok(Some(line)) => {
						match line.as_str() {
							"request" => {
								for topic in &topics_request {
									let default_body = JobRunRequest::default();
									let tx = Tx::default_with(TxKind::JobRunRequest, encode(default_body.clone()));
									let default_tx = TxEvent::default_with_data(encode(tx));
									let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
									if let Err(e) = swarm
										.behaviour_mut().gossipsub
										.publish(topic_wrapper, encode(default_tx)) {
										error!("Publish error: {e:?}");
									}
									info!(
										"PUBLISH: {:?}, TX: '{:?}'",
										topic,
										default_body,
									);
								}
							},
							"trust_update" => {
								for topic in &topics_trust_update {
									let default_body = TrustUpdate::default();
									let tx = Tx::default_with(TxKind::TrustUpdate, encode(default_body.clone()));
									let default_tx = TxEvent::default_with_data(encode(tx));
									let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
									if let Err(e) = swarm
										.behaviour_mut().gossipsub
										.publish(topic_wrapper, encode(default_tx)) {
										error!("Publish error: {e:?}");
									}
									info!(
										"PUBLISH: {:?}, TX: '{:?}'",
										topic,
										default_body,
									);
								}
							}
							"seed_update" => {
								for topic in &topics_seed_update {
									let default_body = SeedUpdate::default();
									let tx = Tx::default_with(TxKind::SeedUpdate, encode(default_body.clone()));
									let default_tx = TxEvent::default_with_data(encode(tx));
									let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
									if let Err(e) = swarm
										.behaviour_mut().gossipsub
										.publish(topic_wrapper, encode(default_tx)) {
										error!("Publish error: {e:?}");
									}
									info!(
										"PUBLISH: {:?}, TX: '{:?}'",
										topic,
										default_body,
									);
								}
							}
							s => info!("stdin: {:?}", s),
						}
					},
					Err(e) => info!("stdin: {:?}", e),
					_ => {}
				}
			}
			event = swarm.select_next_some() => match event {
				SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
					for (peer_id, _multiaddr) in list {
						println!("mDNS discovered a new peer: {peer_id}");
						swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
					}
				},
				SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
					for (peer_id, _multiaddr) in list {
						println!("mDNS discover peer has expired: {peer_id}");
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
