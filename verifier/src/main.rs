mod txs;

use futures::StreamExt;
use libp2p::{
	gossipsub, mdns, noise,
	swarm::{NetworkBehaviour, SwarmEvent},
	tcp, yamux, Swarm,
};
use std::{
	error::Error,
	hash::{DefaultHasher, Hash, Hasher},
	time::Duration,
};
use tokio::{io, select};
use tracing_subscriber::EnvFilter;
use txs::Address;

#[derive(Clone, Debug)]
struct DomainHash(u64);
struct Domain {
	trust_owner: Address,
	trust_suffix: String,
	seed_owner: Address,
	seed_suffix: String,
	algo_id: u64,
}

impl Domain {
	fn new(
		trust_owner: Address, trust_suffix: String, seed_owner: Address, seed_suffix: String,
		algo_id: u64,
	) -> Self {
		Self { trust_owner, trust_suffix, seed_owner, seed_suffix, algo_id }
	}

	fn to_hash(&self) -> DomainHash {
		let mut s = DefaultHasher::new();
		s.write(&self.trust_owner.0);
		s.write(self.trust_suffix.as_bytes());
		s.write(&self.seed_owner.0);
		s.write(self.seed_suffix.as_bytes());
		s.write(&self.algo_id.to_be_bytes());
		let res = s.finish();
		DomainHash(res)
	}
}

struct TopicHash(u64);

impl TopicHash {
	fn to_hex(&self) -> String {
		hex::encode(self.0.to_be_bytes())
	}
}

enum Topic {
	DomainAssignent(DomainHash),
	DomainCommitment(DomainHash),
	DomainScores(DomainHash),
	DomainVerification(DomainHash),
}

impl Topic {
	fn to_hash(&self) -> TopicHash {
		let mut s = DefaultHasher::new();
		match self {
			Self::DomainAssignent(domain_id) => {
				s.write(&domain_id.0.to_be_bytes());
				s.write("assignment".as_bytes());
			},
			Self::DomainCommitment(domain_id) => {
				s.write(&domain_id.0.to_be_bytes());
				s.write("commitment".as_bytes());
			},
			Self::DomainScores(domain_id) => {
				s.write(&domain_id.0.to_be_bytes());
				s.write("scores".as_bytes());
			},
			Self::DomainVerification(domain_id) => {
				s.write(&domain_id.0.to_be_bytes());
				s.write("verification".as_bytes());
			},
		}
		let res = s.finish();
		TopicHash(res)
	}
}

// We create a custom network behaviour that combines Gossipsub and Mdns.
#[derive(NetworkBehaviour)]
struct MyBehaviour {
	gossipsub: gossipsub::Behaviour,
	mdns: mdns::tokio::Behaviour,
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
			// To content-address message, we can take the hash of message and use it as an ID.
			let message_id_fn = |message: &gossipsub::Message| {
				let mut s = DefaultHasher::new();
				message.data.hash(&mut s);
				gossipsub::MessageId::from(s.finish().to_string())
			};

			// Set a custom gossipsub configuration
			let gossipsub_config = gossipsub::ConfigBuilder::default()
				.heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
				.validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
				.message_id_fn(message_id_fn) // content-address messages. No two messages of the same content will be propagated.
				.build()
				.map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?; // Temporary hack because `build` does not return a proper `std::error::Error`.

			// build a gossipsub network behaviour
			let gossipsub = gossipsub::Behaviour::new(
				gossipsub::MessageAuthenticity::Signed(key.clone()),
				gossipsub_config,
			)?;

			let mdns =
				mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;
			Ok(MyBehaviour { gossipsub, mdns })
		})?
		.with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
		.build();

	Ok(swarm)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

	let mut swarm = build_node().await?;

	let domains = vec![Domain::new(
		Address::default(),
		"1".to_string(),
		Address::default(),
		"1".to_string(),
		0,
	)];
	let sub_topics: Vec<Topic> = domains
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| {
			let assignment = Topic::DomainAssignent(domain_hash.clone());
			let scores = Topic::DomainScores(domain_hash.clone());
			let commitment = Topic::DomainCommitment(domain_hash.clone());
			vec![assignment, scores, commitment]
		})
		.flatten()
		.collect();

	for topic in sub_topics {
		// Create a Gossipsub topic
		let topic = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
		// subscribes to our topic
		swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
	}

	// Listen on all interfaces and whatever port the OS assigns
	swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
	swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

	println!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");

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
				SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
					propagation_source: peer_id,
					message_id: id,
					message,
				})) => println!(
						"Got message: '{}' with id: {id} from peer: {peer_id}",
						String::from_utf8_lossy(&message.data),
					),
				SwarmEvent::NewListenAddr { address, .. } => {
					println!("Local node is listening on {address}");
				}
				_ => {}
			}
		}
	}
}
