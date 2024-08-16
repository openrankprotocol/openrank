use alloy_rlp::{encode, Decodable};
use dotenv::dotenv;
use futures::StreamExt;
use k256::ecdsa::SigningKey;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
	broadcast_event, build_node,
	db::{Db, DbError, DbItem},
	result::JobResult,
	topics::{Domain, Topic},
	tx_event::TxEvent,
	txs::{
		Address, CreateCommitment, CreateScores, JobRunAssignment, JobRunRequest, JobVerification,
		Tx, TxKind,
	},
	MyBehaviour, MyBehaviourEvent,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::select;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Whitelist {
	pub computer: Vec<Address>,
	pub verifier: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
	pub domains: Vec<Domain>,
	pub whitelist: Whitelist,
}

fn handle_gossipsub_events(
	mut swarm: &mut Swarm<MyBehaviour>, db: &Db, event: gossipsub::Event, topics: Vec<&Topic>,
	sk: &SigningKey,
) {
	if let gossipsub::Event::Message { message_id, message, propagation_source } = event {
		for topic in topics {
			match topic {
				Topic::DomainRequest(domain_id) => {
					let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
					if message.topic == topic_wrapper.hash() {
						let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
						let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
						assert!(tx.kind() == TxKind::JobRunRequest);
						let (res, _) = tx.verify();
						assert!(res);
						// Add Tx to db
						db.put(tx.clone()).unwrap();
						let _ = JobRunRequest::decode(&mut tx.body().as_slice()).unwrap();
						info!(
							"TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
							message.topic.as_str(),
						);

						let assignment_topic = Topic::DomainAssignent(domain_id.clone());
						let job_assignment = encode(JobRunAssignment::default_with(tx.hash()));
						let mut tx = Tx::default_with(TxKind::JobRunAssignment, job_assignment);
						tx.sign(sk);
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
						let (res, _) = tx.verify();
						assert!(res);
						// Add Tx to db
						db.put(tx.clone()).unwrap();

						let commitment =
							CreateCommitment::decode(&mut tx.body().as_slice()).unwrap();

						let assignment_tx_key = Tx::construct_full_key(
							TxKind::JobRunAssignment,
							commitment.job_run_assignment_tx_hash,
						);
						let assignment_tx: Tx = db.get(assignment_tx_key).unwrap();
						let assignment_body =
							JobRunAssignment::decode(&mut assignment_tx.body().as_slice()).unwrap();
						let request_tx_key = Tx::construct_full_key(
							TxKind::JobRunRequest,
							assignment_body.job_run_request_tx_hash.clone(),
						);
						let request: Tx = db.get(request_tx_key).unwrap();
						let job_result_key =
							JobResult::construct_full_key(assignment_body.job_run_request_tx_hash);
						if let Err(DbError::NotFound) = db.get::<JobResult>(job_result_key) {
							let result = JobResult::new(tx.hash(), Vec::new(), request.hash());
							db.put(result).unwrap();
						}
						info!(
							"TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
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
						let (res, _) = tx.verify();
						assert!(res);
						// Add Tx to db
						db.put(tx.clone()).unwrap();
						let _ = CreateScores::decode(&mut tx.body().as_slice()).unwrap();
						info!(
							"TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
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
						let (res, _) = tx.verify();
						assert!(res);
						// Add Tx to db
						db.put(tx.clone()).unwrap();
						let job_verification =
							JobVerification::decode(&mut tx.body().as_slice()).unwrap();

						let assignment_tx_key = Tx::construct_full_key(
							TxKind::JobRunAssignment,
							job_verification.job_run_assignment_tx_hash,
						);
						let assignment_tx: Tx = db.get(assignment_tx_key).unwrap();
						let assignment_body =
							JobRunAssignment::decode(&mut assignment_tx.body().as_slice()).unwrap();
						let job_result_key =
							JobResult::construct_full_key(assignment_body.job_run_request_tx_hash);
						let mut job_result: JobResult = db.get(job_result_key).unwrap();
						job_result.job_verification_tx_hashes.push(tx.hash());
						db.put(job_result).unwrap();
						info!(
							"TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
							message.topic.as_str(),
						);
					}
				},
				_ => {},
			}
		}
	}
}

pub async fn run() -> Result<(), Box<dyn Error>> {
	dotenv().ok();
	tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();

	let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
	let secret_key =
		SigningKey::from_slice(hex::decode(secret_key_hex).unwrap().as_slice()).unwrap();

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
					handle_gossipsub_events(&mut swarm, &db, event, iter_chain.collect(), &secret_key);
				},
				SwarmEvent::NewListenAddr { address, .. } => {
					info!("Local node is listening on {address}");
				},
				e => info!("NEW_EVENT: {:?}", e),
			}
		}
	}
}
