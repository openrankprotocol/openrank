use alloy_rlp::encode;
use clap::{Parser, Subcommand};
use csv::StringRecord;
use dotenv::dotenv;
use hex::FromHex;
use k256::{ecdsa::SigningKey, schnorr::CryptoRngCore};
use karyon_jsonrpc::Client;
use openrank_common::{
    address_from_sk,
    algos::et::is_converged_address,
    merkle::hash_leaf,
    topics::Domain,
    tx_event::TxEvent,
    txs::{
        Address, JobRunRequest, ScoreEntry, SeedUpdate, TrustEntry, TrustUpdate, Tx, TxHash, TxKind,
    },
};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha3::Keccak256;
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
};
use std::{error::Error, io::BufWriter};

const TRUST_CHUNK_SIZE: usize = 500;
const SEED_CHUNK_SIZE: usize = 1000;

#[derive(Debug, Clone, Subcommand)]
enum Method {
    TrustUpdate { path: String, config_path: String, output_path: Option<String> },
    SeedUpdate { path: String, config_path: String, output_path: Option<String> },
    JobRunRequest { path: String, output_path: Option<String> },
    GetResults { request_id: String, config_path: String, output_path: Option<String> },
    GetResultsAndCheckIntegrity { request_id: String, config_path: String, test_vector: String },
    GenerateKeypair,
    ShowAddress,
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

async fn update_trust(
    sk: SigningKey, path: &str, config_path: &str,
) -> Result<Vec<TxEvent>, Box<dyn Error>> {
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
        let data = encode(TrustUpdate::new(
            config.domain.trust_namespace(),
            chunk.to_vec(),
        ));
        let mut tx = Tx::default_with(TxKind::TrustUpdate, data);
        tx.sign(&sk)?;

        let result: Value = client.call("Sequencer.trust_update", hex::encode(encode(tx))).await?;
        let tx_event: TxEvent = serde_json::from_value(result)?;

        results.push(tx_event);
    }
    Ok(results)
}

async fn update_seed(
    sk: SigningKey, path: &str, config_path: &str,
) -> Result<Vec<TxEvent>, Box<dyn Error>> {
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
        let data = encode(SeedUpdate::new(
            config.domain.seed_namespace(),
            chunk.to_vec(),
        ));
        let mut tx = Tx::default_with(TxKind::SeedUpdate, data);
        tx.sign(&sk)?;

        let result: Value = client.call("Sequencer.seed_update", hex::encode(encode(tx))).await?;
        let tx_event: TxEvent = serde_json::from_value(result)?;

        results.push(tx_event);
    }
    Ok(results)
}

async fn job_run_request(
    sk: SigningKey, path: &str,
) -> Result<JobRunRequestResult, Box<dyn Error>> {
    let config = read_config(path)?;
    // Creates a new client
    let client = Client::builder(config.sequencer.endpoint.as_str())?.build().await?;

    let rng = &mut thread_rng();
    let domain_id = config.domain.to_hash();
    let hash = hash_leaf::<Keccak256>(rng.gen::<[u8; 32]>().to_vec());
    let data = encode(JobRunRequest::new(domain_id, 0, hash));
    let mut tx = Tx::default_with(TxKind::JobRunRequest, data);
    tx.sign(&sk)?;
    let tx_hash = tx.hash();

    let result: Value = client.call("Sequencer.job_run_request", hex::encode(encode(tx))).await?;
    let tx_event: TxEvent = serde_json::from_value(result)?;

    let job_run_result = JobRunRequestResult::new(tx_hash, tx_event);
    Ok(job_run_result)
}

async fn get_results(
    arg: String, config_path: &str,
) -> Result<(Vec<bool>, Vec<ScoreEntry>), Box<dyn Error>> {
    let config = read_config(config_path)?;
    // Creates a new client
    let client = Client::builder(config.sequencer.endpoint.as_str())?.build().await?;
    let result: Value = client.call("Sequencer.get_results", arg).await?;
    let scores: (Vec<bool>, Vec<ScoreEntry>) = serde_json::from_value(result)?;
    Ok(scores)
}

fn generate_keypair<R: CryptoRngCore>(rng: &mut R) -> (SigningKey, Address) {
    let sk = SigningKey::random(rng);
    let addr = address_from_sk(&sk);
    (sk, addr)
}

fn check_score_integrity(
    votes: Vec<bool>, scores: Vec<ScoreEntry>, path: &str,
) -> Result<bool, Box<dyn Error>> {
    let f = File::open(path)?;
    let mut rdr = csv::Reader::from_reader(f);
    let mut test_vector = Vec::new();
    for result in rdr.records() {
        let record: StringRecord = result?;
        let (i, value): (String, f32) = record.deserialize(None)?;
        let i_addr = Address::from_hex(i)?;
        let score_entry = ScoreEntry::new(i_addr, value);
        test_vector.push(score_entry);
    }

    let mut test_map = HashMap::new();
    for score in test_vector {
        test_map.insert(score.id, score.value);
    }

    let mut score_map = HashMap::new();
    for score in scores {
        score_map.insert(score.id, score.value);
    }

    let is_converged = is_converged_address(&test_map, &score_map);
    let votes = votes.iter().fold(true, |acc, vote| acc & vote);

    Ok(is_converged & votes)
}

fn write_json_to_file<T: Serialize>(path: &str, data: T) -> Result<(), Box<dyn Error>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &data)?;
    writer.flush()?;
    Ok(())
}

fn get_secret_key() -> Result<SigningKey, Box<dyn Error>> {
    let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
    let secret_key_bytes = hex::decode(secret_key_hex)?;
    let secret_key = SigningKey::from_slice(secret_key_bytes.as_slice())?;
    Ok(secret_key)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let cli = Args::parse();

    match cli.method {
        Method::TrustUpdate { path, config_path, output_path } => {
            let secret_key = get_secret_key()?;
            let res = update_trust(secret_key, path.as_str(), config_path.as_str()).await?;
            if let Some(output_path) = output_path {
                write_json_to_file(&output_path, res)?;
            }
        },
        Method::SeedUpdate { path, config_path, output_path } => {
            let secret_key = get_secret_key()?;
            let res = update_seed(secret_key, path.as_str(), config_path.as_str()).await?;
            if let Some(output_path) = output_path {
                write_json_to_file(&output_path, res)?;
            }
        },
        Method::JobRunRequest { path, output_path } => {
            let secret_key = get_secret_key()?;
            let res = job_run_request(secret_key, path.as_str()).await?;
            let hex_encoded_tx_hash = hex::encode(res.job_run_tx_hash.0);
            println!("{}", hex_encoded_tx_hash);
            if let Some(output_path) = output_path {
                write_json_to_file(&output_path, res)?;
            }
        },
        Method::GetResults { request_id, config_path, output_path } => {
            let (votes, mut results) = get_results(request_id, config_path.as_str()).await?;
            results.reverse();
            let chunk = results.chunks(10).next();

            println!("votes: {:?}", votes);
            if let Some(scores) = chunk {
                for res in scores {
                    println!("{}: {}", res.id, res.value);
                }
            }
            if let Some(output_path) = output_path {
                write_json_to_file(&output_path, results)?;
            }
        },
        Method::GetResultsAndCheckIntegrity { request_id, config_path, test_vector } => {
            let (votes, mut results) = get_results(request_id, config_path.as_str()).await?;
            results.reverse();
            let res = check_score_integrity(votes, results, &test_vector)?;
            println!("Integrity check result: {}", res);
            assert!(res);
        },
        Method::GenerateKeypair => {
            let rng = &mut thread_rng();
            let (sk, address) = generate_keypair(rng);
            let sk_bytes = sk.to_bytes();
            println!("SIGNING_KEY: {}", hex::encode(sk_bytes));
            println!("ADDRESS:     {}", address.to_hex());
        },
        Method::ShowAddress => {
            let secret_key = get_secret_key()?;
            let addr = address_from_sk(&secret_key);
            println!("ADDRESS: {}", addr.to_hex());
        },
    }
    Ok(())
}
