use alloy_rlp::{encode, Decodable};
use dotenv::dotenv;
use futures::StreamExt;
use k256::ecdsa::SigningKey;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent, Swarm};
use openrank_common::{
	address_from_sk, broadcast_event, build_node,
	db::{Db, DbItem},
	topics::{Domain, Topic},
	tx_event::TxEvent,
	txs::{
		Address, CreateCommitment, JobRunAssignment, SeedUpdate, TrustUpdate, Tx, TxHash, TxKind,
	},
	MyBehaviour, MyBehaviourEvent,
};
use runner::ComputeJobRunner;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::select;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod runner;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Whitelist {
	block_builder: Vec<Address>,
	verifier: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
	pub domains: Vec<Domain>,
	pub whitelist: Whitelist,
}

fn handle_gossipsub_events(
	swarm: &mut Swarm<MyBehaviour>, job_runner: &mut ComputeJobRunner, db: &Db,
	event: gossipsub::Event, topics: Vec<&Topic>, domains: Vec<Domain>, sk: &SigningKey,
	whitelist: &Whitelist,
) {
	if let gossipsub::Event::Message { propagation_source, message_id, message } = event {
		for topic in topics {
			match topic {
				Topic::NamespaceTrustUpdate(namespace) => {
					let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
					if message.topic == topic_wrapper.hash() {
						let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
						let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
						assert!(tx.kind() == TxKind::TrustUpdate);
						let res = tx.verify_against(namespace.owner());
						assert!(res);
						// Add Tx to db
						db.put(tx.clone()).unwrap();
						let trust_update = TrustUpdate::decode(&mut tx.body().as_slice()).unwrap();
						assert!(*namespace == trust_update.trust_id);
						let domain =
							domains.iter().find(|x| &x.trust_namespace() == namespace).unwrap();
						job_runner.update_trust(domain.clone(), trust_update.entries.clone());
						info!(
							"TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
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
						let res = tx.verify_against(namespace.owner());
						assert!(res);
						// Add Tx to db
						db.put(tx.clone()).unwrap();
						let seed_update = SeedUpdate::decode(&mut tx.body().as_slice()).unwrap();
						assert!(*namespace == seed_update.seed_id);
						let domain =
							domains.iter().find(|x| &x.trust_namespace() == namespace).unwrap();
						job_runner.update_seed(domain.clone(), seed_update.entries.clone());
						info!(
							"TOPIC: {}, ID: {message_id}, FROM: {propagation_source}",
							message.topic.as_str(),
						);
					}
				},
				Topic::DomainAssignent(domain_id) => {
					let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
					if message.topic == topic_wrapper.hash() {
						let tx_event = TxEvent::decode(&mut message.data.as_slice()).unwrap();
						let tx = Tx::decode(&mut tx_event.data().as_slice()).unwrap();
						assert!(tx.kind() == TxKind::JobRunAssignment);
						let (res, address) = tx.verify();
						assert!(whitelist.block_builder.contains(&address));
						assert!(res);
						// Add Tx to db
						db.put(tx.clone()).unwrap();
						// Not checking if we are assigned for the job, for now
						let job_run_assignment =
							JobRunAssignment::decode(&mut tx.body().as_slice()).unwrap();
						let computer_address = address_from_sk(sk);
						assert_eq!(computer_address, job_run_assignment.assigned_compute_node);
						assert!(whitelist
							.verifier
							.contains(&job_run_assignment.assigned_verifier_node));

						let domain = domains.iter().find(|x| &x.to_hash() == domain_id).unwrap();
						job_runner.compute(domain.clone());
						job_runner.create_compute_tree(domain.clone());
						let create_scores = job_runner.get_create_scores(domain.clone());
						let (lt_root, compute_root) = job_runner.get_root_hashes(domain.clone());
						let create_scores_tx: Vec<Tx> = create_scores
							.iter()
							.map(|tx_body| Tx::default_with(TxKind::CreateScores, encode(tx_body)))
							.map(|mut tx| {
								tx.sign(sk);
								tx
							})
							.collect();
						let create_scores_tx_hashes: Vec<TxHash> =
							create_scores_tx.iter().map(|x| x.hash()).collect();
						let create_scores_topic = Topic::DomainScores(domain_id.clone());
						let commitment_topic = Topic::DomainCommitment(domain_id.clone());
						let create_commitment = CreateCommitment::default_with(
							tx.hash(),
							lt_root,
							compute_root,
							create_scores_tx_hashes,
						);
						let mut create_commitment_tx =
							Tx::default_with(TxKind::CreateCommitment, encode(create_commitment));
						create_commitment_tx.sign(sk);
						for scores in create_scores_tx {
							broadcast_event(swarm, scores, create_scores_topic.clone()).unwrap();
						}
						broadcast_event(swarm, create_commitment_tx, commitment_topic).unwrap();

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

	let config: Config = toml::from_str(include_str!("../config.toml"))?;
	let db = Db::new("./local-storage", &[&Tx::get_cf()])?;

	let domain_hashes = config.domains.iter().map(|x| x.to_hash()).collect();
	let mut job_runner = ComputeJobRunner::new(domain_hashes);

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

	let iter_chain = topics_assignment
		.iter()
		.chain(topics_trust_update.iter())
		.chain(topics_seed_update.iter())
		.chain(&[Topic::ProposedBlock, Topic::FinalisedBlock]);
	for topic in iter_chain.clone() {
		// Create a Gossipsub topic
		let topic = gossipsub::IdentTopic::new(topic.clone());
		// subscribes to our topic
		swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
	}

	// Listen on all interfaces and whatever port the OS assigns
	swarm.listen_on("/ip4/0.0.0.0/udp/10000/quic-v1".parse()?)?;
	swarm.listen_on("/ip4/0.0.0.0/tcp/10000".parse()?)?;

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
					handle_gossipsub_events(
						&mut swarm,
						&mut job_runner,
						&db,
						event,
						iter_chain.clone().collect(),
						config.domains.clone(),
						&secret_key,
						&config.whitelist
					);
				},
				SwarmEvent::NewListenAddr { address, .. } => {
					info!("Local node is listening on {address}");
				}
				e => info!("{:?}", e),
			}
		}
	}
}
