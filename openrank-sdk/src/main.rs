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
	let f = File::open("./openrank-sdk/trust.csv")?;
	let mut rdr = csv::Reader::from_reader(f);
	let mut entries = Vec::new();
	for result in rdr.records() {
		let record: StringRecord = result?;
		let te: TrustEntry = record.deserialize(None)?;
		entries.push(te);
	}

	// Creates a new client
	let client = Client::builder("tcp://127.0.0.1:60000")?.build().await?;

	for chunk in entries.chunks(1000) {
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
	let f = File::open("./openrank-sdk/seed.csv")?;
	let mut rdr = csv::Reader::from_reader(f);
	let mut entries = Vec::new();
	for result in rdr.records() {
		let record: StringRecord = result?;
		let se: ScoreEntry = record.deserialize(None)?;
		entries.push(se);
	}

	// Creates a new client
	let client = Client::builder("tcp://127.0.0.1:60000")?.build().await?;

	for chunk in entries.chunks(1000) {
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
	let tx = encode(Tx::default_with(TxKind::SeedUpdate, data));

	let result: Value = client.call("Sequencer.job_run_request", hex::encode(tx)).await?;
	let tx_event: TxEvent = serde_json::from_value(result)?;

	println!("Res: {:?}", tx_event);
	Ok(())
}

async fn get_results(arg: String) -> Result<Vec<ScoreEntry>, Box<dyn Error>> {
	// Creates a new client
	let client = Client::builder("tcp://127.0.0.1:60000")?.build().await?;
	let result: Value = client.call("Sequencer.get_results", arg).await?;
	let scores: Vec<ScoreEntry> = serde_json::from_value(result)?;
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
			let arg = cli.arg.unwrap();
			let results = get_results(arg).await?;
			for res in results.get(0..100).unwrap() {
				println!("{:?}: {}", res.id, res.value);
			}
		},
	}
	Ok(())
}
