use alloy_rlp::{encode, Decodable};
use futures::StreamExt;
use libp2p::{
	core::ConnectedPoint, gossipsub, identify, kad, kad::RecordKey, swarm::SwarmEvent, Multiaddr,
	Swarm,
};
use openrank_common::{
	broadcast_event, build_node,
	topics::{Domain, Topic},
	tx_event::TxEvent,
	txs::{
		Address, CreateCommitment, CreateScores, FinalisedBlock, JobRunAssignment, ProposedBlock,
		Tx, TxKind,
	},
	MyBehaviour, MyBehaviourEvent,
};
use std::{error::Error, time::Duration};
use tokio::{select, time::interval};
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

fn handle_gossipsub_events(
	mut swarm: &mut Swarm<MyBehaviour>, event: gossipsub::Event, topics: Vec<&Topic>,
) {
	match event {
		gossipsub::Event::Message { propagation_source: peer_id, message_id: id, message } => {
			for topic in topics {
				match topic {
					Topic::DomainAssignent(domain_id) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert!(tx.kind() == TxKind::JobRunAssignment);
							let job_run_assignment =
								JobRunAssignment::decode(&mut tx.body().as_slice()).unwrap();
							info!(
								"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}, SOURCE: {:?}",
								message.topic.as_str(),
								job_run_assignment,
								message.source,
							);

							let create_scores = encode(CreateScores::default());
							for _ in 0..3 {
								let scores_topic = Topic::DomainScores(domain_id.clone());
								if let Err(e) = broadcast_event(
									&mut swarm,
									TxKind::CreateScores,
									create_scores.clone(),
									scores_topic,
								) {
									error!("Publish error: {e:?}");
								}
							}
							let commitment_topic = Topic::DomainCommitment(domain_id.clone());
							let create_commitment = encode(CreateCommitment::default());
							if let Err(e) = broadcast_event(
								&mut swarm,
								TxKind::CreateCommitment,
								create_commitment,
								commitment_topic,
							) {
								error!("Publish error: {e:?}");
							}
						}
					},
					Topic::ProposedBlock => {
						let topic_wrapper = gossipsub::IdentTopic::new(Topic::ProposedBlock);
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert!(tx.kind() == TxKind::ProposedBlock);
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
						let topic_wrapper = gossipsub::IdentTopic::new(Topic::FinalisedBlock);
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert!(tx.kind() == TxKind::FinalisedBlock);
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

fn handle_dht_events(swarm: &mut Swarm<MyBehaviour>, event: kad::Event) {
	match event {
		kad::Event::OutboundQueryProgressed { result, .. } => match result {
			kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {
				key,
				providers,
				..
			})) => {
				for peer in providers {
					info!(
						"Peer {peer:?} provides key {:?}",
						std::str::from_utf8(key.as_ref()).unwrap()
					);
					if let Err(e) = swarm.dial(peer) {
						error!("Error dialing peer: {:?}", e);
					}
				}
			},
			_ => {},
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

	for topic in
		topics_assignment.iter().chain(&[Topic::ProposedBlock, Topic::FinalisedBlock]).clone()
	{
		// Create a Gossipsub topic
		let topic = gossipsub::IdentTopic::new(topic.clone());
		// subscribes to our topic
		swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
	}

	for topic in topics_scores.iter().chain(&topics_commitment) {
		let topic = gossipsub::IdentTopic::new(topic.clone());
		let key = RecordKey::new(&topic.hash().to_string());
		swarm.behaviour_mut().kademlia.start_providing(key)?;
	}

	// Listen on all interfaces and whatever port the OS assigns
	swarm.listen_on("/ip4/0.0.0.0/udp/10000/quic-v1".parse()?)?;
	swarm.listen_on("/ip4/0.0.0.0/tcp/10000".parse()?)?;

	// Dial the peer identified by the multi-address given as the second
	// command-line argument, if any.
	let bootstrap_node_addr = if let Ok(addr) = std::env::var("SEQUENCER_ADDR") {
		println!("{:?}", addr);
		let remote: Multiaddr = addr.parse()?;
		swarm.dial(remote.clone())?;
		debug!("Dialed {addr}");
		Some(remote)
	} else {
		None
	};

	// Interval at which to look for new publishers of the topics of interest
	let mut topic_search_interval = interval(Duration::from_secs(15));

	// Kick it off
	loop {
		select! {
			instant = topic_search_interval.tick() => {
				for topic in topics_assignment
					.iter()
					.chain(&[Topic::ProposedBlock, Topic::FinalisedBlock]) {
					let topic = gossipsub::IdentTopic::new(topic.clone());
					let key = RecordKey::new(&topic.hash().to_string());
					let query_id = swarm.behaviour_mut().kademlia.get_providers(key.clone());
					debug!("Requesting providers of {:?} {:?} {:?}", key, query_id, instant);
				}
			}
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
				SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
					debug!("Connection Error: {:?} {:?}", peer_id, error);
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
						.chain(&[Topic::ProposedBlock, Topic::FinalisedBlock]);
					handle_gossipsub_events(&mut swarm, event, iter_chain.collect());
				},
				SwarmEvent::Behaviour(MyBehaviourEvent::Kademlia(event)) => {
					handle_dht_events(&mut swarm, event);
				}
				SwarmEvent::NewListenAddr { address, .. } => {
					info!("Local node is listening on {address}");
				}
				e => info!("{:?}", e),
			}
		}
	}
}
