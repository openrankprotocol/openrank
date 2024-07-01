use alloy_rlp::encode;
use futures::StreamExt;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent};
use openrank_common::{
	build_node,
	topics::{Domain, Topic},
	tx_event::TxEvent,
	txs::{Address, JobRunRequest, SeedUpdate, TrustUpdate, Tx, TxKind},
	MyBehaviourEvent,
};
use std::error::Error;
use tokio::{
	io::{self, AsyncBufReadExt},
	select,
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

pub async fn run() -> Result<(), Box<dyn Error>> {
	tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();

	let mut swarm = build_node().await?;
	info!("PEER_ID: {:?}", swarm.local_peer_id());

	// Listen on all interfaces and whatever port the OS assigns
	swarm.listen_on("/ip4/0.0.0.0/udp/8000/quic-v1".parse()?)?;
	swarm.listen_on("/ip4/0.0.0.0/tcp/8000".parse()?)?;

	info!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");

	let domains = vec![Domain::new(
		Address::default(),
		"1".to_string(),
		Address::default(),
		"1".to_string(),
		0,
	)];
	let topics_request: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainRequest(domain_hash.clone()))
		.collect();
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

	// Read full lines from stdin
	let mut stdin = io::BufReader::new(io::stdin()).lines();

	// Kick it off
	loop {
		select! {
			inp = stdin.next_line() => {
				match inp {
					Ok(Some(line)) => {
						match line.as_str() {
							"request" => {
								for topic in &topics_request {
									let tx = Tx::default_with(TxKind::JobRunRequest, encode(JobRunRequest::default()));
									let default_tx = TxEvent::default_with_data(encode(tx));
									let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
									if let Err(e) = swarm
										.behaviour_mut().gossipsub
										.publish(topic_wrapper, encode(default_tx)) {
										error!("Publish error: {e:?}");
									}
								}
							},
							"trust_update" => {
								for topic in &topics_trust_update {
									let tx = Tx::default_with(TxKind::TrustUpdate, encode(TrustUpdate::default()));
									let default_tx = TxEvent::default_with_data(encode(tx));
									let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
									if let Err(e) = swarm
										.behaviour_mut().gossipsub
										.publish(topic_wrapper, encode(default_tx)) {
										error!("Publish error: {e:?}");
									}
								}
							}
							"seed_update" => {
								for topic in &topics_seed_update {
									let tx = Tx::default_with(TxKind::SeedUpdate, encode(SeedUpdate::default()));
									let default_tx = TxEvent::default_with_data(encode(tx));
									let topic_wrapper = gossipsub::IdentTopic::new(topic.clone());
									if let Err(e) = swarm
										.behaviour_mut().gossipsub
										.publish(topic_wrapper, encode(default_tx)) {
										error!("Publish error: {e:?}");
									}
								}
							}
							s => info!("stdin: {:?}", s),
						}
					},
					Err(e) => info!("stdin: {:?}", e),
					_ => {}
				}
			}
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
				SwarmEvent::NewListenAddr { address, .. } => {
					info!("Local node is listening on {address}");
				},
				e => info!("{:?}", e),
			}
		}
	}
}
