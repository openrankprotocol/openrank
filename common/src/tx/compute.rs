use crate::tx::{trust::ScoreEntry, Address, TxHash};
use crate::{db::DbItem, merkle::Hash, topics::DomainHash};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use getset::Getters;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, RlpEncodable, RlpDecodable, Getters,
)]
#[getset(get = "pub")]
pub struct Commitment {
    assignment_tx_hash: TxHash,
    lt_root_hash: Hash,
    compute_root_hash: Hash,
    scores_tx_hashes: Vec<TxHash>,
}

impl Commitment {
    pub fn new(
        assignment_tx_hash: TxHash, lt_root_hash: Hash, compute_root_hash: Hash,
        scores_tx_hashes: Vec<TxHash>,
    ) -> Self {
        Self { assignment_tx_hash, lt_root_hash, compute_root_hash, scores_tx_hashes }
    }
}

#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, RlpEncodable, RlpDecodable, Getters,
)]
#[getset(get = "pub")]
pub struct Scores {
    entries: Vec<ScoreEntry>,
}

impl Scores {
    pub fn new(entries: Vec<ScoreEntry>) -> Self {
        Self { entries }
    }
}

#[derive(
    Debug, Clone, PartialEq, Default, Serialize, Deserialize, RlpEncodable, RlpDecodable, Getters,
)]
#[getset(get = "pub")]
pub struct Request {
    domain_id: DomainHash,
    block_height: u32,
    compute_id: Hash,
}

impl Request {
    pub fn new(domain_id: DomainHash, block_height: u32, compute_id: Hash) -> Self {
        Self { domain_id, block_height, compute_id }
    }
}

#[derive(
    Debug, Clone, PartialEq, Default, Serialize, Deserialize, RlpEncodable, RlpDecodable, Getters,
)]
#[getset(get = "pub")]
pub struct Assignment {
    request_tx_hash: TxHash,
    assigned_compute_node: Address,
    assigned_verifier_node: Address,
}

impl Assignment {
    pub fn new(
        request_tx_hash: TxHash, assigned_compute_node: Address, assigned_verifier_node: Address,
    ) -> Self {
        Self { request_tx_hash, assigned_compute_node, assigned_verifier_node }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, RlpEncodable, RlpDecodable, Getters)]
#[getset(get = "pub")]
pub struct Verification {
    assignment_tx_hash: TxHash,
    verification_result: bool,
}

impl Verification {
    pub fn new(assignment_tx_hash: TxHash, verification_result: bool) -> Self {
        Self { assignment_tx_hash, verification_result }
    }
}

impl Default for Verification {
    fn default() -> Self {
        Self { assignment_tx_hash: TxHash::default(), verification_result: true }
    }
}

/// Combination of several tx hashes representing the result of a compute run by `Computer`.
#[derive(Debug, Clone, RlpEncodable, RlpDecodable, Serialize, Deserialize, Getters)]
#[rlp(trailing)]
#[getset(get = "pub")]
pub struct Result {
    /// Hash of the ComputeCommitment TX.
    compute_commitment_tx_hash: TxHash,
    /// Hashes of the ComputeVerification TXs.
    compute_verification_tx_hashes: Vec<TxHash>,
    /// Hash of the original ComputeRequest TX.
    compute_request_tx_hash: TxHash,
    /// Sequence number assigned by the block builder.
    seq_number: Option<u64>,
}

impl Result {
    pub fn new(
        compute_commitment_tx_hash: TxHash, compute_verification_tx_hashes: Vec<TxHash>,
        compute_request_tx_hash: TxHash,
    ) -> Self {
        Self {
            compute_commitment_tx_hash,
            compute_verification_tx_hashes,
            compute_request_tx_hash,
            seq_number: None,
        }
    }

    /// Constructs the full key for the given tx hash.
    pub fn construct_full_key(seq_number: u64) -> Vec<u8> {
        let mut prefix = "result".to_string().as_bytes().to_vec();
        prefix.extend(seq_number.to_be_bytes());
        prefix
    }

    /// Set sequence number
    pub fn set_seq_number(&mut self, seq_number: u64) {
        self.seq_number = Some(seq_number);
    }

    /// Append verification tx hash
    pub fn append_verification_tx_hash(&mut self, tx_hash: TxHash) {
        self.compute_verification_tx_hashes.push(tx_hash);
    }
}

impl DbItem for Result {
    fn get_key(&self) -> Vec<u8> {
        self.seq_number.unwrap().to_be_bytes().to_vec()
    }

    fn get_cf() -> String {
        "metadata".to_string()
    }

    fn get_prefix(&self) -> String {
        "result".to_string()
    }
}

/// Object connecting the sequence number with the original compute request
#[derive(Debug, Clone, RlpEncodable, RlpDecodable, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct ResultReference {
    /// Hash of the original job run request transaction.
    compute_request_tx_hash: TxHash,
    /// Sequence number assigned by the block builder.
    seq_number: u64,
}

impl ResultReference {
    pub fn new(compute_request_tx_hash: TxHash, seq_number: u64) -> Self {
        Self { compute_request_tx_hash, seq_number }
    }
}

impl DbItem for ResultReference {
    fn get_key(&self) -> Vec<u8> {
        self.compute_request_tx_hash.0.to_vec()
    }

    fn get_prefix(&self) -> String {
        String::new()
    }

    fn get_cf() -> String {
        "result_reference".to_string()
    }
}
