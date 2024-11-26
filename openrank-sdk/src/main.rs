use alloy_rlp::encode;
use clap::{Parser, Subcommand};
use csv::StringRecord;
use dotenv::dotenv;
use getset::Getters;
use jsonrpsee::{core::client::ClientT, http_client::HttpClient};
use k256::{ecdsa::Error as EcdsaError, ecdsa::SigningKey, schnorr::CryptoRngCore};
use openrank_common::{
    address_from_sk,
    algos::et::is_converged_org,
    merkle::hash_leaf,
    topics::Domain,
    tx::{
        self,
        compute::{self, Verification},
        consts,
        trust::{ScoreEntry, SeedUpdate, TrustEntry, TrustUpdate},
        Address, Body, Tx, TxHash,
    },
    tx_event::TxEvent,
};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use sha3::Keccak256;
use std::{
    cmp::Ordering,
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    process::ExitCode,
};
use std::{
    io::{self, BufWriter},
    num,
};

const TRUST_CHUNK_SIZE: usize = 500;
const SEED_CHUNK_SIZE: usize = 1000;
const NOT_FOUND_CODE: i32 = -32016;
const TRANSIENT_STATUS_CODE: u8 = 2;

#[derive(thiserror::Error, Debug)]
pub enum SdkError {
    #[error("IO error: {0}")]
    IoError(io::Error),
    #[error("Toml error: {0}")]
    TomlError(toml::de::Error),
    #[error("Csv error: {0}")]
    CsvError(csv::Error),
    #[error("JsonRPC client error: {0}")]
    JsonRpcClientError(jsonrpsee::core::ClientError),
    #[error("Ecdsa error: {0}")]
    EcdsaError(EcdsaError),
    #[error("Hex error: {0}")]
    HexError(hex::FromHexError),
    #[error("Parse int error: {0}")]
    ParseIntError(num::ParseIntError),
    #[error("Invalid TX kind")]
    InvalidTxKind,
    #[error("Arg parse error: {0}")]
    ArgParseError(String),
    #[error("Serde error: {0}")]
    SerdeError(serde_json::Error),
    #[error("Incomplete result: expected {0} verifications, got {1}")]
    IncompleteResult(usize, usize),
    #[error("Compute job verification failed: {0}/{1}")]
    ComputeJobFailed(usize, usize),
    #[error("Compute job still in progress.")]
    ComputeJobInProgress,
}

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
    GetResults {
        request_id: String,
        config_path: String,
        output_path: Option<String>,
        #[arg(long)]
        allow_incomplete: bool,
        #[arg(long)]
        allow_failed: bool,
    },
    /// The method takes a ComputeRequest TX hash and returns the computed results,
    /// and also checks the integrity/correctness of the results.
    GetResultsAndCheckIntegrity { request_id: String, config_path: String, test_vector: String },
    /// Get ComputeResult TXs
    GetComputeResult { seq_number: String, config_path: String, output_path: Option<String> },
    /// Get TXs included in ComputeResult
    GetComputeResultTxs { seq_number: String, config_path: String, output_path: Option<String> },
    /// Get arbitrary TX
    GetTx { tx_id: String, config_path: String, output_path: Option<String> },
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

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// The configuration for the Sequencer.
pub struct Sequencer {
    endpoint: String,
    result_start: Option<u32>,
    result_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// The configuration for the SDK.
pub struct Config {
    /// The domain to be updated.
    domain: Domain,
    /// The Sequencer configuration. It contains the endpoint of the Sequencer.
    sequencer: Sequencer,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
struct ComputeRequestResult {
    compute_tx_hash: TxHash,
    tx_event: TxEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
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
fn read_config(path: &str) -> Result<Config, SdkError> {
    let mut f = File::open(path).map_err(SdkError::IoError)?;
    let mut toml_config = String::new();
    f.read_to_string(&mut toml_config).map_err(SdkError::IoError)?;
    let config: Config = toml::from_str(toml_config.as_str()).map_err(SdkError::TomlError)?;
    Ok(config)
}

/// 1. Reads a CSV file and get a list of `TrustEntry`.
/// 2. Creates a new `Client`, which can be used to call the Sequencer.
/// 3. Sends the list of `TrustEntry` to the Sequencer.
async fn update_trust(
    sk: SigningKey, path: &str, config_path: &str,
) -> Result<Vec<TxEvent>, SdkError> {
    let f = File::open(path).map_err(SdkError::IoError)?;
    let mut rdr = csv::Reader::from_reader(f);
    let mut entries = Vec::new();
    for result in rdr.records() {
        let record: StringRecord = result.map_err(SdkError::CsvError)?;
        let (from, to, value): (String, String, f32) =
            record.deserialize(None).map_err(SdkError::CsvError)?;
        let trust_entry = TrustEntry::new(from, to, value);
        entries.push(trust_entry);
    }

    let config = read_config(config_path)?;
    // Creates a new client
    let client = HttpClient::builder()
        .build(config.sequencer.endpoint.as_str())
        .map_err(SdkError::JsonRpcClientError)?;

    let mut results = Vec::new();
    for chunk in entries.chunks(TRUST_CHUNK_SIZE) {
        let mut tx = Tx::default_with(Body::TrustUpdate(TrustUpdate::new(
            config.domain.trust_namespace(),
            chunk.to_vec(),
        )));
        tx.sign(&sk).map_err(SdkError::EcdsaError)?;

        let result: TxEvent = client
            .request("sequencer_trust_update", vec![hex::encode(encode(tx))])
            .await
            .map_err(SdkError::JsonRpcClientError)?;
        results.push(result);
    }
    Ok(results)
}

/// 1. Reads a CSV file and get a list of `ScoreEntry`.
/// 2. Creates a new `Client`, which can be used to call the Sequencer.
/// 3. Sends the list of `ScoreEntry` to the Sequencer.
async fn update_seed(
    sk: SigningKey, path: &str, config_path: &str,
) -> Result<Vec<TxEvent>, SdkError> {
    let f = File::open(path).map_err(SdkError::IoError)?;
    let mut rdr = csv::Reader::from_reader(f);
    let mut entries = Vec::new();
    for result in rdr.records() {
        let record: StringRecord = result.map_err(SdkError::CsvError)?;
        let (i, value): (String, f32) = record.deserialize(None).map_err(SdkError::CsvError)?;
        let score_entry = ScoreEntry::new(i, value);
        entries.push(score_entry);
    }

    let config = read_config(config_path)?;
    // Creates a new client
    let client = HttpClient::builder()
        .build(config.sequencer.endpoint.as_str())
        .map_err(SdkError::JsonRpcClientError)?;

    let mut results = Vec::new();
    for chunk in entries.chunks(SEED_CHUNK_SIZE) {
        let mut tx = Tx::default_with(Body::SeedUpdate(SeedUpdate::new(
            config.domain.seed_namespace(),
            chunk.to_vec(),
        )));
        tx.sign(&sk).map_err(SdkError::EcdsaError)?;

        let tx_event: TxEvent = client
            .request("sequencer_seed_update", vec![hex::encode(encode(tx))])
            .await
            .map_err(SdkError::JsonRpcClientError)?;
        results.push(tx_event);
    }
    Ok(results)
}

/// 1. Creates a new `Client`, which can be used to call the Sequencer.
/// 2. Sends a `ComputeRequest` transaction to the Sequencer.
async fn compute_request(sk: SigningKey, path: &str) -> Result<ComputeRequestResult, SdkError> {
    let config = read_config(path)?;
    // Creates a new client
    let client = HttpClient::builder()
        .build(config.sequencer.endpoint.as_str())
        .map_err(SdkError::JsonRpcClientError)?;

    let rng = &mut thread_rng();
    let domain_id = config.domain.to_hash();
    let hash = hash_leaf::<Keccak256>(rng.gen::<[u8; 32]>().to_vec());
    let mut tx = Tx::default_with(Body::ComputeRequest(compute::Request::new(
        domain_id, 0, hash,
    )));
    tx.sign(&sk).map_err(SdkError::EcdsaError)?;
    let tx_hash = tx.hash();

    let tx_event: TxEvent = client
        .request("sequencer_compute_request", vec![hex::encode(encode(tx))])
        .await
        .map_err(SdkError::JsonRpcClientError)?;

    let compute_result = ComputeRequestResult::new(tx_hash, tx_event);
    Ok(compute_result)
}

fn is_verification_passed(ver: Vec<Verification>) -> (bool, usize) {
    let mut passed = true;
    let mut count = 0;
    for v in ver {
        if !v.verification_result() {
            passed = false;
        } else {
            count += 1;
        }
    }
    (passed, count)
}

/// 1. Creates a new `Client`, which can be used to call the Sequencer.
/// 2. Calls the Sequencer to get the EigenTrust scores(`ScoreEntry`s).
async fn get_results(
    arg: String, config_path: &str, allow_incomplete: bool, allow_failed: bool,
) -> Result<(Vec<bool>, Vec<ScoreEntry>), SdkError> {
    let config = read_config(config_path)?;
    // Creates a new client
    let client = HttpClient::builder()
        .build(config.sequencer.endpoint.as_str())
        .map_err(SdkError::JsonRpcClientError)?;
    let tx_hash_bytes = hex::decode(arg).map_err(SdkError::HexError)?;
    let compute_request_tx_hash = TxHash::from_bytes(tx_hash_bytes);
    // Checking if ComputeRequest exists, if not, it should return an error
    let compute_request_key = (consts::COMPUTE_REQUEST, compute_request_tx_hash.clone());
    let _: Tx = client
        .request("sequencer_get_tx", compute_request_key)
        .await
        .map_err(SdkError::JsonRpcClientError)?;

    // If ComputeRequest exists, we can proceed with fetching results
    let res: Result<u64, SdkError> = client
        .request(
            "sequencer_get_compute_result_seq_number",
            vec![compute_request_tx_hash.clone()],
        )
        .await
        .map_err(SdkError::JsonRpcClientError);
    let seq_number = match res {
        Ok(res) => Ok(res),
        Err(e) => match e {
            SdkError::JsonRpcClientError(jsonrpsee::core::ClientError::Call(call_e)) => {
                if call_e.code() == NOT_FOUND_CODE {
                    Err(SdkError::ComputeJobInProgress)
                } else {
                    Err(SdkError::JsonRpcClientError(
                        jsonrpsee::core::ClientError::Call(call_e),
                    ))
                }
            },
            e => Err(e),
        },
    }?;

    let result: compute::Result = client
        .request("sequencer_get_compute_result", vec![seq_number])
        .await
        .map_err(SdkError::JsonRpcClientError)?;

    let commitment_tx_key = (
        consts::COMPUTE_COMMITMENT,
        result.compute_commitment_tx_hash().clone(),
    );
    let commitment_tx: Tx = client
        .request("sequencer_get_tx", commitment_tx_key)
        .await
        .map_err(SdkError::JsonRpcClientError)?;
    let commitment = match commitment_tx.body() {
        tx::Body::ComputeCommitment(commitment) => Ok(commitment),
        _ => Err(SdkError::InvalidTxKind),
    }?;
    let mut scores_tx_keys = Vec::new();
    for scores_tx_hash in commitment.scores_tx_hashes() {
        scores_tx_keys.push((consts::COMPUTE_SCORES, scores_tx_hash));
    }
    let scores_txs: Vec<Tx> = client
        .request("sequencer_get_txs", vec![scores_tx_keys])
        .await
        .map_err(SdkError::JsonRpcClientError)?;
    let mut score_entries = Vec::new();
    for scores_tx in &scores_txs {
        let scores = match scores_tx.body() {
            tx::Body::ComputeScores(scores) => Ok(scores),
            _ => Err(SdkError::InvalidTxKind),
        }?;
        score_entries.extend(scores.entries().clone());
    }

    score_entries.sort_by(|a, b| match a.value().partial_cmp(b.value()) {
        Some(ordering) => ordering,
        None => {
            if a.value().is_nan() && b.value().is_nan() {
                Ordering::Equal
            } else if a.value().is_nan() {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        },
    });
    score_entries.reverse();

    let assignment_key = (consts::COMPUTE_ASSIGNMENT, commitment.assignment_tx_hash());
    let assignment_tx: Tx = client
        .request("sequencer_get_tx", assignment_key)
        .await
        .map_err(SdkError::JsonRpcClientError)?;
    let _assignment = match assignment_tx.body() {
        tx::Body::ComputeAssignment(assignment) => Ok(assignment),
        _ => Err(SdkError::InvalidTxKind),
    }?;

    // let num_assigned_verifiers = assignment.assigned_verifier_node().len();
    let num_assigned_verifiers = 1;
    let verification_tx_hashes = result.compute_verification_tx_hashes();
    if (verification_tx_hashes.len() < num_assigned_verifiers) & !allow_incomplete {
        return Err(SdkError::IncompleteResult(
            num_assigned_verifiers,
            verification_tx_hashes.len(),
        ));
    }

    let mut verification_tx_keys = Vec::new();
    for verification_tx_hash in verification_tx_hashes {
        verification_tx_keys.push((consts::COMPUTE_VERIFICATION, verification_tx_hash));
    }
    let verification_txs: Vec<Tx> = client
        .request("sequencer_get_txs", vec![verification_tx_keys])
        .await
        .map_err(SdkError::JsonRpcClientError)?;

    let mut verification_results = Vec::new();
    for verification_tx in &verification_txs {
        let res = match verification_tx.body() {
            tx::Body::ComputeVerification(res) => Ok(res),
            _ => Err(SdkError::InvalidTxKind),
        }?;
        verification_results.push(res.clone());
    }

    let (verification_result, count_positive) =
        is_verification_passed(verification_results.clone());
    if !verification_result & !allow_failed {
        return Err(SdkError::ComputeJobFailed(
            count_positive,
            verification_tx_hashes.len(),
        ));
    }

    let votes = verification_results.iter().map(|x| *x.verification_result()).collect();
    Ok((votes, score_entries))
}

/// 1. Creates a new `Client`, which can be used to call the Sequencer.
/// 2. Calls the Sequencer to get the results of the compute that contains references to compute hashes.
async fn get_compute_result(arg: String, config_path: &str) -> Result<compute::Result, SdkError> {
    let config = read_config(config_path)?;
    // Creates a new client
    let client = HttpClient::builder()
        .build(config.sequencer.endpoint.as_str())
        .map_err(SdkError::JsonRpcClientError)?;
    let seq_number = arg.parse::<u64>().map_err(SdkError::ParseIntError)?;
    let result: compute::Result = client
        .request("sequencer_get_compute_result", vec![seq_number])
        .await
        .map_err(SdkError::JsonRpcClientError)?;
    Ok(result)
}

/// 1. Creates a new `Client`, which can be used to call the Sequencer.
/// 2. Calls the Sequencer to get all the txs that are included inside a specific compute result.
async fn get_compute_result_txs(arg: String, config_path: &str) -> Result<Vec<Tx>, SdkError> {
    let config = read_config(config_path)?;
    // Creates a new client
    let client = HttpClient::builder()
        .build(config.sequencer.endpoint.as_str())
        .map_err(SdkError::JsonRpcClientError)?;
    let seq_number = arg.parse::<u64>().map_err(SdkError::ParseIntError)?;
    let result: compute::Result = client
        .request("sequencer_get_compute_result", vec![seq_number])
        .await
        .map_err(SdkError::JsonRpcClientError)?;

    let mut txs_arg = Vec::new();
    txs_arg.push((
        consts::COMPUTE_REQUEST,
        result.compute_request_tx_hash().clone(),
    ));
    txs_arg.push((
        consts::COMPUTE_COMMITMENT,
        result.compute_commitment_tx_hash().clone(),
    ));
    for verification_tx_hash in result.compute_verification_tx_hashes() {
        txs_arg.push((consts::COMPUTE_VERIFICATION, verification_tx_hash.clone()));
    }

    let txs_res = client
        .request("sequencer_get_txs", vec![txs_arg])
        .await
        .map_err(SdkError::JsonRpcClientError)?;
    Ok(txs_res)
}

/// 1. Creates a new `Client`, which can be used to call the Sequencer.
/// 2. Calls the Sequencer to get the TX given a TX hash.
async fn get_tx(arg: String, config_path: &str) -> Result<Tx, SdkError> {
    let config = read_config(config_path)?;
    // Creates a new client
    let client = HttpClient::builder()
        .build(config.sequencer.endpoint.as_str())
        .map_err(SdkError::JsonRpcClientError)?;
    let arg_clone = arg.clone();
    let (prefix, tx_hash) = arg.split_once(':').ok_or(SdkError::ArgParseError(arg_clone))?;
    let tx_hash_bytes = hex::decode(tx_hash).map_err(SdkError::HexError)?;
    let tx_hash = TxHash::from_bytes(tx_hash_bytes);
    let tx: Tx = client
        .request("sequencer_get_tx", (prefix.to_string(), tx_hash))
        .await
        .map_err(SdkError::JsonRpcClientError)?;
    Ok(tx)
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
) -> Result<bool, SdkError> {
    let f = File::open(path).map_err(SdkError::IoError)?;
    let mut rdr = csv::Reader::from_reader(f);
    let mut test_vector = Vec::new();
    for result in rdr.records() {
        let record: StringRecord = result.map_err(SdkError::CsvError)?;
        let (i, value): (String, f32) = record.deserialize(None).map_err(SdkError::CsvError)?;
        let score_entry = ScoreEntry::new(i, value);
        test_vector.push(score_entry);
    }

    let mut test_map = HashMap::new();
    for score in test_vector {
        test_map.insert(score.id().clone(), *score.value());
    }

    let mut score_map = HashMap::new();
    for score in scores {
        score_map.insert(score.id().clone(), *score.value());
    }

    let is_converged = is_converged_org(&test_map, &score_map);
    let votes = votes.iter().fold(true, |acc, vote| acc & vote);

    Ok(is_converged & votes)
}

/// Utility function for writing json to a file.
fn write_json_to_file<T: Serialize>(path: &str, data: T) -> Result<(), SdkError> {
    let file = File::create(path).map_err(SdkError::IoError)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &data).map_err(SdkError::SerdeError)?;
    writer.flush().map_err(SdkError::IoError)?;
    Ok(())
}

/// Returns the secret key from the environment variable.
fn get_secret_key() -> Result<SigningKey, SdkError> {
    let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
    let secret_key_bytes = hex::decode(secret_key_hex).map_err(SdkError::HexError)?;
    let secret_key =
        SigningKey::from_slice(secret_key_bytes.as_slice()).map_err(SdkError::EcdsaError)?;
    Ok(secret_key)
}

#[tokio::main]
async fn main() -> ExitCode {
    dotenv().ok();
    let cli = Args::parse();

    match cli.method {
        Method::TrustUpdate { path, config_path, output_path } => {
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    println!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            match update_trust(secret_key, path.as_str(), config_path.as_str()).await {
                Ok(tx_events) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, tx_events) {
                            println!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    println!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::SeedUpdate { path, config_path, output_path } => {
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    println!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            match update_seed(secret_key, path.as_str(), config_path.as_str()).await {
                Ok(tx_events) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, tx_events) {
                            println!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    println!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::ComputeRequest { path, output_path } => {
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    println!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            match compute_request(secret_key, path.as_str()).await {
                Ok(result) => {
                    let hex_encoded_tx_hash = hex::encode(result.compute_tx_hash.inner());
                    println!("{}", hex_encoded_tx_hash);
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, result) {
                            println!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    println!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::GetResults {
            request_id,
            config_path,
            output_path,
            allow_incomplete,
            allow_failed,
        } => {
            let res = get_results(
                request_id,
                config_path.as_str(),
                allow_incomplete,
                allow_failed,
            )
            .await;
            match res {
                Ok((votes, scores)) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, scores) {
                            println!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    } else {
                        println!("votes: {:?}", votes);
                        for res in &scores {
                            println!("{}: {}", res.id().clone(), *res.value());
                        }
                    }
                },
                Err(e) => match e {
                    SdkError::ComputeJobInProgress => {
                        println!("ComputeJob still in progress");
                        return ExitCode::from(TRANSIENT_STATUS_CODE);
                    },
                    SdkError::ComputeJobFailed(positive, total) => {
                        println!(
                            "ComputeJob verification failed. Votes: {}/{}",
                            positive, total
                        );
                        return ExitCode::FAILURE;
                    },
                    e => {
                        println!("{}", e);
                        return ExitCode::FAILURE;
                    },
                },
            }
        },
        Method::GetResultsAndCheckIntegrity { request_id, config_path, test_vector } => {
            match get_results(request_id, config_path.as_str(), false, false).await {
                Ok((votes, results)) => {
                    let res = match check_score_integrity(votes, results, &test_vector) {
                        Ok(res) => res,
                        Err(e) => {
                            println!("{}", e);
                            return ExitCode::FAILURE;
                        },
                    };
                    println!("Integrity check result: {}", res);
                    assert!(res);
                },
                Err(e) => {
                    println!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::GetComputeResult { seq_number, config_path, output_path } => {
            match get_compute_result(seq_number, &config_path).await {
                Ok(result) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, result) {
                            println!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    println!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::GetComputeResultTxs { seq_number, config_path, output_path } => {
            match get_compute_result_txs(seq_number, &config_path).await {
                Ok(txs) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, txs) {
                            println!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    println!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::GetTx { tx_id, config_path, output_path } => {
            match get_tx(tx_id, &config_path).await {
                Ok(tx) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, tx) {
                            println!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    println!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::GenerateKeypair => {
            let rng = &mut thread_rng();
            let (sk, address) = generate_keypair(rng);
            let sk_bytes = sk.to_bytes();
            println!("SECRET_KEY=\"{}\"", hex::encode(sk_bytes));
            println!("# ADDRESS: {}", address);
        },
        Method::ShowAddress => match get_secret_key() {
            Ok(secret_key) => {
                let addr = address_from_sk(&secret_key);
                println!("ADDRESS: {}", addr);
            },
            Err(e) => {
                println!("{}", e);
                return ExitCode::FAILURE;
            },
        },
    }

    ExitCode::SUCCESS
}
