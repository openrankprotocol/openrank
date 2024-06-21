use futures::{prelude::*, select};
use libp2p::core::ConnectedPoint;
use libp2p::kad::store::MemoryStore;
use libp2p::kad::Mode;
use libp2p::{
	identify, noise,
	swarm::{NetworkBehaviour, SwarmEvent},
	tcp, yamux,
};
use libp2p::{kad, Multiaddr};
use std::error::Error;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

// We create a custom network behaviour that combines Kademlia and mDNS.
#[derive(NetworkBehaviour)]
struct MyBehaviour {
	kademlia: kad::Behaviour<MemoryStore>,
	identify: identify::Behaviour,
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

	let mut swarm = libp2p::SwarmBuilder::with_new_identity()
		.with_async_std()
		.with_tcp(
			tcp::Config::default(),
			noise::Config::new,
			yamux::Config::default,
		)?
		.with_behaviour(|key| {
			Ok(MyBehaviour {
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
		.with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
		.build();

	println!("PEER_ID: {:?}", swarm.local_peer_id());

	// Listen on all interfaces and whatever port the OS assigns.
	swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

	// Dial the peer identified by the multi-address given as the second
	// command-line argument, if any.
	if let Some(addr) = std::env::args().nth(1) {
		let remote: Multiaddr = addr.parse()?;
		swarm.dial(remote)?;
		println!("Dialed {addr}")
	} else {
		swarm.behaviour_mut().kademlia.set_mode(Some(Mode::Server));
	}

	// Kick it off.
	loop {
		select! {
			event = swarm.select_next_some() => match event {
				SwarmEvent::NewListenAddr { address, .. } => {
					println!("Listening in {address:?}");
				},
				SwarmEvent::NewExternalAddrOfPeer { peer_id, address } => {
					println!("New peer: {:?} {:?}", peer_id, address);
					swarm.behaviour_mut().kademlia.add_address(&peer_id, address);
					if let Err(err) = swarm.behaviour_mut().kademlia.bootstrap() {
						println!("Failed to bootstrap DHT: {:?}", err);
					}
				},
				SwarmEvent::ConnectionClosed { peer_id, endpoint: ConnectedPoint::Dialer { address, .. }, ..} => {
					swarm.behaviour_mut().kademlia.remove_address(&peer_id, &address);
				},
				e => {
					// println!("{:?}", e);
				}
			}
		}
	}
}
