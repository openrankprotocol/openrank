use alloy_rlp::{encode, Decodable};
use futures::StreamExt;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
	broadcast_event, build_node,
	db::{Db, DbError, DbItem},
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
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod error;
mod runner;
use error::VerifierNodeError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
	pub domains: Vec<Domain>,
}

fn handle_gossipsub_events(
	swarm: &mut Swarm<MyBehaviour>, job_runner: &mut VerificationJobRunner, db: &Db,
	event: gossipsub::Event, topics: Vec<&Topic>, domains: Vec<Domain>,
) -> Result<(), VerifierNodeError> {
	match event {
		gossipsub::Event::Message { propagation_source: peer_id, message_id: id, message } => {
			for topic in topics {
				match topic {
					Topic::NamespaceTrustUpdate(namespace) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							let mut tx = Tx::decode(&mut tx_event.data().as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							assert!(tx.kind() == TxKind::TrustUpdate);
							// Add Tx to db
							tx.set_sequence_number(message.sequence_number.unwrap_or_default());
							db.put(tx.clone()).map_err(|e| VerifierNodeError::DbError(e))?;
							let trust_update = TrustUpdate::decode(&mut tx.body().as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							assert!(*namespace == trust_update.trust_id);
							let domain =
								domains.iter().find(|x| &x.trust_namespace() == namespace).ok_or(
									VerifierNodeError::DomainNotFound(namespace.clone().to_hex()),
								)?;
							job_runner
								.update_trust(domain.clone(), trust_update.entries.clone())
								.map_err(Into::into)?;
							info!(
								"TOPIC: {}, ID: {id}, FROM: {peer_id}",
								message.topic.as_str(),
							);
						}
					},
					Topic::NamespaceSeedUpdate(namespace) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							let mut tx = Tx::decode(&mut tx_event.data().as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							assert!(tx.kind() == TxKind::SeedUpdate);
							// Add Tx to db
							tx.set_sequence_number(message.sequence_number.unwrap_or_default());
							db.put(tx.clone()).map_err(|e| VerifierNodeError::DbError(e))?;
							let seed_update = SeedUpdate::decode(&mut tx.body().as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							assert!(*namespace == seed_update.seed_id);
							let domain =
								domains.iter().find(|x| &x.trust_namespace() == namespace).ok_or(
									VerifierNodeError::DomainNotFound(namespace.clone().to_hex()),
								)?;
							job_runner
								.update_seed(domain.clone(), seed_update.entries.clone())
								.map_err(Into::into)?;
							info!(
								"TOPIC: {}, ID: {id}, FROM: {peer_id}",
								message.topic.as_str(),
							);
						}
					},
					Topic::DomainAssignent(domain_id) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							let tx = Tx::decode(&mut tx_event.data().as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							assert_eq!(tx.kind(), TxKind::JobRunAssignment);
							// Add Tx to db
							db.put(tx.clone()).map_err(|e| VerifierNodeError::DbError(e))?;
							// Not checking if this node is assigned for the job
							JobRunAssignment::decode(&mut tx.body().as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							let domain = domains.iter().find(|x| &x.to_hash() == domain_id).ok_or(
								VerifierNodeError::DomainNotFound(domain_id.clone().to_hex()),
							)?;
							let _ = job_runner
								.update_assigment(domain.clone(), tx.hash())
								.map_err(Into::into)?;
							let res = job_runner
								.check_finished_jobs(domain.clone())
								.map_err(Into::into)?;
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
								.map_err(|e| VerifierNodeError::P2PError(e.to_string()))?;
							}
							info!(
								"TOPIC: {}, ID: {id}, FROM: {peer_id}",
								message.topic.as_str(),
							);
						}
					},
					Topic::DomainScores(domain_id) => {
						let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
						if message.topic == topic_wrapper.hash() {
							let tx_event = TxEvent::decode(&mut message.data.as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							let tx = Tx::decode(&mut tx_event.data().as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							assert_eq!(tx.kind(), TxKind::CreateScores);
							// Add Tx to db
							db.put(tx.clone()).map_err(|e| VerifierNodeError::DbError(e))?;
							let create_scores = CreateScores::decode(&mut tx.body().as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;

							let domain = domains.iter().find(|x| &x.to_hash() == domain_id).ok_or(
								VerifierNodeError::DomainNotFound(domain_id.clone().to_hex()),
							)?;
							let _ = job_runner
								.update_scores(domain.clone(), tx.hash(), create_scores.clone())
								.map_err(Into::into)?;
							let res = job_runner
								.check_finished_jobs(domain.clone())
								.map_err(Into::into)?;
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
								.map_err(|e| VerifierNodeError::P2PError(e.to_string()))?;
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
							let tx_event = TxEvent::decode(&mut message.data.as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							let tx = Tx::decode(&mut tx_event.data().as_slice())
								.map_err(|e| VerifierNodeError::SerdeError(e))?;
							assert_eq!(tx.kind(), TxKind::CreateCommitment);
							// Add Tx to db
							db.put(tx.clone()).map_err(|e| VerifierNodeError::DbError(e))?;
							let create_commitment =
								CreateCommitment::decode(&mut tx.body().as_slice())
									.map_err(|e| VerifierNodeError::SerdeError(e))?;

							let domain = domains.iter().find(|x| &x.to_hash() == domain_id).ok_or(
								VerifierNodeError::DomainNotFound(domain_id.clone().to_hex()),
							)?;
							job_runner.update_commitment(create_commitment.clone());
							let res = job_runner
								.check_finished_jobs(domain.clone())
								.map_err(Into::into)?;
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
								.map_err(|e| VerifierNodeError::P2PError(e.to_string()))?;
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

	Ok(())
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

	node_recovery(&mut job_runner, &db, &config)?;

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
					match handle_gossipsub_events(&mut swarm, &mut job_runner, &db, event, iter_chain.clone().collect(), config.domains.clone()) {
						Ok(_) => {},
						Err(e) => {
							error!("Failed to handle gossipsub event: {e:?}");
							continue;
						},
					}
				},
				SwarmEvent::NewListenAddr { address, .. } => {
					info!("Local node is listening on {address}");
				}
				e => info!("{:?}", e),
			}
		}
	}
}

/// Recover JobRunner state from DB.
///
/// - Load all the TXs from the DB
/// - Just take TrustUpdate and SeedUpdate transactions
/// - Update JobRunner using functions update_trust, update_seed
fn node_recovery(
	job_runner: &mut VerificationJobRunner, db: &Db, config: &Config,
) -> Result<(), VerifierNodeError> {
	let db_iter = db.read_from_start_iter::<Tx>();
	for maybe_entry in db_iter {
		let (_, value) =
			maybe_entry.map_err(|e| VerifierNodeError::DbError(DbError::RocksDB(e)))?;
		let tx: Tx = serde_json::from_slice(&value)
			.map_err(|e| VerifierNodeError::DbError(DbError::Serde(e)))?;
		match tx.kind() {
			TxKind::TrustUpdate => job_runner_update_trust_update(job_runner, &config.domains, tx)?,
			TxKind::SeedUpdate => job_runner_update_seed_update(job_runner, &config.domains, tx)?,
			_ => (),
		}
	}
	Ok(())
}

fn job_runner_update_trust_update(
	job_runner: &mut VerificationJobRunner, domains: &Vec<Domain>, tx: Tx,
) -> Result<(), VerifierNodeError> {
	let trust_update = TrustUpdate::decode(&mut tx.body().as_slice())
		.map_err(|e| VerifierNodeError::SerdeError(e))?;
	let namespace = trust_update.trust_id;
	let domain = domains.iter().find(|x| &x.trust_namespace() == &namespace).ok_or(
		VerifierNodeError::DomainNotFound(namespace.clone().to_hex()),
	)?;
	job_runner
		.update_trust(domain.clone(), trust_update.entries.clone())
		.map_err(|e| VerifierNodeError::ComputeInternalError(e))?;

	Ok(())
}

fn job_runner_update_seed_update(
	job_runner: &mut VerificationJobRunner, domains: &Vec<Domain>, tx: Tx,
) -> Result<(), VerifierNodeError> {
	let seed_update = SeedUpdate::decode(&mut tx.body().as_slice())
		.map_err(|e| VerifierNodeError::SerdeError(e))?;
	let namespace = seed_update.seed_id;
	let domain = domains.iter().find(|x| &x.seed_namespace() == &namespace).ok_or(
		VerifierNodeError::DomainNotFound(namespace.clone().to_hex()),
	)?;
	job_runner
		.update_seed(domain.clone(), seed_update.entries.clone())
		.map_err(|e| VerifierNodeError::ComputeInternalError(e))?;
	Ok(())
}
