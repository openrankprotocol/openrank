use futures::StreamExt;
use libp2p::{
	core::ConnectedPoint,
	gossipsub::{self, MessageId, PublishError},
	identify,
	kad::{self, store::MemoryStore},
	noise,
	swarm::{NetworkBehaviour, SwarmEvent},
	tcp, yamux, Multiaddr, Swarm,
};
use openrank_common::{
	topics::{Domain, Topic},
	txs::{
		CreateCommitment, CreateScores, FinalisedBlock, JobRunAssignment, JobVerification,
		ProposedBlock, Tx, TxKind,
	},
};
use openrank_common::{tx_event::TxEvent, txs::Address};
use std::{error::Error, time::Duration};
use tokio::{
	io::{self},
	select,
};
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

// We create a custom network behaviour.
#[derive(NetworkBehaviour)]
pub struct MyBehaviour {
	gossipsub: gossipsub::Behaviour,
	kademlia: kad::Behaviour<MemoryStore>,
	identify: identify::Behaviour,
}

async fn build_node() -> Result<Swarm<MyBehaviour>, Box<dyn Error>> {
	let swarm = libp2p::SwarmBuilder::with_new_identity()
		.with_tokio()
		.with_tcp(
			tcp::Config::default(),
			noise::Config::new,
			yamux::Config::default,
		)?
		.with_quic()
		.with_behaviour(|key| {
			// Set a custom gossipsub configuration
			let gossipsub_config = gossipsub::ConfigBuilder::default()
				// This is set to aid debugging by not cluttering the log space
				.heartbeat_interval(Duration::from_secs(10))
				// This sets the kind of message validation. The default is Strict (enforce message signing)
				.validation_mode(gossipsub::ValidationMode::Strict)
				.build()
				// Temporary hack because `build` does not return a proper `std::error::Error`.
				.map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?;

			// build a gossipsub network behaviour
			let gossipsub = gossipsub::Behaviour::new(
				gossipsub::MessageAuthenticity::Signed(key.clone()),
				gossipsub_config,
			)?;

			Ok(MyBehaviour {
				gossipsub,
				kademlia: kad::Behaviour::new(
					key.public().to_peer_id(),
					MemoryStore::new(key.public().to_peer_id()),
				),
				identify: identify::Behaviour::new(identify::Config::new(
					"openrank/1.0.0".to_string(),
					key.public(),
				)),
			})
		})?
		.with_swarm_config(|c| c.with_idle_connection_timeout(Duration::MAX))
		.build();

	Ok(swarm)
}

pub fn broadcast_event(
	swarm: &mut Swarm<MyBehaviour>, kind: TxKind, data: Vec<u8>, topic: &Topic,
) -> Result<MessageId, PublishError> {
	let tx = Tx::default_with(kind, data);
	let tx_event = TxEvent::default_with_data(tx.to_bytes());
	let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
	swarm.behaviour_mut().gossipsub.publish(topic_wrapper, tx_event.to_bytes())
}

pub async fn run() -> Result<(), Box<dyn Error>> {
	tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();

	let mut swarm = build_node().await?;
	info!("PEER_ID: {:?}", swarm.local_peer_id());

	let domains = vec![Domain::new(
		Address::default(),
		"1".to_string(),
		Address::default(),
		"1".to_string(),
		0,
	)];
	let topics_assignment: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainAssignent(domain_hash.clone()))
		.collect();
	let topics_scores: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainScores(domain_hash.clone()))
		.collect();
	let topics_commitment: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainCommitment(domain_hash.clone()))
		.collect();

	for topic in topics_assignment
		.iter()
		.chain(topics_scores.iter())
		.chain(topics_commitment.iter())
		.chain(&[Topic::ProposedBlock])
		.chain(&[Topic::FinalisedBlock])
	{
		// Create a Gossipsub topic
		let topic = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
		// subscribes to our topic
		swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
	}

	// Listen on all interfaces and whatever port the OS assigns
	swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
	swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

	// Dial the peer identified by the multi-address given as the second
	// command-line argument, if any.
	let bootstrap_node_addr = if let Some(addr) = std::env::args().nth(1) {
		let remote: Multiaddr = addr.parse()?;
		swarm.dial(remote.clone())?;
		info!("Dialed {addr}");
		Some(remote)
	} else {
		None
	};

	// Kick it off
	loop {
		select! {
			event = swarm.select_next_some() => match event {
				SwarmEvent::NewExternalAddrOfPeer { peer_id, address } => {
					info!("New peer: {:?} {:?}", peer_id, address);
					swarm.behaviour_mut().kademlia.add_address(&peer_id, address);
					swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
				},
				SwarmEvent::ConnectionClosed { peer_id, endpoint: ConnectedPoint::Dialer { address, .. }, ..} => {
					debug!("Connection closed: {:?} {:?}", peer_id, address);
					swarm.behaviour_mut().kademlia.remove_address(&peer_id, &address);
					swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
				},
				SwarmEvent::ConnectionEstablished { peer_id, endpoint: ConnectedPoint::Dialer { address, .. }, .. } => {
					swarm.behaviour_mut().kademlia.add_address(&peer_id, address.clone());
					bootstrap_node_addr.clone().map(|addr| {
						if address == addr {
							let res = swarm.behaviour_mut().kademlia.bootstrap();
							if let Err(err) = res {
								error!("Failed to bootstrap DHT: {:?}", err);
							}
						}
					});
				},
				SwarmEvent::Behaviour(MyBehaviourEvent::Identify(identify::Event::Received { peer_id, info })) => {
					swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
					for addr in &info.listen_addrs {
						swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
					}
				},
				SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
					propagation_source: peer_id,
					message_id: id,
					message,
				})) => {
					let iter_chain = topics_assignment
						.iter()
						.chain(&topics_scores)
						.chain(&topics_commitment)
						.chain(&[Topic::ProposedBlock])
						.chain(&[Topic::FinalisedBlock]);
					for topic in iter_chain {
						match topic {
							Topic::DomainAssignent(_) => {
								let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
									let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = Tx::from_bytes(tx_event.data());
									let job_run_assignment = JobRunAssignment::from_bytes(tx.body());
									info!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										job_run_assignment,
									);
								}
							}
							Topic::DomainScores(_) => {
								let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
									let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = Tx::from_bytes(tx_event.data());
									let create_scores = CreateScores::from_bytes(tx.body());
									info!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										create_scores,
									);
								}
							}
							Topic::DomainCommitment(domain_id) => {
								let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
									let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = Tx::from_bytes(tx_event.data());
									let create_commitment = CreateCommitment::from_bytes(tx.body());
									info!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										create_commitment,
									);
									let new_topic = Topic::DomainVerification(domain_id.clone());
									let tx_bytes = JobVerification::default().to_bytes();
									if let Err(e) = broadcast_event(&mut swarm, TxKind::JobVerification, tx_bytes, &new_topic) {
										error!("Publish error: {e:?}");
									}
								}
							}
							Topic::ProposedBlock => {
								let topic_wrapper = gossipsub::IdentTopic::new(Topic::ProposedBlock.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
									let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = Tx::from_bytes(tx_event.data());
									let proposed_block = ProposedBlock::from_bytes(tx.body());
									info!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										proposed_block,
									);
								}
							},
							Topic::FinalisedBlock => {
								let topic_wrapper = gossipsub::IdentTopic::new(Topic::FinalisedBlock.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
									let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = Tx::from_bytes(tx_event.data());
									let finalised_block = FinalisedBlock::from_bytes(tx.body());
									info!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										finalised_block,
									);
								}
							},
							_ => {}
						}
					}
				},
				SwarmEvent::NewListenAddr { address, .. } => {
					info!("Local node is listening on {address}");
				}
				e => debug!("{:?}", e),
			}
		}
	}
}
