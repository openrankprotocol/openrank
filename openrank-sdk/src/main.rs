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
use std::error::Error;
use std::fs::File;

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
	arg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
	pub domain: Domain,
}

async fn update_trust() -> Result<(), Box<dyn Error>> {
	let f = File::open("./openrank-sdk/trust-db.csv")?;
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

	// Creates a new client
	let client = Client::builder("tcp://127.0.0.1:60000")?.build().await?;

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

async fn update_seed() -> Result<(), Box<dyn Error>> {
	let f = File::open("./openrank-sdk/seed-db.csv")?;
	let mut rdr = csv::Reader::from_reader(f);
	let mut entries = Vec::new();
	for result in rdr.records() {
		let record: StringRecord = result?;
		let (i, value): (u32, f32) = record.deserialize(None)?;
		let i_addr = Address::from(i);
		let score_entry = ScoreEntry::new(i_addr, value);
		entries.push(score_entry);
	}

	// Creates a new client
	let client = Client::builder("tcp://127.0.0.1:60000")?.build().await?;

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

async fn job_run_request() -> Result<(), Box<dyn Error>> {
	let config: Config = toml::from_str(include_str!("../config.toml"))?;

	// Creates a new client
	let client = Client::builder("tcp://127.0.0.1:60000")?.build().await?;

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

async fn get_results(arg: String) -> Result<(Vec<bool>, Vec<ScoreEntry>), Box<dyn Error>> {
	// Creates a new client
	let client = Client::builder("tcp://127.0.0.1:60000")?.build().await?;
	let result: Value = client.call("Sequencer.get_results", arg).await?;
	let scores: (Vec<bool>, Vec<ScoreEntry>) = serde_json::from_value(result)?;
	Ok(scores)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let cli = Args::parse();

	match cli.method {
		Method::TrustUpdate => {
			update_trust().await?;
		},
		Method::SeedUpdate => {
			update_seed().await?;
		},
		Method::JobRunRequest => {
			job_run_request().await?;
		},
		Method::GetResults => {
			let arg = match cli.arg {
				Some(arg) => arg,
				None => {
					eprintln!("Missing argument");
					std::process::exit(1);
				},
			};
			let (votes, mut results) = get_results(arg).await?;
			println!("votes: {:?}", votes);
			results.reverse();
			for res in results.chunks(100).next().unwrap() {
				println!("{}: {}", res.id, res.value);
			}
		},
	}
	Ok(())
}