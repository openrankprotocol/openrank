use alloy_rlp::encode;
use clap::{Parser, ValueEnum};
use csv::StringRecord;
use karyon_jsonrpc::Client;
use openrank_common::{
	topics::Domain,
	tx_event::TxEvent,
	txs::{
		Address, JobRunRequest, OwnedNamespace, ScoreEntry, SeedUpdate, TrustEntry, TrustUpdate,
		Tx, TxKind,
	},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::{error::Error, io::Read};

const TRUST_CHUNK_SIZE: usize = 500;
const SEED_CHUNK_SIZE: usize = 1000;

#[derive(Parser, Debug, Clone, ValueEnum)]
enum Method {
	TrustUpdate,
	SeedUpdate,
	JobRunRequest,
	GetResults,
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
	#[arg(value_enum)]
	method: Method,
	arg1: Option<String>,
	arg2: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sequencer {
	endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
	pub domain: Domain,
	pub sequencer: Sequencer,
}

/// Create a new `Config` from a local TOML file, given file path
fn read_config(path: &str) -> Result<Config, Box<dyn Error>> {
	let mut f = File::open(path)?;
	let mut toml_config = String::new();
	f.read_to_string(&mut toml_config)?;
	let config: Config = toml::from_str(toml_config.as_str())?;
	Ok(config)
}

/// 1. Read a CSV file and get a list of `TrustEntry`
/// 2. Create a new `Client`, which can be used to call the Sequencer
/// 3. Send the list of `TrustEntry` to the Sequencer
async fn update_trust(path: &str, config_path: &str) -> Result<(), Box<dyn Error>> {
	let f = File::open(path)?;
	let mut rdr = csv::Reader::from_reader(f);
	let mut entries = Vec::new();
	for result in rdr.records() {
		let record: StringRecord = result?;
		let (from, to, value): (u32, u32, f32) = record.deserialize(None)?;
		let from_addr = Address::from(from);
		let to_addr = Address::from(to);
		let trust_entry = TrustEntry::new(from_addr, to_addr, value);
		entries.push(trust_entry);
	}

	let config = read_config(config_path)?;
	// Creates a new client
	let client = Client::builder(config.sequencer.endpoint.as_str())?.build().await?;

	for chunk in entries.chunks(TRUST_CHUNK_SIZE) {
		let owned_namespace = OwnedNamespace::new(Address::default(), 1);
		let data = encode(TrustUpdate::new(owned_namespace.clone(), chunk.to_vec()));
		let tx = encode(Tx::default_with(TxKind::TrustUpdate, data));

		let result: Value = client.call("Sequencer.trust_update", hex::encode(tx)).await?;
		let tx_event: TxEvent = serde_json::from_value(result)?;

		println!("Res: {:?}", tx_event);
	}
	Ok(())
}

/// 1. Read a CSV file and get a list of `ScoreEntry`
/// 2. Create a new `Client`, which can be used to call the Sequencer
/// 3. Send the list of `ScoreEntry` to the Sequencer
async fn update_seed(path: &str, config_path: &str) -> Result<(), Box<dyn Error>> {
	let f = File::open(path)?;
	let mut rdr = csv::Reader::from_reader(f);
	let mut entries = Vec::new();
	for result in rdr.records() {
		let record: StringRecord = result?;
		let (i, value): (u32, f32) = record.deserialize(None)?;
		let i_addr = Address::from(i);
		let score_entry = ScoreEntry::new(i_addr, value);
		entries.push(score_entry);
	}

	let config = read_config(config_path)?;
	// Creates a new client
	let client = Client::builder(config.sequencer.endpoint.as_str())?.build().await?;

	for chunk in entries.chunks(SEED_CHUNK_SIZE) {
		let owned_namespace = OwnedNamespace::new(Address::default(), 1);
		let data = encode(SeedUpdate::new(owned_namespace.clone(), chunk.to_vec()));
		let tx = encode(Tx::default_with(TxKind::SeedUpdate, data));

		let result: Value = client.call("Sequencer.seed_update", hex::encode(tx)).await?;
		let tx_event: TxEvent = serde_json::from_value(result)?;

		println!("Res: {:?}", tx_event);
	}
	Ok(())
}

/// 1. Create a new `Client`, which can be used to call the Sequencer
/// 2. Send a `JobRunRequest` to the Sequencer
async fn job_run_request(path: &str) -> Result<(), Box<dyn Error>> {
	let config = read_config(path)?;
	// Creates a new client
	let client = Client::builder(config.sequencer.endpoint.as_str())?.build().await?;

	let domain_id = config.domain.to_hash();
	let data = encode(JobRunRequest::new(domain_id, u32::MAX));
	let tx = Tx::default_with(TxKind::JobRunRequest, data);
	let tx_hash = tx.hash();
	let hex_encoded_tx_hash = hex::encode(tx_hash.0);
	println!("JobRunRequest TX_HASH: {}", hex_encoded_tx_hash);

	let result: Value = client.call("Sequencer.job_run_request", hex::encode(encode(tx))).await?;
	let tx_event: TxEvent = serde_json::from_value(result)?;

	println!("Res: {:?}", tx_event);
	Ok(())
}

/// 1. Create a new `Client`, which can be used to call the Sequencer
/// 2. Call the Sequencer to get the results(`ScoreEntry`s)
async fn get_results(
	arg: String, path: &str,
) -> Result<(Vec<bool>, Vec<ScoreEntry>), Box<dyn Error>> {
	let config = read_config(path)?;
	// Creates a new client
	let client = Client::builder(config.sequencer.endpoint.as_str())?.build().await?;
	let result: Value = client.call("Sequencer.get_results", arg).await?;
	let scores: (Vec<bool>, Vec<ScoreEntry>) = serde_json::from_value(result)?;
	Ok(scores)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let cli = Args::parse();

	match cli.method {
		Method::TrustUpdate => {
			let arg1 = cli.arg1.unwrap_or_else(|| {
				eprintln!("Missing argument");
				std::process::exit(1);
			});
			let arg2 = cli.arg2.unwrap_or_else(|| {
				eprintln!("Missing argument");
				std::process::exit(1);
			});
			update_trust(arg1.as_str(), arg2.as_str()).await?;
		},
		Method::SeedUpdate => {
			let arg1 = cli.arg1.unwrap_or_else(|| {
				eprintln!("Missing argument");
				std::process::exit(1);
			});
			let arg2 = cli.arg2.unwrap_or_else(|| {
				eprintln!("Missing argument");
				std::process::exit(1);
			});
			update_seed(arg1.as_str(), arg2.as_str()).await?;
		},
		Method::JobRunRequest => {
			let arg1 = cli.arg1.unwrap_or_else(|| {
				eprintln!("Missing argument");
				std::process::exit(1);
			});
			job_run_request(arg1.as_str()).await?;
		},
		Method::GetResults => {
			let arg1 = cli.arg1.unwrap_or_else(|| {
				eprintln!("Missing argument");
				std::process::exit(1);
			});
			let arg2 = cli.arg2.unwrap_or_else(|| {
				eprintln!("Missing argument");
				std::process::exit(1);
			});
			let (votes, mut results) = get_results(arg1, arg2.as_str()).await?;
			println!("votes: {:?}", votes);
			results.reverse();
			let chunk = results.chunks(100).next();
			if let Some(scores) = chunk {
				for res in scores {
					println!("{}: {}", res.id, res.value);
				}
			}
		},
	}
	Ok(())
}
