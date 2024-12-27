use clap::{Parser, Subcommand};
use dotenv::dotenv;
use openrank_common::address_from_sk;
use rand::thread_rng;
use std::process::ExitCode;

use openrank_sdk::*;

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
            match get_results(request_id, config_path.as_str(), false, false).await {
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
            match get_compute_result(seq_number, &config_path).await {
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
            match get_compute_result_txs(seq_number, &config_path).await {
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
            match get_tx(tx_id, &config_path).await {
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
            match get_trust_updates(&config_path, from, size).await {
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
            match get_seed_updates(&config_path, from, size).await {
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
