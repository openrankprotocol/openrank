use futures::StreamExt;
use libp2p::{
	gossipsub::{self, MessageId, PublishError},
	mdns, noise,
	swarm::{NetworkBehaviour, SwarmEvent},
	tcp, yamux, Swarm,
};
use openrank_common::{
	topics::{Domain, Topic},
	tx_event::TxEvent,
	txs::{
		Address, CreateCommitment, FinalisedBlock, JobRunAssignment, JobRunRequest,
		JobVerification, ProposedBlock, Tx, TxKind,
	},
};
use std::{error::Error, time::Duration};
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

			let mdns =
				mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;
			Ok(MyBehaviour { gossipsub, mdns })
		})?
		.with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
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
	let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

	let mut swarm = build_node().await?;
	println!("PEER_ID: {:?}", swarm.local_peer_id());

	let domains = vec![Domain::new(
		Address::default(),
		"1".to_string(),
		Address::default(),
		"1".to_string(),
		0,
	)];
	let topics_requests: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainRequest(domain_hash.clone()))
		.collect();
	let topics_commitment: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainCommitment(domain_hash.clone()))
		.collect();
	let topics_verification: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainVerification(domain_hash.clone()))
		.collect();

	for topic in topics_verification.iter().chain(&topics_commitment).chain(&topics_requests) {
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
						// println!("mDNS discovered a new peer: {peer_id}");
						swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
					}
				},
				SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
					for (peer_id, _multiaddr) in list {
						// println!("mDNS discover peer has expired: {peer_id}");
						swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
					}
				},
				SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
					propagation_source: peer_id,
					message_id: id,
					message,
				})) => {
					let iter_chain = topics_requests
						.iter()
						.chain(&topics_commitment)
						.chain(&topics_verification);
					for topic in iter_chain {
						match topic {
							Topic::DomainRequest(domain_id) => {
								let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
									let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = Tx::from_bytes(tx_event.data());
									assert!(tx.kind() == TxKind::JobRunRequest);
									let job_run_request = JobRunRequest::from_bytes(tx.body());
									println!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										job_run_request,
									);

									let assignment_topic = Topic::DomainAssignent(domain_id.clone());
									let job_assignment = JobRunAssignment::default().to_bytes();
									if let Err(e) = broadcast_event(
										&mut swarm,
										TxKind::JobRunAssignment,
										job_assignment,
										&assignment_topic,
									) {
										println!("Publish error: {e:?}");
									}
								}
							},
							Topic::DomainCommitment(_) => {
								let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
									let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = Tx::from_bytes(tx_event.data());
									let commitment = CreateCommitment::from_bytes(tx.body());
									println!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										commitment,
									);

									let proposed_block_topic = Topic::ProposedBlock;
									let proposed_block = ProposedBlock::default().to_bytes();
									if let Err(e) = broadcast_event(
										&mut swarm,
										TxKind::ProposedBlock,
										proposed_block,
										&proposed_block_topic,
									) {
										println!("Publish error: {e:?}");
									}
								}
							},
							Topic::DomainVerification(_) => {
								let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
								if message.topic == topic_wrapper.hash() {
									let tx_event = TxEvent::from_bytes(message.data.clone());
									let tx = Tx::from_bytes(tx_event.data());
									let job_verification = JobVerification::from_bytes(tx.body());
									println!(
										"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
										message.topic.as_str(),
										job_verification,
									);

									let finalised_block_topic = Topic::FinalisedBlock;
									let finalised_block = FinalisedBlock::default().to_bytes();
									if let Err(e) = broadcast_event(
										&mut swarm,
										TxKind::FinalisedBlock,
										finalised_block,
										&finalised_block_topic,
									) {
										println!("Publish error: {e:?}");
									}
								}
							},
							_ => {},
						}
					}
				},
				SwarmEvent::NewListenAddr { address, .. } => {
					println!("Local node is listening on {address}");
				},
				_ => {}
			}
		}
	}
}
