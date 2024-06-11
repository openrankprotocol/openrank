use libp2p::{gossipsub, mdns, noise, swarm::NetworkBehaviour, tcp, yamux, Swarm};
use openrank_common::{
	topics::{Domain, Topic},
	txs::Address,
};
use std::{
	error::Error,
	hash::{DefaultHasher, Hash, Hasher},
	time::Duration,
};
use tokio::{
	io::{self, AsyncBufReadExt},
	select,
};
use tracing_subscriber::EnvFilter;

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

pub async fn run() -> Result<(), Box<dyn Error>> {
	let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

	let mut swarm = build_node().await?;

	let domains = vec![Domain::new(
		Address::default(),
		"1".to_string(),
		Address::default(),
		"1".to_string(),
		0,
	)];
	let sub_topics_assignment: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainAssignent(domain_hash.clone()))
		.collect();
	let sub_topics_scores: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainScores(domain_hash.clone()))
		.collect();
	let sub_topics_commitment: Vec<Topic> = domains
		.clone()
		.into_iter()
		.map(|x| x.to_hash())
		.map(|domain_hash| Topic::DomainCommitment(domain_hash.clone()))
		.collect();

	// Read full lines from stdin
	let mut stdin = io::BufReader::new(io::stdin()).lines();

	// Listen on all interfaces and whatever port the OS assigns
	swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
	swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

	println!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");

	// Kick it off
	loop {
		select! {
			Ok(Some(line)) = stdin.next_line() => {
				match line.as_str() {
					"assignment" => {
						for topic in &sub_topics_assignment {
							let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
							if let Err(e) = swarm
								.behaviour_mut().gossipsub
								.publish(topic_wrapper, line.as_bytes()) {
								println!("Publish error: {e:?}");
							}
						}
					},
					"scores" => {
						for topic in &sub_topics_scores {
							let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
							if let Err(e) = swarm
								.behaviour_mut().gossipsub
								.publish(topic_wrapper, line.as_bytes()) {
								println!("Publish error: {e:?}");
							}
						}
					},
					"commitment" => {
						for topic in &sub_topics_commitment {
							let topic_wrapper = gossipsub::IdentTopic::new(topic.to_hash().to_hex());
							if let Err(e) = swarm
								.behaviour_mut().gossipsub
								.publish(topic_wrapper, line.as_bytes()) {
								println!("Publish error: {e:?}");
							}
						}
					},
					&_ => {}
				}
			}
		}
	}
}
