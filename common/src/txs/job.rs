use super::{trust::ScoreEntry, Address, TxHash};
use crate::{merkle::Hash, topics::DomainHash};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct ComputeCommitment {
    pub job_assignment_tx_hash: TxHash,
    pub lt_root_hash: Hash,
    pub compute_root_hash: Hash,
    pub scores_tx_hashes: Vec<TxHash>,
}

impl ComputeCommitment {
    pub fn new(
        job_assignment_tx_hash: TxHash, lt_root_hash: Hash, compute_root_hash: Hash,
        scores_tx_hashes: Vec<TxHash>,
    ) -> Self {
        Self { job_assignment_tx_hash, lt_root_hash, compute_root_hash, scores_tx_hashes }
    }
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct ComputeScores {
    pub entries: Vec<ScoreEntry>,
}

impl ComputeScores {
    pub fn new(entries: Vec<ScoreEntry>) -> Self {
        Self { entries }
    }
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct ComputeRequest {
    pub domain_id: DomainHash,
    pub block_height: u32,
    pub job_id: Hash,
}

impl ComputeRequest {
    pub fn new(domain_id: DomainHash, block_height: u32, job_id: Hash) -> Self {
        Self { domain_id, block_height, job_id }
    }
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct ComputeAssignment {
    pub job_request_tx_hash: TxHash,
    pub assigned_compute_node: Address,
    pub assigned_verifier_node: Address,
}

impl ComputeAssignment {
    pub fn new(
        job_request_tx_hash: TxHash, assigned_compute_node: Address,
        assigned_verifier_node: Address,
    ) -> Self {
        Self { job_request_tx_hash, assigned_compute_node, assigned_verifier_node }
    }
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct ComputeVerification {
    pub job_assignment_tx_hash: TxHash,
    pub verification_result: bool,
}

impl ComputeVerification {
    pub fn new(job_assignment_tx_hash: TxHash, verification_result: bool) -> Self {
        Self { job_assignment_tx_hash, verification_result }
    }
}

impl Default for ComputeVerification {
    fn default() -> Self {
        Self { job_assignment_tx_hash: TxHash::default(), verification_result: true }
    }
}
