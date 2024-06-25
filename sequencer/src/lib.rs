use futures::StreamExt;
use libp2p::{
	core::ConnectedPoint,
	gossipsub, identify,
	kad::{self, store::MemoryStore, Mode},
	noise,
	swarm::{NetworkBehaviour, SwarmEvent},
	tcp, yamux, Swarm,
};
use openrank_common::{
	topics::{Domain, Topic},
	tx_event::TxEvent,
	txs::{Address, JobRunRequest, Tx, TxKind},
};
use std::{error::Error, time::Duration};
use tokio::{
	io::{self, AsyncBufReadExt},
	select,
};
use tracing_subscriber::EnvFilter;

// We create a custom network behaviour.
#[derive(NetworkBehaviour)]
struct MyBehaviour {
	gossipsub: gossipsub::Behaviour,
	kademlia: kad::Behaviour<MemoryStore>,
	identify: identify::Behaviour,
}

async fn build_node() -> Result<Swarm<MyBehaviour>, Box<dyn Error>> {
	let swarm = libp2p::SwarmBuilder::with_new_identity()
		.with_tokio()
		.with_tcp(
			tcp::Config::default(),
			noise::Config::new,
			yamux::Config::default,
		)?
		.with_quic()
		.with_behaviour(|key| {
			// Set a custom gossipsub configuration
			let gossipsub_config = gossipsub::ConfigBuilder::default()
				// This is set to aid debugging by not cluttering the log space
				.heartbeat_interval(Duration::from_secs(10))
				// This sets the kind of message validation. The default is Strict (enforce message signing)
				.validation_mode(gossipsub::ValidationMode::Strict)
				.build()
				// Temporary hack because `build` does not return a proper `std::error::Error`.
				.map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?;

			// build a gossipsub network behaviour
			let gossipsub = gossipsub::Behaviour::new(
				gossipsub::MessageAuthenticity::Signed(key.clone()),
				gossipsub_config,
			)?;

			Ok(MyBehaviour {
				gossipsub,
				kademlia: kad::Behaviour::new(
					key.public().to_peer_id(),
					MemoryStore::new(key.public().to_peer_id()),
				),
				identify: identify::Behaviour::new(identify::Config::new(
					"openrank/1.0.0".to_string(),
					key.public(),
				)),
			})
		})?
		.with_swarm_config(|c| c.with_idle_connection_timeout(Duration::MAX))
		.build();

	Ok(swarm)
}

pub async fn run() -> Result<(), Box<dyn Error>> {
	let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

	let mut swarm = build_node().await?;
	println!("PEER_ID: {:?}", swarm.local_peer_id());

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

	println!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");

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
							let tx = Tx::default_with(TxKind::JobRunRequest, JobRunRequest::default().to_bytes());
							let default_tx = TxEvent::default_with_data(tx.to_bytes());
							let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
							if let Err(e) = swarm
								.behaviour_mut().gossipsub
								.publish(topic_wrapper, default_tx.to_bytes()) {
								println!("Publish error: {e:?}");
							}
						}
					},
					&_ => {}
				}
			}
			event = swarm.select_next_some() => match event {
				SwarmEvent::NewExternalAddrOfPeer { peer_id, address } => {
					println!("New peer: {:?} {:?}", peer_id, address);
					swarm.behaviour_mut().kademlia.add_address(&peer_id, address);
					swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
				},
				SwarmEvent::ConnectionClosed { peer_id, endpoint: ConnectedPoint::Dialer { address, .. }, ..} => {
					println!("Connection closed: {:?} {:?}", peer_id, address);
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
					println!("Local node is listening on {address}");
				},
				_ => {},
			}
		}
	}
}
