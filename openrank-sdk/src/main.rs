use clap::{Parser, Subcommand};
use dotenv::dotenv;
use rand::thread_rng;
use std::process::ExitCode;

use csv::StringRecord;
use k256::{ecdsa::SigningKey, schnorr::CryptoRngCore};
use openrank_common::{
    address_from_sk,
    tx::{
        compute,
        trust::{ScoreEntry, SeedUpdate, TrustEntry, TrustUpdate},
        Address, Tx, TxHash,
    },
    tx_event::TxEvent,
};
use serde::Serialize;
use std::io::BufWriter;
use std::{fs::File, io::Write};

use openrank_sdk::*;

const TRANSIENT_STATUS_CODE: u8 = 2;

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
    /// Get TrustUpdate contents
    GetTrustUpdates {
        config_path: String,
        output_path: Option<String>,
        from: Option<String>,
        size: Option<usize>,
    },
    /// Get SeedUpdate contents
    GetSeedUpdates {
        config_path: String,
        output_path: Option<String>,
        from: Option<String>,
        size: Option<usize>,
    },
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

#[tokio::main]
async fn main() -> ExitCode {
    dotenv().ok();
    let cli = Args::parse();

    match cli.method {
        Method::TrustUpdate { path, config_path, output_path } => {
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            match update_trust(secret_key, path.as_str(), config_path.as_str()).await {
                Ok(tx_events) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, tx_events) {
                            eprintln!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::SeedUpdate { path, config_path, output_path } => {
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            match update_seed(secret_key, path.as_str(), config_path.as_str()).await {
                Ok(tx_events) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, tx_events) {
                            eprintln!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::ComputeRequest { path, output_path } => {
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            match compute_request(secret_key, path.as_str()).await {
                Ok(result) => {
                    let hex_encoded_tx_hash = hex::encode(result.compute_tx_hash().inner());
                    println!("{}", hex_encoded_tx_hash);
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, result) {
                            eprintln!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    eprintln!("{}", e);
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
            // TODO: do we really need this extra "get_secret_key" call? Atm, it is just for "OpenRankSDK::new".
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            let res = get_results(
                secret_key,
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
                            eprintln!("{}", e);
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
                        eprintln!("ComputeJob still in progress");
                        return ExitCode::from(TRANSIENT_STATUS_CODE);
                    },
                    SdkError::ComputeJobFailed(positive, total) => {
                        eprintln!(
                            "ComputeJob verification failed. Votes: {}/{}",
                            positive, total
                        );
                        return ExitCode::FAILURE;
                    },
                    e => {
                        eprintln!("{}", e);
                        return ExitCode::FAILURE;
                    },
                },
            }
        },
        Method::GetResultsAndCheckIntegrity { request_id, config_path, test_vector } => {
            // TODO: do we really need this extra "get_secret_key" call? Atm, it is just for "OpenRankSDK::new".
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            match get_results(secret_key, request_id, config_path.as_str(), false, false).await {
                Ok((votes, results)) => {
                    let res = match check_score_integrity(votes, results, &test_vector) {
                        Ok(res) => res,
                        Err(e) => {
                            eprintln!("{}", e);
                            return ExitCode::FAILURE;
                        },
                    };
                    println!("Integrity check result: {}", res);
                    assert!(res);
                },
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::GetComputeResult { seq_number, config_path, output_path } => {
            // TODO: do we really need this extra "get_secret_key" call? Atm, it is just for "OpenRankSDK::new".
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            match get_compute_result(secret_key, seq_number, &config_path).await {
                Ok(result) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, result) {
                            eprintln!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::GetComputeResultTxs { seq_number, config_path, output_path } => {
            // TODO: do we really need this extra "get_secret_key" call? Atm, it is just for "OpenRankSDK::new".
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            match get_compute_result_txs(secret_key, seq_number, &config_path).await {
                Ok(txs) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, txs) {
                            eprintln!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::GetTx { tx_id, config_path, output_path } => {
            // TODO: do we really need this extra "get_secret_key" call? Atm, it is just for "OpenRankSDK::new".
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            match get_tx(secret_key, tx_id, &config_path).await {
                Ok(tx) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, tx) {
                            eprintln!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::GetTrustUpdates { from, size, config_path, output_path } => {
            // TODO: do we really need this extra "get_secret_key" call? Atm, it is just for "OpenRankSDK::new".
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            match get_trust_updates(secret_key, &config_path, from, size).await {
                Ok(res) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, res) {
                            eprintln!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            }
        },
        Method::GetSeedUpdates { from, size, config_path, output_path } => {
            // TODO: do we really need this extra "get_secret_key" call? Atm, it is just for "OpenRankSDK::new".
            let secret_key = match get_secret_key() {
                Ok(secret_key) => secret_key,
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::FAILURE;
                },
            };
            match get_seed_updates(secret_key, &config_path, from, size).await {
                Ok(res) => {
                    if let Some(output_path) = output_path {
                        if let Err(e) = write_json_to_file(&output_path, res) {
                            eprintln!("{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                },
                Err(e) => {
                    eprintln!("{}", e);
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
                eprintln!("{}", e);
                return ExitCode::FAILURE;
            },
        },
    }

    ExitCode::SUCCESS
}

/// 1. Reads a CSV file and get a list of `TrustEntry`.
/// 2. Call the `OpenRankSDK::trust_update` func
async fn update_trust(
    sk: SigningKey, path: &str, config_path: &str,
) -> Result<Vec<TxEvent>, SdkError> {
    // Read CSV, to get a list of `TrustEntry`
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

    // Read config
    let config = read_config(config_path)?;

    // Create SDK & send trust updates
    let sdk = OpenRankSDK::new(sk, config)?;
    let results = sdk.trust_update(&entries).await?;

    Ok(results)
}

/// 1. Reads a CSV file and get a list of `ScoreEntry`.
/// 2. Call the `OpenRankSDK::seed_update` func
async fn update_seed(
    sk: SigningKey, path: &str, config_path: &str,
) -> Result<Vec<TxEvent>, SdkError> {
    // Read CSV, to get a list of `ScoreEntry`
    let f = File::open(path).map_err(SdkError::IoError)?;
    let mut rdr = csv::Reader::from_reader(f);
    let mut entries = Vec::new();
    for result in rdr.records() {
        let record: StringRecord = result.map_err(SdkError::CsvError)?;
        let (i, value): (String, f32) = record.deserialize(None).map_err(SdkError::CsvError)?;
        let score_entry = ScoreEntry::new(i, value);
        entries.push(score_entry);
    }

    // Read config
    let config = read_config(config_path)?;

    // Create SDK & send seed updates
    let sdk = OpenRankSDK::new(sk, config)?;
    let results = sdk.seed_update(&entries).await?;

    Ok(results)
}

/// Call the `OpenRankSDK::compute_request` func
async fn compute_request(
    sk: SigningKey, config_path: &str,
) -> Result<ComputeRequestResult, SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Create SDK & send compute request
    let sdk = OpenRankSDK::new(sk, config)?;
    let result = sdk.compute_request().await?;

    Ok(result)
}

/// Parse the `arg` & call the `OpenRankSDK::get_results` func
async fn get_results(
    sk: SigningKey, arg: String, config_path: &str, allow_incomplete: bool, allow_failed: bool,
) -> Result<(Vec<bool>, Vec<ScoreEntry>), SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Decoding the ComputeRequest TX hash
    let tx_hash_bytes = hex::decode(arg).map_err(SdkError::HexError)?;
    let compute_request_tx_hash = TxHash::from_bytes(tx_hash_bytes);

    // Create SDK & get results
    let sdk = OpenRankSDK::new(sk, config)?;
    let result = sdk.get_results(compute_request_tx_hash, allow_incomplete, allow_failed).await?;

    Ok((result.votes, result.scores))
}

/// Parse the `arg` & call the `OpenRankSDK::get_compute_result` func
async fn get_compute_result(
    sk: SigningKey, arg: String, config_path: &str,
) -> Result<compute::Result, SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Decoding the sequence number
    let seq_number = arg.parse::<u64>().map_err(SdkError::ParseIntError)?;

    // Create SDK & get results
    let sdk = OpenRankSDK::new(sk, config)?;
    let result = sdk.get_compute_result(seq_number).await?;

    Ok(result)
}

/// Parse the `arg` & call the `OpenRankSDK::get_compute_result_txs` func
async fn get_compute_result_txs(
    sk: SigningKey, arg: String, config_path: &str,
) -> Result<Vec<Tx>, SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Decoding the sequence number
    let seq_number = arg.parse::<u64>().map_err(SdkError::ParseIntError)?;

    // Create SDK & get results
    let sdk = OpenRankSDK::new(sk, config)?;
    let result = sdk.get_compute_result_txs(seq_number).await?;

    Ok(result)
}

/// Parse the `arg` & call the `OpenRankSDK::get_tx` func
async fn get_tx(sk: SigningKey, arg: String, config_path: &str) -> Result<Tx, SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Parse arg, to get the prefix and TX hash
    let arg_clone = arg.clone();
    let (prefix, tx_hash) = arg.split_once(':').ok_or(SdkError::ArgParseError(arg_clone))?;

    // Create SDK & get results
    let sdk = OpenRankSDK::new(sk, config)?;
    let result = sdk.get_tx(prefix, tx_hash).await?;

    Ok(result)
}

/// Checks the score integrity against the ones stored in `path`.
pub fn check_score_integrity(
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

    OpenRankSDK::check_score_integrity(votes, scores, test_vector)
}

/// Call the `OpenRankSDK::get_trust_updates` func
async fn get_trust_updates(
    sk: SigningKey, config_path: &str, from: Option<String>, size: Option<usize>,
) -> Result<Vec<TrustUpdate>, SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Create SDK & get results
    let sdk = OpenRankSDK::new(sk, config)?;
    let result = sdk.get_trust_updates(from, size).await?;

    Ok(result)
}

/// Call the `OpenRankSDK::get_seed_updates` func
async fn get_seed_updates(
    sk: SigningKey, config_path: &str, from: Option<String>, size: Option<usize>,
) -> Result<Vec<SeedUpdate>, SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Create SDK & get results
    let sdk = OpenRankSDK::new(sk, config)?;
    let result = sdk.get_seed_updates(from, size).await?;

    Ok(result)
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

/// Generates a new ECDSA keypair and returns the address and the private key.
fn generate_keypair<R: CryptoRngCore>(rng: &mut R) -> (SigningKey, Address) {
    let sk = SigningKey::random(rng);
    let addr = address_from_sk(&sk);
    (sk, addr)
}
