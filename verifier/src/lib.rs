use alloy_rlp::{encode, Decodable};
use futures::StreamExt;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
	broadcast_event, build_node,
	topics::{Domain, Topic},
	txs::{
		CreateCommitment, CreateScores, FinalisedBlock, JobRunAssignment, JobVerification,
		ProposedBlock, SeedUpdate, TrustUpdate, Tx, TxKind,
	},
	MyBehaviour, MyBehaviourEvent,
};
use openrank_common::{tx_event::TxEvent, txs::Address};
use std::error::Error;
use tokio::select;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

fn handle_gossipsub_events(
	mut swarm: &mut Swarm<MyBehaviour>, event: gossipsub::Event, topics: Vec<&Topic>,
) {
	match event {
		gossipsub::Event::Message { propagation_source: peer_id, message_id: id, message } => {
			for topic in topics {
				match topic {
					Topic::DomainTrustUpdate(_) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert!(tx.kind() == TxKind::TrustUpdate);
							let trust_update =
								TrustUpdate::decode(&mut tx.body().as_slice()).unwrap();
							info!(
								"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
								message.topic.as_str(),
								trust_update,
							);
						}
					},
					Topic::DomainSeedUpdate(_) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert!(tx.kind() == TxKind::SeedUpdate);
							let seed_update =
								SeedUpdate::decode(&mut tx.body().as_slice()).unwrap();
							info!(
								"TOPIC: {}, TX: '{:?}' ID: {id} FROM: {peer_id}",
								message.topic.as_str(),
								seed_update,
							);
						}
					},
					Topic::DomainAssignent(_) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
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
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
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
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
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
								new_topic,
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

	let iter_chain = topics_assignment
		.iter()
		.chain(&topics_trust_update)
		.chain(&topics_seed_update)
		.chain(&topics_scores)
		.chain(&topics_commitment)
		.chain(&[Topic::ProposedBlock, Topic::FinalisedBlock]);
	for topic in iter_chain.clone() {
		// Create a Gossipsub topic
		let topic = gossipsub::IdentTopic::new(topic.clone());
		// subscribes to our topic
		swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
	}

	// Listen on all interfaces and whatever port the OS assigns
	swarm.listen_on("/ip4/0.0.0.0/udp/11001/quic-v1".parse()?)?;
	swarm.listen_on("/ip4/0.0.0.0/tcp/11001".parse()?)?;

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
				SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(event)) => {
					handle_gossipsub_events(&mut swarm, event, iter_chain.clone().collect());
				},
				SwarmEvent::NewListenAddr { address, .. } => {
					info!("Local node is listening on {address}");
				}
				e => info!("{:?}", e),
			}
		}
	}
}
