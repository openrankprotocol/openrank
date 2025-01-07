use alloy_rlp::encode;
use csv::StringRecord;
use getset::Getters;
use jsonrpsee::{core::client::ClientT, http_client::HttpClient};
use k256::{
    ecdsa::{Error as EcdsaError, SigningKey},
    schnorr::CryptoRngCore,
};
use openrank_common::{
    address_from_sk,
    algos::et::is_converged_org,
    merkle::hash_leaf,
    query::{GetSeedUpdateQuery, GetTrustUpdateQuery},
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
};
use std::{
    io::{self, BufWriter},
    num,
};

const TRUST_CHUNK_SIZE: usize = 500;
const SEED_CHUNK_SIZE: usize = 1000;
const NOT_FOUND_CODE: i32 = -32016;

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// The configuration for the SDK.
pub struct Config {
    /// The domain to be updated.
    domain: Domain,
    /// The Sequencer configuration. It contains the endpoint of the Sequencer.
    sequencer: Sequencer,
}

/// Creates a new `Config` from a local TOML file, given file path.
fn read_config(path: &str) -> Result<Config, SdkError> {
    let mut f = File::open(path).map_err(SdkError::IoError)?;
    let mut toml_config = String::new();
    f.read_to_string(&mut toml_config).map_err(SdkError::IoError)?;
    let config: Config = toml::from_str(toml_config.as_str()).map_err(SdkError::TomlError)?;
    if config.sequencer.result_start.is_some() {
        eprintln!(
            "'sequencer.result_start' is depricated. This will become a hard error in the future."
        );
    }
    if config.sequencer.result_size.is_some() {
        eprintln!(
            "'sequencer.result_size' is depricated. This will become a hard error in the future."
        );
    }
    Ok(config)
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// The configuration for the Sequencer.
pub struct Sequencer {
    endpoint: String,
    result_start: Option<u32>,
    result_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct ComputeRequestResult {
    compute_tx_hash: TxHash,
    tx_event: TxEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
struct ComputeResult {
    votes: Vec<bool>,
    scores: Vec<ScoreEntry>,
}

impl ComputeRequestResult {
    pub fn new(compute_tx_hash: TxHash, tx_event: TxEvent) -> Self {
        Self { compute_tx_hash, tx_event }
    }
}

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

struct OpenRankSDK {
    secret_key: SigningKey,
    config: Config,
}

impl OpenRankSDK {
    pub fn new(secret_key: SigningKey, config: Config) -> Self {
        Self { secret_key, config }
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Sends the list of `TrustEntry` to the Sequencer.
    pub async fn trust_update(
        &self, trust_entries: &[TrustEntry],
    ) -> Result<Vec<TxEvent>, SdkError> {
        let client = HttpClient::builder()
            .build(self.config.sequencer.endpoint.as_str())
            .map_err(SdkError::JsonRpcClientError)?;

        let mut results = Vec::new();
        for chunk in trust_entries.chunks(TRUST_CHUNK_SIZE) {
            let mut tx = Tx::default_with(Body::TrustUpdate(TrustUpdate::new(
                self.config.domain.trust_namespace(),
                chunk.to_vec(),
            )));
            tx.sign(&self.secret_key).map_err(SdkError::EcdsaError)?;

            let result: TxEvent = client
                .request("sequencer_trust_update", vec![hex::encode(encode(tx))])
                .await
                .map_err(SdkError::JsonRpcClientError)?;
            results.push(result);
        }
        Ok(results)
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Sends the list of `ScoreEntry` to the Sequencer.
    pub async fn seed_update(&self, seed_entries: &[ScoreEntry]) -> Result<Vec<TxEvent>, SdkError> {
        let client = HttpClient::builder()
            .build(self.config.sequencer.endpoint.as_str())
            .map_err(SdkError::JsonRpcClientError)?;

        let mut results = Vec::new();
        for chunk in seed_entries.chunks(SEED_CHUNK_SIZE) {
            let mut tx = Tx::default_with(Body::SeedUpdate(SeedUpdate::new(
                self.config.domain.seed_namespace(),
                chunk.to_vec(),
            )));
            tx.sign(&self.secret_key).map_err(SdkError::EcdsaError)?;

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
    pub async fn compute_request(&self) -> Result<ComputeRequestResult, SdkError> {
        let client = HttpClient::builder()
            .build(self.config.sequencer.endpoint.as_str())
            .map_err(SdkError::JsonRpcClientError)?;

        let rng = &mut thread_rng();
        let domain_id = self.config.domain.to_hash();
        let hash = hash_leaf::<Keccak256>(rng.gen::<[u8; 32]>().to_vec());
        let mut tx = Tx::default_with(Body::ComputeRequest(compute::Request::new(
            domain_id, 0, hash,
        )));
        tx.sign(&self.secret_key).map_err(SdkError::EcdsaError)?;
        let tx_hash = tx.hash();

        let tx_event: TxEvent = client
            .request("sequencer_compute_request", vec![hex::encode(encode(tx))])
            .await
            .map_err(SdkError::JsonRpcClientError)?;

        let compute_result = ComputeRequestResult::new(tx_hash, tx_event);
        Ok(compute_result)
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Calls the Sequencer to get the EigenTrust scores(`ScoreEntry`s).
    pub async fn get_results(
        &self, compute_request_tx_hash: TxHash, allow_incomplete: bool, allow_failed: bool,
    ) -> Result<ComputeResult, SdkError> {
        let client = HttpClient::builder()
            .build(self.config.sequencer.endpoint.as_str())
            .map_err(SdkError::JsonRpcClientError)?;
        
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
        Ok(ComputeResult { votes, scores: score_entries })
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Calls the Sequencer to get the results of the compute that contains references to compute hashes.
    pub async fn get_compute_result(&self, seq_number: u64) -> Result<compute::Result, SdkError> {
        let client = HttpClient::builder()
            .build(self.config.sequencer.endpoint.as_str())
            .map_err(SdkError::JsonRpcClientError)?;
        let result: compute::Result = client
            .request("sequencer_get_compute_result", vec![seq_number])
            .await
            .map_err(SdkError::JsonRpcClientError)?;
        Ok(result)
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Calls the Sequencer to get all the txs that are included inside a specific compute result.
    pub async fn get_compute_result_txs(&self, seq_number: u64) -> Result<Vec<Tx>, SdkError> {
        let client = HttpClient::builder()
            .build(self.config.sequencer.endpoint.as_str())
            .map_err(SdkError::JsonRpcClientError)?;
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
    pub async fn get_tx(&self, prefix: &str, tx_hash: &str) -> Result<Tx, SdkError> {
        let client = HttpClient::builder()
            .build(self.config.sequencer.endpoint.as_str())
            .map_err(SdkError::JsonRpcClientError)?;
        let tx_hash_bytes = hex::decode(tx_hash).map_err(SdkError::HexError)?;
        let tx_hash = TxHash::from_bytes(tx_hash_bytes);
        let tx: Tx = client
            .request("sequencer_get_tx", (prefix.to_string(), tx_hash))
            .await
            .map_err(SdkError::JsonRpcClientError)?;
        Ok(tx)
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Calls the Sequencer to get the `TrustUpdate`s.
    pub async fn get_trust_updates(
        &self, from: Option<String>, size: Option<usize>,
    ) -> Result<Vec<TrustUpdate>, SdkError> {
        let from = if let Some(data) = from {
            let tx_hash_bytes = hex::decode(data).map_err(SdkError::HexError)?;
            let trust_update_tx_hash = TxHash::from_bytes(tx_hash_bytes);
            Some(trust_update_tx_hash)
        } else {
            None
        };
        let client = HttpClient::builder()
            .build(self.config.sequencer.endpoint.as_str())
            .map_err(SdkError::JsonRpcClientError)?;
        let results_query = GetTrustUpdateQuery::new(from, size);
        let trust_updates: Vec<TrustUpdate> = client
            .request("sequencer_get_trust_updates", vec![results_query])
            .await
            .map_err(SdkError::JsonRpcClientError)?;
        Ok(trust_updates)
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Calls the Sequencer to get the `SeedUpdate`s.
    pub async fn get_seed_updates(
        &self, from: Option<String>, size: Option<usize>,
    ) -> Result<Vec<SeedUpdate>, SdkError> {
        let from = if let Some(data) = from {
            let tx_hash_bytes = hex::decode(data).map_err(SdkError::HexError)?;
            let seed_update_tx_hash = TxHash::from_bytes(tx_hash_bytes);
            Some(seed_update_tx_hash)
        } else {
            None
        };

        let client = HttpClient::builder()
            .build(self.config.sequencer.endpoint.as_str())
            .map_err(SdkError::JsonRpcClientError)?;
        let results_query = GetSeedUpdateQuery::new(from, size);
        let seed_updates: Vec<SeedUpdate> = client
            .request("sequencer_get_seed_updates", vec![results_query])
            .await
            .map_err(SdkError::JsonRpcClientError)?;
        Ok(seed_updates)
    }

    pub fn check_score_integrity(
        votes: Vec<bool>, computed_scores: Vec<ScoreEntry>, correct_scores: Vec<ScoreEntry>,
    ) -> Result<bool, SdkError> {
        let mut computed_scores_map = HashMap::new();
        for score in computed_scores {
            computed_scores_map.insert(score.id().clone(), *score.value());
        }

        let mut correct_scores_map = HashMap::new();
        for score in correct_scores {
            correct_scores_map.insert(score.id().clone(), *score.value());
        }

        let is_converged = is_converged_org(&computed_scores_map, &correct_scores_map);
        let votes = votes.iter().fold(true, |acc, vote| acc & vote);

        Ok(is_converged & votes)
    }
}

/// 1. Reads a CSV file and get a list of `TrustEntry`.
/// 2. Call the `OpenRankSDK::trust_update` func
pub async fn update_trust(
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
    let sdk = OpenRankSDK::new(sk, config);
    let results = sdk.trust_update(&entries).await?;

    Ok(results)
}

/// 1. Reads a CSV file and get a list of `ScoreEntry`.
/// 2. Call the `OpenRankSDK::seed_update` func
pub async fn update_seed(
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
    let sdk = OpenRankSDK::new(sk, config);
    let results = sdk.seed_update(&entries).await?;

    Ok(results)
}

/// Call the `OpenRankSDK::compute_request` func
pub async fn compute_request(
    sk: SigningKey, config_path: &str,
) -> Result<ComputeRequestResult, SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Create SDK & send compute request
    let sdk = OpenRankSDK::new(sk, config);
    let result = sdk.compute_request().await?;

    Ok(result)
}

/// Parse the `arg` & call the `OpenRankSDK::get_results` func
pub async fn get_results(
    sk: SigningKey, arg: String, config_path: &str, allow_incomplete: bool, allow_failed: bool,
) -> Result<(Vec<bool>, Vec<ScoreEntry>), SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Decoding the ComputeRequest TX hash
    let tx_hash_bytes = hex::decode(arg).map_err(SdkError::HexError)?;
    let compute_request_tx_hash = TxHash::from_bytes(tx_hash_bytes);

    // Create SDK & get results
    let sdk = OpenRankSDK::new(sk, config);
    let result = sdk.get_results(compute_request_tx_hash, allow_incomplete, allow_failed).await?;

    Ok((result.votes, result.scores))
}

/// Parse the `arg` & call the `OpenRankSDK::get_compute_result` func
pub async fn get_compute_result(
    sk: SigningKey, arg: String, config_path: &str,
) -> Result<compute::Result, SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Decoding the sequence number
    let seq_number = arg.parse::<u64>().map_err(SdkError::ParseIntError)?;

    // Create SDK & get results
    let sdk = OpenRankSDK::new(sk, config);
    let result = sdk.get_compute_result(seq_number).await?;

    Ok(result)
}

/// Parse the `arg` & call the `OpenRankSDK::get_compute_result_txs` func
pub async fn get_compute_result_txs(
    sk: SigningKey, arg: String, config_path: &str,
) -> Result<Vec<Tx>, SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Decoding the sequence number
    let seq_number = arg.parse::<u64>().map_err(SdkError::ParseIntError)?;

    // Create SDK & get results
    let sdk = OpenRankSDK::new(sk, config);
    let result = sdk.get_compute_result_txs(seq_number).await?;

    Ok(result)
}

/// Parse the `arg` & call the `OpenRankSDK::get_tx` func
pub async fn get_tx(sk: SigningKey, arg: String, config_path: &str) -> Result<Tx, SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Parse arg, to get the prefix and TX hash
    let arg_clone = arg.clone();
    let (prefix, tx_hash) = arg.split_once(':').ok_or(SdkError::ArgParseError(arg_clone))?;

    // Create SDK & get results
    let sdk = OpenRankSDK::new(sk, config);
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
pub async fn get_trust_updates(
    sk: SigningKey, config_path: &str, from: Option<String>, size: Option<usize>,
) -> Result<Vec<TrustUpdate>, SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Create SDK & get results
    let sdk = OpenRankSDK::new(sk, config);
    let result = sdk.get_trust_updates(from, size).await?;

    Ok(result)
}

/// Call the `OpenRankSDK::get_seed_updates` func
pub async fn get_seed_updates(
    sk: SigningKey, config_path: &str, from: Option<String>, size: Option<usize>,
) -> Result<Vec<SeedUpdate>, SdkError> {
    // Read config
    let config = read_config(config_path)?;

    // Create SDK & get results
    let sdk = OpenRankSDK::new(sk, config);
    let result = sdk.get_seed_updates(from, size).await?;

    Ok(result)
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

/// Utility function for writing json to a file.
pub fn write_json_to_file<T: Serialize>(path: &str, data: T) -> Result<(), SdkError> {
    let file = File::create(path).map_err(SdkError::IoError)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &data).map_err(SdkError::SerdeError)?;
    writer.flush().map_err(SdkError::IoError)?;
    Ok(())
}

/// Returns the secret key from the environment variable.
pub fn get_secret_key() -> Result<SigningKey, SdkError> {
    let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
    let secret_key_bytes = hex::decode(secret_key_hex).map_err(SdkError::HexError)?;
    let secret_key =
        SigningKey::from_slice(secret_key_bytes.as_slice()).map_err(SdkError::EcdsaError)?;
    Ok(secret_key)
}

/// Generates a new ECDSA keypair and returns the address and the private key.
pub fn generate_keypair<R: CryptoRngCore>(rng: &mut R) -> (SigningKey, Address) {
    let sk = SigningKey::random(rng);
    let addr = address_from_sk(&sk);
    (sk, addr)
}
