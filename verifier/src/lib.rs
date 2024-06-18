use futures::StreamExt;
use libp2p::{
	gossipsub::{self, MessageId, PublishError},
	mdns, noise,
	swarm::{NetworkBehaviour, SwarmEvent},
	tcp, yamux, Swarm,
};
use openrank_common::{
	topics::{Domain, Topic},
	txs::{
		CreateCommitment, CreateScores, FinalisedBlock, JobRunAssignment, JobVerification,
		ProposedBlock,
	},
};
use openrank_common::{txs::Address, TxEvent};
use std::{
	error::Error,
	hash::{DefaultHasher, Hash, Hasher},
	time::Duration,
};
use tokio::{
	io::{self},
	select,
};
use tracing_subscriber::EnvFilter;

// We create a custom network behaviour that combines Gossipsub and Mdns.
#[derive(NetworkBehaviour)]
pub struct MyBehaviour {
	gossipsub: gossipsub::Behaviour,
	mdns: mdns::tokio::Behaviour,
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
			// To content-address message, we can take the hash of message and use it as an ID.
			let message_id_fn = |message: &gossipsub::Message| {
				let mut s = DefaultHasher::new();
				message.data.hash(&mut s);
				gossipsub::MessageId::from(s.finish().to_string())
			};

			// Set a custom gossipsub configuration
			let gossipsub_config = gossipsub::ConfigBuilder::default()
				.heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
				.validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
				.message_id_fn(message_id_fn) // content-address messages. No two messages of the same content will be propagated.
				.build()
				.map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?; // Temporary hack because `build` does not return a proper `std::error::Error`.

			// build a gossipsub network behaviour
			let gossipsub = gossipsub::Behaviour::new(
				gossipsub::MessageAuthenticity::Signed(key.clone()),
				gossipsub_config,
			)?;

			let mdns =
				mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;
			Ok(MyBehaviour { gossipsub, mdns })
		})?
		.with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
		.build();

	Ok(swarm)
}

pub fn broadcast_event(
	swarm: &mut Swarm<MyBehaviour>, data: Vec<u8>, topic: &Topic,
) -> Result<MessageId, PublishError> {
	let default_tx = TxEvent::default_with_data(data);
	let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
	swarm.behaviour_mut().gossipsub.publish(topic_wrapper, default_tx.to_bytes())
}

pub async fn run() -> Result<(), Box<dyn Error>> {
	let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

	let mut swarm = build_node().await?;

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

	for topic in
		topics_assignment.iter().chain(topics_scores.iter()).chain(topics_commitment.iter())
	{
		// Create a Gossipsub topic
		let topic = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
		// subscribes to our topic
		swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
	}

	// Listen on all interfaces and whatever port the OS assigns
	swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
	swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

	// Kick it off
	loop {
		select! {
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
				SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
					propagation_source: peer_id,
					message_id: id,
					message,
				})) => {
					for topic in topics_assignment.iter().chain(&topics_scores).chain(&topics_commitment) {
						match topic {
							Topic::DomainAssignent(_) => {
								let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
								let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = JobRunAssignment::from_bytes(tx_event.data());
									println!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										tx,
									);
								}
							}
							Topic::DomainScores(_) => {
								let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
									let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = CreateScores::from_bytes(tx_event.data());
									println!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										tx,
									);
								}
							}
							Topic::DomainCommitment(domain_id) => {
								let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
									let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = CreateCommitment::from_bytes(tx_event.data());
									println!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										tx,
									);
									let new_topic = Topic::DomainVerification(domain_id.clone());
									if let Err(e) = broadcast_event(&mut swarm, JobVerification::default().to_bytes(), &new_topic) {
										println!("Publish error: {e:?}");
									}
								}
							}
							Topic::ProposedBlock => {
								let topic_wrapper = gossipsub::IdentTopic::new(Topic::ProposedBlock.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
									let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = ProposedBlock::from_bytes(tx_event.data());
									println!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										tx,
									);
								}
							},
							Topic::FinalisedBlock => {
								let topic_wrapper = gossipsub::IdentTopic::new(Topic::FinalisedBlock.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
									let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = FinalisedBlock::from_bytes(tx_event.data());
									println!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										tx,
									);
								}
							},
							_ => {}
						}
					}
				},
				SwarmEvent::NewListenAddr { address, .. } => {
					println!("Local node is listening on {address}");
				}
				_ => {}
			}
		}
	}
}
