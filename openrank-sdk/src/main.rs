use alloy_rlp::encode;
use clap::{Parser, Subcommand};
use csv::StringRecord;
use dotenv::dotenv;
use jsonrpsee::{core::client::ClientT, http_client::HttpClient};
use k256::{ecdsa::SigningKey, schnorr::CryptoRngCore};
use openrank_common::{
    address_from_sk,
    algos::et::is_converged_org,
    merkle::hash_leaf,
    result::GetResultsQuery,
    topics::Domain,
    tx_event::TxEvent,
    txs::{
        compute,
        trust::{ScoreEntry, SeedUpdate, TrustEntry, TrustUpdate},
        Address, Kind, Tx, TxHash,
    },
};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
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
/// The method to call.
enum Method {
    /// Trust update. The method takes a list of trust entries and updates the trust graph.
    TrustUpdate { path: String, config_path: String, output_path: Option<String> },
    /// Seed update. The method takes a list of seed entries and updates the seed vector.
    SeedUpdate { path: String, config_path: String, output_path: Option<String> },
    /// The method creates a ComputeRequest TX.
    ComputeRequest { path: String, output_path: Option<String> },
    /// The method takes a ComputeRequest TX hash and returns the computed results.
    GetResults { request_id: String, config_path: String, output_path: Option<String> },
    /// The method takes a ComputeRequest TX hash and returns the computed results,
    /// and also checks the integrity/correctness of the results.
    GetResultsAndCheckIntegrity { request_id: String, config_path: String, test_vector: String },
    /// The method generates a new ECDSA keypair and returns the address and the private key.
    GenerateKeypair,
    /// The method shows the address of the node, given the private key.
    ShowAddress,
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
/// The command line arguments.
struct Args {
    #[command(subcommand)]
    method: Method,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The configuration for the Sequencer.
pub struct Sequencer {
    endpoint: String,
    result_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The configuration for the SDK.
pub struct Config {
    /// The domain to be updated.
    pub domain: Domain,
    /// The Sequencer configuration. It contains the endpoint of the Sequencer.
    pub sequencer: Sequencer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComputeRequestResult {
    compute_tx_hash: TxHash,
    tx_event: TxEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComputeResults {
    votes: Vec<bool>,
    scores: Vec<ScoreEntry>,
}

impl ComputeRequestResult {
    pub fn new(compute_tx_hash: TxHash, tx_event: TxEvent) -> Self {
        Self { compute_tx_hash, tx_event }
    }
}

/// Creates a new `Config` from a local TOML file, given file path.
fn read_config(path: &str) -> Result<Config, Box<dyn Error>> {
    let mut f = File::open(path)?;
    let mut toml_config = String::new();
    f.read_to_string(&mut toml_config)?;
    let config: Config = toml::from_str(toml_config.as_str())?;
    Ok(config)
}

/// 1. Reads a CSV file and get a list of `TrustEntry`.
/// 2. Creates a new `Client`, which can be used to call the Sequencer.
/// 3. Sends the list of `TrustEntry` to the Sequencer.
async fn update_trust(
    sk: SigningKey, path: &str, config_path: &str,
) -> Result<Vec<TxEvent>, Box<dyn Error>> {
    let f = File::open(path)?;
    let mut rdr = csv::Reader::from_reader(f);
    let mut entries = Vec::new();
    for result in rdr.records() {
        let record: StringRecord = result?;
        let (from, to, value): (String, String, f32) = record.deserialize(None)?;
        let trust_entry = TrustEntry::new(from, to, value);
        entries.push(trust_entry);
    }

    let config = read_config(config_path)?;
    // Creates a new client
    let client = HttpClient::builder().build(config.sequencer.endpoint.as_str())?;

    let mut results = Vec::new();
    for chunk in entries.chunks(TRUST_CHUNK_SIZE) {
        let data = encode(TrustUpdate::new(
            config.domain.trust_namespace(),
            chunk.to_vec(),
        ));
        let mut tx = Tx::default_with(Kind::TrustUpdate, data);
        tx.sign(&sk)?;

        let result: TxEvent =
            client.request("sequencer_trust_update", vec![hex::encode(encode(tx))]).await?;
        results.push(result);
    }
    Ok(results)
}

/// 1. Reads a CSV file and get a list of `ScoreEntry`.
/// 2. Creates a new `Client`, which can be used to call the Sequencer.
/// 3. Sends the list of `ScoreEntry` to the Sequencer.
async fn update_seed(
    sk: SigningKey, path: &str, config_path: &str,
) -> Result<Vec<TxEvent>, Box<dyn Error>> {
    let f = File::open(path)?;
    let mut rdr = csv::Reader::from_reader(f);
    let mut entries = Vec::new();
    for result in rdr.records() {
        let record: StringRecord = result?;
        let (i, value): (String, f32) = record.deserialize(None)?;
        let score_entry = ScoreEntry::new(i, value);
        entries.push(score_entry);
    }

    let config = read_config(config_path)?;
    // Creates a new client
    let client = HttpClient::builder().build(config.sequencer.endpoint.as_str())?;

    let mut results = Vec::new();
    for chunk in entries.chunks(SEED_CHUNK_SIZE) {
        let data = encode(SeedUpdate::new(
            config.domain.seed_namespace(),
            chunk.to_vec(),
        ));
        let mut tx = Tx::default_with(Kind::SeedUpdate, data);
        tx.sign(&sk)?;

        let tx_event: TxEvent =
            client.request("sequencer_seed_update", vec![hex::encode(encode(tx))]).await?;
        results.push(tx_event);
    }
    Ok(results)
}

/// 1. Creates a new `Client`, which can be used to call the Sequencer.
/// 2. Sends a `ComputeRequest` transaction to the Sequencer.
async fn compute_request(
    sk: SigningKey, path: &str,
) -> Result<ComputeRequestResult, Box<dyn Error>> {
    let config = read_config(path)?;
    // Creates a new client
    let client = HttpClient::builder().build(config.sequencer.endpoint.as_str())?;

    let rng = &mut thread_rng();
    let domain_id = config.domain.to_hash();
    let hash = hash_leaf::<Keccak256>(rng.gen::<[u8; 32]>().to_vec());
    let data = encode(compute::Request::new(domain_id, 0, hash));
    let mut tx = Tx::default_with(Kind::ComputeRequest, data);
    tx.sign(&sk)?;
    let tx_hash = tx.hash();

    let tx_event: TxEvent =
        client.request("sequencer_compute_request", vec![hex::encode(encode(tx))]).await?;

    let compute_result = ComputeRequestResult::new(tx_hash, tx_event);
    Ok(compute_result)
}

/// 1. Creates a new `Client`, which can be used to call the Sequencer.
/// 2. Calls the Sequencer to get the EigenTrust scores(`ScoreEntry`s).
async fn get_results(
    arg: String, config_path: &str,
) -> Result<(Vec<bool>, Vec<ScoreEntry>), Box<dyn Error>> {
    let config = read_config(config_path)?;
    // Creates a new client
    let client = HttpClient::builder().build(config.sequencer.endpoint.as_str())?;
    let tx_hash_bytes = hex::decode(arg)?;
    let compute_request_tx_hash = TxHash::from_bytes(tx_hash_bytes);
    let results_query =
        GetResultsQuery::new(compute_request_tx_hash, 0, config.sequencer.result_size);
    let scores: (Vec<bool>, Vec<ScoreEntry>) =
        client.request("sequencer_get_results", vec![results_query]).await?;
    Ok(scores)
}

/// Generates a new ECDSA keypair and returns the address and the private key.
fn generate_keypair<R: CryptoRngCore>(rng: &mut R) -> (SigningKey, Address) {
    let sk = SigningKey::random(rng);
    let addr = address_from_sk(&sk);
    (sk, addr)
}

/// Checks the score integrity against the ones stored in `path`.
fn check_score_integrity(
    votes: Vec<bool>, scores: Vec<ScoreEntry>, path: &str,
) -> Result<bool, Box<dyn Error>> {
    let f = File::open(path)?;
    let mut rdr = csv::Reader::from_reader(f);
    let mut test_vector = Vec::new();
    for result in rdr.records() {
        let record: StringRecord = result?;
        let (i, value): (String, f32) = record.deserialize(None)?;
        let score_entry = ScoreEntry::new(i, value);
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

    let is_converged = is_converged_org(&test_map, &score_map);
    let votes = votes.iter().fold(true, |acc, vote| acc & vote);

    Ok(is_converged & votes)
}

/// Utility function for writing json to a file.
fn write_json_to_file<T: Serialize>(path: &str, data: T) -> Result<(), Box<dyn Error>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &data)?;
    writer.flush()?;
    Ok(())
}

/// Returns the secret key from the environment variable.
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
        Method::ComputeRequest { path, output_path } => {
            let secret_key = get_secret_key()?;
            let res = compute_request(secret_key, path.as_str()).await?;
            let hex_encoded_tx_hash = hex::encode(res.compute_tx_hash.0);
            println!("{}", hex_encoded_tx_hash);
            if let Some(output_path) = output_path {
                write_json_to_file(&output_path, res)?;
            }
        },
        Method::GetResults { request_id, config_path, output_path } => {
            let (votes, scores) = get_results(request_id, config_path.as_str()).await?;

            println!("votes: {:?}", votes);
            for res in &scores {
                println!("{}: {}", res.id, res.value);
            }
            if let Some(output_path) = output_path {
                write_json_to_file(&output_path, scores)?;
            }
        },
        Method::GetResultsAndCheckIntegrity { request_id, config_path, test_vector } => {
            let (votes, results) = get_results(request_id, config_path.as_str()).await?;
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
