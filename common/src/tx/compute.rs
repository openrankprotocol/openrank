use crate::tx::{trust::ScoreEntry, Address, TxHash};
use crate::{merkle::Hash, topics::DomainHash};
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
#[rlp(trailing)]
pub struct Request {
    domain_id: DomainHash,
    block_height: u32,
    compute_id: Hash,
    seq_number: Option<u64>,
}

impl Request {
    pub fn new(domain_id: DomainHash, block_height: u32, compute_id: Hash) -> Self {
        Self { domain_id, block_height, compute_id, seq_number: None }
    }

    pub fn set_seq_number(&mut self, seq_number: u64) {
        self.seq_number = Some(seq_number)
    }
}

#[derive(
    Debug, Clone, PartialEq, Default, Serialize, Deserialize, RlpEncodable, RlpDecodable, Getters,
)]
#[getset(get = "pub")]
pub struct RequestSequence {
    compute_request_tx_hash: TxHash,
    seq_number: u64,
}

impl RequestSequence {
    pub fn new(compute_request_tx_hash: TxHash, seq_number: u64) -> Self {
        Self { compute_request_tx_hash, seq_number }
    }
}

#[derive(
    Debug, Clone, PartialEq, Default, Serialize, Deserialize, RlpEncodable, RlpDecodable, Getters,
)]
#[getset(get = "pub")]
pub struct Assignment {
    request_tx_hash: TxHash,
    assigned_compute_node: Address,
    assigned_verifier_nodes: Vec<Address>,
}

impl Assignment {
    pub fn new(
        request_tx_hash: TxHash, assigned_compute_node: Address,
        assigned_verifier_nodes: Vec<Address>,
    ) -> Self {
        Self { request_tx_hash, assigned_compute_node, assigned_verifier_nodes }
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
#[derive(Debug, Clone, PartialEq, RlpEncodable, RlpDecodable, Serialize, Deserialize, Getters)]
#[rlp(trailing)]
#[getset(get = "pub")]
pub struct Result {
    /// Hash of the ComputeCommitment TX.
    compute_commitment_tx_hash: TxHash,
    /// Hashes of the ComputeVerification TXs.
    compute_verification_tx_hashes: Vec<TxHash>,
    /// Hash of the original ComputeRequest TX.
    compute_request_tx_hash: TxHash,
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
        }
    }

    /// Append verification tx hash
    pub fn append_verification_tx_hash(&mut self, tx_hash: TxHash) {
        self.compute_verification_tx_hashes.push(tx_hash);
    }
}

/// Object connecting the compute result with the original compute request
#[derive(Debug, Clone, RlpEncodable, RlpDecodable, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct ResultReference {
    /// Hash of the original compute request transaction.
    compute_request_tx_hash: TxHash,
    /// Hash of the result transaction
    compute_result_tx_hash: TxHash,
}

impl ResultReference {
    pub fn new(compute_request_tx_hash: TxHash, compute_result_tx_hash: TxHash) -> Self {
        Self { compute_request_tx_hash, compute_result_tx_hash }
    }

    pub fn set_compute_result_tx_hash(&mut self, tx_hash: TxHash) {
        self.compute_result_tx_hash = tx_hash;
    }
}
