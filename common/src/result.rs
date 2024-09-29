use crate::{db::DbItem, txs::TxHash};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResultsQuery {
    pub compute_request_tx_hash: TxHash,
    pub start: u32,
    pub size: u32,
}

impl GetResultsQuery {
    pub fn new(compute_request_tx_hash: TxHash, start: u32, size: u32) -> Self {
        Self { compute_request_tx_hash, start, size }
    }
}

/// Combination of several tx hashes representing the result of a compute run by `Computer`.
#[derive(Debug, Clone, RlpEncodable, RlpDecodable, Serialize, Deserialize)]
pub struct ComputeResult {
    /// Hash of the ComputeCommitment TX.
    pub compute_commitment_tx_hash: TxHash,
    /// Hashes of the ComputeVerification TXs.
    pub compute_verification_tx_hashes: Vec<TxHash>,
    /// Hash of the original ComputeRequest TX.
    compute_request_tx_hash: TxHash,
}

impl ComputeResult {
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

    /// Constructs the full key for the given tx hash.
    pub fn construct_full_key(tx_hash: TxHash) -> Vec<u8> {
        let mut prefix = "result".to_string().as_bytes().to_vec();
        prefix.extend(tx_hash.0);
        prefix
    }
}

impl DbItem for ComputeResult {
    fn get_key(&self) -> Vec<u8> {
        self.compute_request_tx_hash.0.to_vec()
    }

    fn get_cf() -> String {
        "metadata".to_string()
    }

    fn get_prefix(&self) -> String {
        "result".to_string()
    }
}
