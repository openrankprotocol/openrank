use alloy_rlp::encode;
use clap::{Parser, Subcommand};
use csv::StringRecord;
use karyon_jsonrpc::Client;
use openrank_common::{
	topics::Domain,
	tx_event::TxEvent,
	txs::{
		Address, JobRunRequest, OwnedNamespace, ScoreEntry, SeedUpdate, TrustEntry, TrustUpdate,
		Tx, TxHash, TxKind,
	},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::{error::Error, io::Read};

const TRUST_CHUNK_SIZE: usize = 500;
const SEED_CHUNK_SIZE: usize = 1000;

#[derive(Debug, Clone, Subcommand)]
enum Method {
	TrustUpdate { path: String, config_path: String, output_path: Option<String> },
	SeedUpdate { path: String, config_path: String, output_path: Option<String> },
	JobRunRequest { path: String, output_path: Option<String> },
	GetResults { request_id: String, config_path: String, output_path: Option<String> },
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
	#[command(subcommand)]
	method: Method,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobRunRequestResult {
	job_run_tx_hash: TxHash,
	tx_event: TxEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobRunResults {
	votes: Vec<bool>,
	scores: Vec<ScoreEntry>,
}

impl JobRunRequestResult {
	pub fn new(job_run_tx_hash: TxHash, tx_event: TxEvent) -> Self {
		Self { job_run_tx_hash, tx_event }
	}
}

fn read_config(path: &str) -> Result<Config, Box<dyn Error>> {
	let mut f = File::open(path)?;
	let mut toml_config = String::new();
	f.read_to_string(&mut toml_config)?;
	let config: Config = toml::from_str(toml_config.as_str())?;
	Ok(config)
}

async fn update_trust(path: &str, config_path: &str) -> Result<Vec<TxEvent>, Box<dyn Error>> {
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

	let mut results = Vec::new();
	for chunk in entries.chunks(TRUST_CHUNK_SIZE) {
		let owned_namespace = OwnedNamespace::new(Address::default(), 1);
		let data = encode(TrustUpdate::new(owned_namespace.clone(), chunk.to_vec()));
		let tx = encode(Tx::default_with(TxKind::TrustUpdate, data));

		let result: Value = client.call("Sequencer.trust_update", hex::encode(tx)).await?;
		let tx_event: TxEvent = serde_json::from_value(result)?;

		results.push(tx_event);
	}
	Ok(results)
}

async fn update_seed(path: &str, config_path: &str) -> Result<Vec<TxEvent>, Box<dyn Error>> {
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
	let mut results = Vec::new();
	for chunk in entries.chunks(SEED_CHUNK_SIZE) {
		let owned_namespace = OwnedNamespace::new(Address::default(), 1);
		let data = encode(SeedUpdate::new(owned_namespace.clone(), chunk.to_vec()));
		let tx = encode(Tx::default_with(TxKind::SeedUpdate, data));

		let result: Value = client.call("Sequencer.seed_update", hex::encode(tx)).await?;
		let tx_event: TxEvent = serde_json::from_value(result)?;

		results.push(tx_event);
	}
	Ok(results)
}

async fn job_run_request(path: &str) -> Result<JobRunRequestResult, Box<dyn Error>> {
	let config = read_config(path)?;
	// Creates a new client
	let client = Client::builder(config.sequencer.endpoint.as_str())?.build().await?;

	let domain_id = config.domain.to_hash();
	let data = encode(JobRunRequest::new(domain_id, u32::MAX));
	let tx = Tx::default_with(TxKind::JobRunRequest, data);
	let tx_hash = tx.hash();

	let result: Value = client.call("Sequencer.job_run_request", hex::encode(encode(tx))).await?;
	let tx_event: TxEvent = serde_json::from_value(result)?;

	let job_run_result = JobRunRequestResult::new(tx_hash, tx_event);
	Ok(job_run_result)
}

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
		Method::TrustUpdate { path, config_path, output_path } => {
			let res = update_trust(path.as_str(), config_path.as_str()).await?;
			if let Some(output_path) = output_path {
				// println!("{:?}", serde_json::to_string(&res));
			}
		},
		Method::SeedUpdate { path, config_path, output_path } => {
			let res = update_seed(path.as_str(), config_path.as_str()).await?;
			if let Some(output_path) = output_path {
				// println!("{:?}", serde_json::to_string(&res));
			}
		},
		Method::JobRunRequest { path, output_path } => {
			let res = job_run_request(path.as_str()).await?;
			let hex_encoded_tx_hash = hex::encode(res.job_run_tx_hash.0);
			println!("{}", hex_encoded_tx_hash);
			if let Some(output_path) = output_path {
				// println!("{:?}", serde_json::to_string(&res));
			}
		},
		Method::GetResults { request_id, config_path, output_path } => {
			let (votes, mut results) = get_results(request_id, config_path.as_str()).await?;
			results.reverse();
			let chunk = results.chunks(10).next();
			if let Some(scores) = chunk {
				for res in scores {
					println!("{}: {}", res.id, res.value);
				}
			}
			if let Some(output_path) = output_path {
				// println!("{:?}", serde_json::to_string(&res));
			}
		},
	}
	Ok(())
}
