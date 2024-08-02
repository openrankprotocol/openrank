use alloy_rlp::{encode, Decodable};
use futures::StreamExt;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
	broadcast_event, build_node,
	db::{Db, DbItem},
	result::JobResult,
	topics::{Domain, Topic},
	tx_event::TxEvent,
	txs::{
		CreateCommitment, CreateScores, JobRunAssignment, JobRunRequest, JobVerification, Tx,
		TxKind,
	},
	MyBehaviour, MyBehaviourEvent,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::select;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
	pub domains: Vec<Domain>,
}

fn handle_gossipsub_events(
	mut swarm: &mut Swarm<MyBehaviour>, db: &Db, event: gossipsub::Event, topics: Vec<&Topic>,
) {
	match event {
		gossipsub::Event::Message { propagation_source: peer_id, message_id: id, message } => {
			for topic in topics {
				match topic {
					Topic::DomainRequest(domain_id) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert!(tx.kind() == TxKind::JobRunRequest);
							// Add Tx to db
							db.put(tx.clone()).unwrap();
							let _ = JobRunRequest::decode(&mut tx.body().as_slice()).unwrap();
							info!(
								"TOPIC: {}, ID: {id}, FROM: {peer_id}",
								message.topic.as_str(),
							);

							let assignment_topic = Topic::DomainAssignent(domain_id.clone());
							let job_assignment = encode(JobRunAssignment::default_with(tx.hash()));
							let tx = Tx::default_with(TxKind::JobRunAssignment, job_assignment);
							db.put(tx.clone()).unwrap();
							if let Err(e) = broadcast_event(&mut swarm, tx, assignment_topic) {
								error!("Publish error: {e:?}");
							}
						}
					},
					Topic::DomainCommitment(_) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert_eq!(tx.kind(), TxKind::CreateCommitment);
							// Add Tx to db
							db.put(tx.clone()).unwrap();

							let commitment =
								CreateCommitment::decode(&mut tx.body().as_slice()).unwrap();

							let assignment_tx_key = Tx::construct_full_key(
								TxKind::JobRunAssignment,
								commitment.job_run_assignment_tx_hash.0.to_vec(),
							);
							let assignment_tx: Tx = db.get(assignment_tx_key).unwrap();
							let assignment_body =
								JobRunAssignment::decode(&mut assignment_tx.body().as_slice())
									.unwrap();
							let request_tx_key = Tx::construct_full_key(
								TxKind::JobRunRequest,
								assignment_body.job_run_request_tx_hash.0.to_vec(),
							);
							let request: Tx = db.get(request_tx_key).unwrap();
							let result = JobResult::new(tx.hash(), Vec::new(), request.hash());
							db.put(result).unwrap();
							info!(
								"TOPIC: {}, ID: {id}, FROM: {peer_id}",
								message.topic.as_str(),
							);
						}
					},
					Topic::DomainScores(_) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert_eq!(tx.kind(), TxKind::CreateScores);
							// Add Tx to db
							db.put(tx.clone()).unwrap();
							let _ = CreateScores::decode(&mut tx.body().as_slice()).unwrap();
							info!(
								"TOPIC: {}, ID: {id}, FROM: {peer_id}",
								message.topic.as_str(),
							);
						}
					},
					Topic::DomainVerification(_) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert_eq!(tx.kind(), TxKind::JobVerification);
							// Add Tx to db
							db.put(tx.clone()).unwrap();
							let _ = JobVerification::decode(&mut tx.body().as_slice()).unwrap();
							info!(
								"TOPIC: {}, ID: {id}, FROM: {peer_id}",
								message.topic.as_str(),
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

	// Listen on all interfaces and whatever port the OS assigns
	swarm.listen_on("/ip4/0.0.0.0/udp/9000/quic-v1".parse()?)?;
	swarm.listen_on("/ip4/0.0.0.0/tcp/9000".parse()?)?;

	let config: Config = toml::from_str(include_str!("../config.toml"))?;
	let db = Db::new("./local-storage", &[&Tx::get_cf(), &JobResult::get_cf()])?;

	let topics_requests: Vec<Topic> = config
		.domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainRequest(domain_hash.clone()))
		.collect();
	let topics_commitment: Vec<Topic> = config
		.domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainCommitment(domain_hash.clone()))
		.collect();
	let topics_scores: Vec<Topic> = config
		.domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainScores(domain_hash.clone()))
		.collect();
	let topics_verification: Vec<Topic> = config
		.domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainVerification(domain_hash.clone()))
		.collect();

	for topic in topics_verification
		.iter()
		.chain(&topics_commitment)
		.chain(&topics_scores)
		.chain(&topics_requests)
	{
		// Create a Gossipsub topic
		let topic = gossipsub::IdentTopic::new(topic.clone());
		// subscribes to our topic
		swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
	}

	// Kick it off
	loop {
		select! {
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
				SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(event)) => {
					let iter_chain = topics_requests.iter().chain(&topics_commitment).chain(&topics_scores).chain(&topics_verification);
					handle_gossipsub_events(&mut swarm, &db, event, iter_chain.collect());
				},
				SwarmEvent::NewListenAddr { address, .. } => {
					info!("Local node is listening on {address}");
				},
				e => info!("NEW_EVENT: {:?}", e),
			}
		}
	}
}
