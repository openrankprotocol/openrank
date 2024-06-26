use alloy_rlp::{encode, Decodable};
use futures::StreamExt;
use libp2p::{core::ConnectedPoint, gossipsub, identify, swarm::SwarmEvent, Multiaddr, Swarm};
use openrank_common::{
	broadcast_event, build_node,
	topics::{Domain, Topic},
	txs::{
		CreateCommitment, CreateScores, FinalisedBlock, JobRunAssignment, JobVerification,
		ProposedBlock, Tx, TxKind,
	},
	MyBehaviour, MyBehaviourEvent,
};
use openrank_common::{tx_event::TxEvent, txs::Address};
use std::error::Error;
use tokio::select;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

fn handle_gossipsub_events(
	mut swarm: &mut Swarm<MyBehaviour>, event: gossipsub::Event, topics: Vec<&Topic>,
) {
	match event {
		gossipsub::Event::Message { propagation_source: peer_id, message_id: id, message } => {
			for topic in topics {
				match topic {
					Topic::DomainAssignent(_) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							let job_run_assignment =
								JobRunAssignment::decode(&mut tx.body().as_slice()).unwrap();
							info!(
								"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
								message.topic.as_str(),
								job_run_assignment,
							);
						}
					},
					Topic::DomainScores(_) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							let create_scores =
								CreateScores::decode(&mut tx.body().as_slice()).unwrap();
							info!(
								"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
								message.topic.as_str(),
								create_scores,
							);
						}
					},
					Topic::DomainCommitment(domain_id) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							let create_commitment =
								CreateCommitment::decode(&mut tx.body().as_slice()).unwrap();
							info!(
								"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
								message.topic.as_str(),
								create_commitment,
							);
							let new_topic = Topic::DomainVerification(domain_id.clone());
							let tx_bytes = encode(JobVerification::default());
							if let Err(e) = broadcast_event(
								&mut swarm,
								TxKind::JobVerification,
								tx_bytes,
								&new_topic,
							) {
								error!("Publish error: {e:?}");
							}
						}
					},
					Topic::ProposedBlock => {
						let topic_wrapper =
							gossipsub::IdentTopic::new(Topic::ProposedBlock.to_hash().to_hex());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							let proposed_block =
								ProposedBlock::decode(&mut tx.body().as_slice()).unwrap();
							info!(
								"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
								message.topic.as_str(),
								proposed_block,
							);
						}
					},
					Topic::FinalisedBlock => {
						let topic_wrapper =
							gossipsub::IdentTopic::new(Topic::FinalisedBlock.to_hash().to_hex());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							let finalised_block =
								FinalisedBlock::decode(&mut tx.body().as_slice()).unwrap();
							info!(
								"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
								message.topic.as_str(),
								finalised_block,
							);
						}
					},
					_ => {},
				}
			}
		},
		_ => {},
	}
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
				SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(event)) => {
					let iter_chain = topics_assignment
						.iter()
						.chain(&topics_scores)
						.chain(&topics_commitment)
						.chain(&[Topic::ProposedBlock])
						.chain(&[Topic::FinalisedBlock]);
					handle_gossipsub_events(&mut swarm, event, iter_chain.collect());
				},
				SwarmEvent::NewListenAddr { address, .. } => {
					info!("Local node is listening on {address}");
				}
				e => info!("{:?}", e),
			}
		}
	}
}
