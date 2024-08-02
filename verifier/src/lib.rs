use alloy_rlp::{encode, Decodable};
use futures::StreamExt;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
	broadcast_event, build_node,
	db::{Db, DbItem},
	topics::{Domain, Topic},
	tx_event::TxEvent,
	txs::{
		CreateCommitment, CreateScores, JobRunAssignment, JobVerification, SeedUpdate, TrustUpdate,
		Tx, TxKind,
	},
	MyBehaviour, MyBehaviourEvent,
};
use runner::VerificationJobRunner;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::select;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod runner;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
	pub domains: Vec<Domain>,
}

fn handle_gossipsub_events(
	swarm: &mut Swarm<MyBehaviour>, job_runner: &mut VerificationJobRunner, db: &Db,
	event: gossipsub::Event, topics: Vec<&Topic>, domains: Vec<Domain>,
) {
	match event {
		gossipsub::Event::Message { propagation_source: peer_id, message_id: id, message } => {
			for topic in topics {
				match topic {
					Topic::NamespaceTrustUpdate(namespace) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert!(tx.kind() == TxKind::TrustUpdate);
							// Add Tx to db
							db.put(tx.clone()).unwrap();
							let trust_update =
								TrustUpdate::decode(&mut tx.body().as_slice()).unwrap();
							assert!(*namespace == trust_update.trust_id);
							let domain =
								domains.iter().find(|x| &x.trust_namespace() == namespace).unwrap();
							job_runner.update_trust(domain.clone(), trust_update.entries.clone());
							info!(
								"TOPIC: {}, ID: {id}, FROM: {peer_id}",
								message.topic.as_str(),
							);
						}
					},
					Topic::NamespaceSeedUpdate(namespace) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert!(tx.kind() == TxKind::SeedUpdate);
							// Add Tx to db
							db.put(tx.clone()).unwrap();
							let seed_update =
								SeedUpdate::decode(&mut tx.body().as_slice()).unwrap();
							assert!(*namespace == seed_update.seed_id);
							let domain =
								domains.iter().find(|x| &x.trust_namespace() == namespace).unwrap();
							job_runner.update_seed(domain.clone(), seed_update.entries.clone());
							info!(
								"TOPIC: {}, ID: {id}, FROM: {peer_id}",
								message.topic.as_str(),
							);
						}
					},
					Topic::DomainAssignent(domain_id) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert_eq!(tx.kind(), TxKind::JobRunAssignment);
							// Add Tx to db
							db.put(tx.clone()).unwrap();
							// Not checking if this node is assigned for the job
							let _ = JobRunAssignment::decode(&mut tx.body().as_slice()).unwrap();

							let domain =
								domains.iter().find(|x| &x.to_hash() == domain_id).unwrap();
							job_runner.update_assigment(domain.clone(), tx.hash());
							info!(
								"TOPIC: {}, ID: {id}, FROM: {peer_id}",
								message.topic.as_str(),
							);
						}
					},
					Topic::DomainScores(domain_id) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert_eq!(tx.kind(), TxKind::CreateScores);
							// Add Tx to db
							db.put(tx.clone()).unwrap();
							let create_scores =
								CreateScores::decode(&mut tx.body().as_slice()).unwrap();

							let domain =
								domains.iter().find(|x| &x.to_hash() == domain_id).unwrap();
							job_runner.update_scores(
								domain.clone(),
								tx.hash(),
								create_scores.clone(),
							);
							let res = job_runner.check_finished_jobs(domain.clone());
							for (tx_hash, verification_res) in res {
								let verification_res =
									JobVerification::new(tx_hash, verification_res);
								let tx = Tx::default_with(
									TxKind::JobVerification,
									encode(verification_res),
								);
								broadcast_event(
									swarm,
									tx,
									Topic::DomainVerification(domain_id.clone()),
								)
								.unwrap();
							}
							info!(
								"TOPIC: {}, ID: {id}, FROM: {peer_id}",
								message.topic.as_str(),
							);
						}
					},
					Topic::DomainCommitment(domain_id) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
							let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
							assert_eq!(tx.kind(), TxKind::CreateCommitment);
							// Add Tx to db
							db.put(tx.clone()).unwrap();
							let create_commitment =
								CreateCommitment::decode(&mut tx.body().as_slice()).unwrap();

							let domain =
								domains.iter().find(|x| &x.to_hash() == domain_id).unwrap();
							job_runner.update_commitment(create_commitment.clone());
							let res = job_runner.check_finished_jobs(domain.clone());
							for (tx_hash, verification_res) in res {
								let verification_res =
									JobVerification::new(tx_hash, verification_res);
								let tx = Tx::default_with(
									TxKind::JobVerification,
									encode(verification_res),
								);
								broadcast_event(
									swarm,
									tx,
									Topic::DomainVerification(domain_id.clone()),
								)
								.unwrap();
							}
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

	let config: Config = toml::from_str(include_str!("../config.toml"))?;
	let db = Db::new("./local-storage", &[&Tx::get_cf()])?;
	let domain_hashes = config.domains.iter().map(|x| x.to_hash()).collect();
	let mut job_runner = VerificationJobRunner::new(domain_hashes);

	let topics_trust_update: Vec<Topic> = config
		.domains
		.clone()
		.into_iter()
		.map(|x| x.trust_namespace())
		.map(|namespace| Topic::NamespaceTrustUpdate(namespace.clone()))
		.collect();
	let topics_seed_update: Vec<Topic> = config
		.domains
		.clone()
		.into_iter()
		.map(|x| x.seed_namespace())
		.map(|namespace| Topic::NamespaceSeedUpdate(namespace.clone()))
		.collect();
	let topics_assignment: Vec<Topic> = config
		.domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainAssignent(domain_hash.clone()))
		.collect();
	let topics_scores: Vec<Topic> = config
		.domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainScores(domain_hash.clone()))
		.collect();
	let topics_commitment: Vec<Topic> = config
		.domains
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
					handle_gossipsub_events(&mut swarm, &mut job_runner, &db, event, iter_chain.clone().collect(), config.domains.clone());
				},
				SwarmEvent::NewListenAddr { address, .. } => {
					info!("Local node is listening on {address}");
				}
				e => info!("{:?}", e),
			}
		}
	}
}
