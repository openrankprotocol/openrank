use alloy_rlp::encode;
use futures::StreamExt;
use libp2p::{core::ConnectedPoint, gossipsub, identify, kad::Mode, swarm::SwarmEvent};
use openrank_common::{
	build_node,
	topics::{Domain, Topic},
	tx_event::TxEvent,
	txs::{Address, JobRunRequest, Tx, TxKind},
	MyBehaviourEvent,
};
use std::error::Error;
use tokio::{
	io::{self, AsyncBufReadExt},
	select,
};
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

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
	let topics_request: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainRequest(domain_hash.clone()))
		.collect();

	// Listen on all interfaces and whatever port the OS assigns
	swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
	swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

	info!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");

	// Read full lines from stdin
	let mut stdin = io::BufReader::new(io::stdin()).lines();

	swarm.behaviour_mut().kademlia.set_mode(Some(Mode::Server));

	// Kick it off
	loop {
		select! {
			Ok(Some(line)) = stdin.next_line() => {
				match line.as_str() {
					"request" => {
						for topic in &topics_request {
							let tx = Tx::default_with(TxKind::JobRunRequest, encode(JobRunRequest::default()));
							let default_tx = TxEvent::default_with_data(encode(tx));
							let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
							if let Err(e) = swarm
								.behaviour_mut().gossipsub
								.publish(topic_wrapper, encode(default_tx)) {
								error!("Publish error: {e:?}");
							}
						}
					},
					&_ => {}
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
				SwarmEvent::Behaviour(MyBehaviourEvent::Identify(identify::Event::Received { peer_id, info })) => {
					swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
					for addr in info.listen_addrs {
						swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
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
