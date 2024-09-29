use super::{trust::ScoreEntry, Address, TxHash};
use crate::{merkle::Hash, topics::DomainHash};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct ComputeCommitment {
    pub compute_assignment_tx_hash: TxHash,
    pub lt_root_hash: Hash,
    pub compute_root_hash: Hash,
    pub scores_tx_hashes: Vec<TxHash>,
}

impl ComputeCommitment {
    pub fn new(
        compute_assignment_tx_hash: TxHash, lt_root_hash: Hash, compute_root_hash: Hash,
        scores_tx_hashes: Vec<TxHash>,
    ) -> Self {
        Self { compute_assignment_tx_hash, lt_root_hash, compute_root_hash, scores_tx_hashes }
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
    pub compute_id: Hash,
}

impl ComputeRequest {
    pub fn new(domain_id: DomainHash, block_height: u32, compute_id: Hash) -> Self {
        Self { domain_id, block_height, compute_id }
    }
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct ComputeAssignment {
    pub compute_request_tx_hash: TxHash,
    pub assigned_compute_node: Address,
    pub assigned_verifier_node: Address,
}

impl ComputeAssignment {
    pub fn new(
        compute_request_tx_hash: TxHash, assigned_compute_node: Address,
        assigned_verifier_node: Address,
    ) -> Self {
        Self { compute_request_tx_hash, assigned_compute_node, assigned_verifier_node }
    }
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct ComputeVerification {
    pub compute_assignment_tx_hash: TxHash,
    pub verification_result: bool,
}

impl ComputeVerification {
    pub fn new(compute_assignment_tx_hash: TxHash, verification_result: bool) -> Self {
        Self { compute_assignment_tx_hash, verification_result }
    }
}

impl Default for ComputeVerification {
    fn default() -> Self {
        Self { compute_assignment_tx_hash: TxHash::default(), verification_result: true }
    }
}
