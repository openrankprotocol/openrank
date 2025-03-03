use alloy_rlp::encode;
use getset::Getters;
use jsonrpsee::{core::client::ClientT, http_client::HttpClient};
use k256::ecdsa::{Error as EcdsaError, SigningKey};
use openrank_common::{
    algos::et::is_converged_org,
    merkle::hash_leaf,
    misc::{LocalTrustStateResponse, SeedTrustStateResponse},
    query::{GetSeedUpdateQuery, GetTrustUpdateQuery},
    runners::compute_runner::{self, ComputeRunner},
    topics::Domain,
    tx::{
        self,
        compute::{self, Verification},
        consts,
        trust::{ScoreEntry, SeedUpdate, TrustEntry, TrustUpdate},
        Body, Tx, TxHash,
    },
    tx_event::TxEvent,
};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use sha3::Keccak256;
use std::{cmp::Ordering, collections::BTreeMap, fs::File, io::Read};
use std::{io, num};

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
    /// The Computer configuration
    computer: Computer,
    /// The Verifier configuration
    verifier: Verifier,
}

/// Creates a new `Config` from a local TOML file, given file path.
pub fn read_config(path: &str) -> Result<Config, SdkError> {
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
/// The configuration for the Computer.
pub struct Computer {
    endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// The configuration for the Verifier.
pub struct Verifier {
    endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct ComputeRequestResult {
    compute_tx_hash: TxHash,
    tx_event: TxEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct ComputeResult {
    pub votes: Vec<bool>,
    pub scores: Vec<ScoreEntry>,
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
    #[error("Signing key unavailable")]
    SigningKeyUnavailable,
    #[error("ComputeRunner Error: {0}")]
    ComputeRunnerError(compute_runner::Error),
}

pub struct OpenRankSDK {
    secret_key: Option<SigningKey>,
    config: Config,
    sequencer_client: HttpClient,
    computer_client: HttpClient,
    _verifier_client: HttpClient,
}

impl OpenRankSDK {
    pub fn new(config: Config, secret_key: Option<SigningKey>) -> Result<Self, SdkError> {
        let sequencer_client = HttpClient::builder()
            .build(config.sequencer.endpoint.as_str())
            .map_err(SdkError::JsonRpcClientError)?;

        let computer_client = HttpClient::builder()
            .build(config.computer.endpoint.as_str())
            .map_err(SdkError::JsonRpcClientError)?;

        let _verifier_client = HttpClient::builder()
            .build(config.verifier.endpoint.as_str())
            .map_err(SdkError::JsonRpcClientError)?;
        Ok(Self { secret_key, config, sequencer_client, computer_client, _verifier_client })
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Sends the list of `TrustEntry` to the Sequencer.
    pub async fn trust_update(
        &self, trust_entries: &[TrustEntry],
    ) -> Result<Vec<TxEvent>, SdkError> {
        let sk = self.secret_key.as_ref().ok_or(SdkError::SigningKeyUnavailable)?;
        let mut results = Vec::new();
        for chunk in trust_entries.chunks(TRUST_CHUNK_SIZE) {
            let mut tx = Tx::default_with(Body::TrustUpdate(TrustUpdate::new(
                self.config.domain.trust_namespace(),
                chunk.to_vec(),
            )));
            tx.sign(sk).map_err(SdkError::EcdsaError)?;

            let result: TxEvent = self
                .sequencer_client
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
        let sk = self.secret_key.as_ref().ok_or(SdkError::SigningKeyUnavailable)?;
        let mut results = Vec::new();
        for chunk in seed_entries.chunks(SEED_CHUNK_SIZE) {
            let mut tx = Tx::default_with(Body::SeedUpdate(SeedUpdate::new(
                self.config.domain.seed_namespace(),
                chunk.to_vec(),
            )));
            tx.sign(sk).map_err(SdkError::EcdsaError)?;

            let tx_event: TxEvent = self
                .sequencer_client
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
        let sk = self.secret_key.as_ref().ok_or(SdkError::SigningKeyUnavailable)?;
        let rng = &mut thread_rng();
        let domain_id = self.config.domain.to_hash();
        let hash = hash_leaf::<Keccak256>(rng.gen::<[u8; 32]>().to_vec());
        let mut tx = Tx::default_with(Body::ComputeRequest(compute::Request::new(
            domain_id, 0, hash,
        )));
        tx.sign(sk).map_err(SdkError::EcdsaError)?;
        let tx_hash = tx.hash();

        let tx_event: TxEvent = self
            .sequencer_client
            .request("sequencer_compute_request", vec![hex::encode(encode(tx))])
            .await
            .map_err(SdkError::JsonRpcClientError)?;

        let compute_result = ComputeRequestResult::new(tx_hash, tx_event);
        Ok(compute_result)
    }

    pub async fn compute_local(
        trust_entries: &[TrustEntry], seed_entries: &[ScoreEntry],
    ) -> Result<Vec<ScoreEntry>, SdkError> {
        let mock_domain = Domain::default();
        let mut runner = ComputeRunner::new(&[mock_domain.clone()]);
        runner
            .update_trust(mock_domain.clone(), trust_entries.to_vec())
            .map_err(SdkError::ComputeRunnerError)?;
        runner
            .update_seed(mock_domain.clone(), seed_entries.to_vec())
            .map_err(SdkError::ComputeRunnerError)?;
        runner.compute(mock_domain.clone()).map_err(SdkError::ComputeRunnerError)?;
        let scores =
            runner.get_compute_scores(mock_domain.clone()).map_err(SdkError::ComputeRunnerError)?;
        let score_entries: Vec<ScoreEntry> =
            scores.iter().flat_map(|x| x.clone().inner()).collect();
        Ok(score_entries)
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Calls the Sequencer to get the EigenTrust scores(`ScoreEntry`s).
    pub async fn get_results(
        &self, compute_request_tx_hash: TxHash, allow_incomplete: bool, allow_failed: bool,
    ) -> Result<ComputeResult, SdkError> {
        // Checking if ComputeRequest exists, if not, it should return an error
        let compute_request_key = (consts::COMPUTE_REQUEST, compute_request_tx_hash.clone());
        let _: Tx = self
            .sequencer_client
            .request("sequencer_get_tx", compute_request_key)
            .await
            .map_err(SdkError::JsonRpcClientError)?;

        // If ComputeRequest exists, we can proceed with fetching results
        let res: Result<u64, SdkError> = self
            .sequencer_client
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

        let result: compute::Result = self
            .sequencer_client
            .request("sequencer_get_compute_result", vec![seq_number])
            .await
            .map_err(SdkError::JsonRpcClientError)?;

        let commitment_tx_key = (
            consts::COMPUTE_COMMITMENT,
            result.compute_commitment_tx_hash().clone(),
        );
        let commitment_tx: Tx = self
            .sequencer_client
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
        let scores_txs: Vec<Tx> = self
            .sequencer_client
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
        let assignment_tx: Tx = self
            .sequencer_client
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
        let verification_txs: Vec<Tx> = self
            .sequencer_client
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
        let result: compute::Result = self
            .sequencer_client
            .request("sequencer_get_compute_result", vec![seq_number])
            .await
            .map_err(SdkError::JsonRpcClientError)?;
        Ok(result)
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Calls the Sequencer to get all the txs that are included inside a specific compute result.
    pub async fn get_compute_result_txs(&self, seq_number: u64) -> Result<Vec<Tx>, SdkError> {
        let result: compute::Result = self
            .sequencer_client
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

        let txs_res = self
            .sequencer_client
            .request("sequencer_get_txs", vec![txs_arg])
            .await
            .map_err(SdkError::JsonRpcClientError)?;
        Ok(txs_res)
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Calls the Sequencer to get the TX given a TX hash.
    pub async fn get_tx(&self, prefix: &str, tx_hash: &str) -> Result<Tx, SdkError> {
        let tx_hash_bytes = hex::decode(tx_hash).map_err(SdkError::HexError)?;
        let tx_hash = TxHash::from_bytes(tx_hash_bytes);
        let tx: Tx = self
            .sequencer_client
            .request("sequencer_get_tx", (prefix.to_string(), tx_hash))
            .await
            .map_err(SdkError::JsonRpcClientError)?;
        Ok(tx)
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Calls the Sequencer to get the current state of LT matrix.
    pub async fn get_trust_state(
        &self, domain: Domain,
    ) -> Result<LocalTrustStateResponse, SdkError> {
        let trust_state: LocalTrustStateResponse = self
            .computer_client
            .request(
                "computer_get_lt_state",
                (domain, None::<usize>, None::<String>),
            )
            .await
            .map_err(SdkError::JsonRpcClientError)?;
        Ok(trust_state)
    }

    /// 1. Creates a new `Client`, which can be used to call the Sequencer.
    /// 2. Calls the Sequencer to get the current state of LT matrix.
    pub async fn get_seed_state(&self, domain: Domain) -> Result<SeedTrustStateResponse, SdkError> {
        let seed_state: SeedTrustStateResponse = self
            .computer_client
            .request(
                "computer_get_seed_state",
                (domain, None::<usize>, None::<String>),
            )
            .await
            .map_err(SdkError::JsonRpcClientError)?;
        Ok(seed_state)
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

        let results_query = GetTrustUpdateQuery::new(from, size);
        let trust_updates: Vec<TrustUpdate> = self
            .sequencer_client
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

        let results_query = GetSeedUpdateQuery::new(from, size);
        let seed_updates: Vec<SeedUpdate> = self
            .sequencer_client
            .request("sequencer_get_seed_updates", vec![results_query])
            .await
            .map_err(SdkError::JsonRpcClientError)?;
        Ok(seed_updates)
    }

    pub fn check_score_integrity(
        votes: Vec<bool>, computed_scores: Vec<ScoreEntry>, correct_scores: Vec<ScoreEntry>,
    ) -> Result<bool, SdkError> {
        let mut computed_scores_map = BTreeMap::new();
        for score in computed_scores {
            computed_scores_map.insert(score.id().clone(), *score.value());
        }

        let mut correct_scores_map = BTreeMap::new();
        for score in correct_scores {
            correct_scores_map.insert(score.id().clone(), *score.value());
        }

        let is_converged = is_converged_org(&computed_scores_map, &correct_scores_map);
        let votes = votes.iter().fold(true, |acc, vote| acc & vote);

        Ok(is_converged & votes)
    }
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
