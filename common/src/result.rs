use crate::{db::DbItem, txs::TxHash};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResultsQuery {
    pub job_run_request_tx_hash: TxHash,
    pub start: u32,
    pub size: u32,
}

impl GetResultsQuery {
    pub fn new(job_run_request_tx_hash: TxHash, start: u32, size: u32) -> Self {
        Self { job_run_request_tx_hash, start, size }
    }
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable, Serialize, Deserialize)]
pub struct ComputeResult {
    pub job_commitment_tx_hash: TxHash,
    pub job_verification_tx_hashes: Vec<TxHash>,
    job_request_tx_hash: TxHash,
}

impl ComputeResult {
    pub fn new(
        job_commitment_tx_hash: TxHash, job_verification_tx_hashes: Vec<TxHash>,
        job_request_tx_hash: TxHash,
    ) -> Self {
        Self { job_commitment_tx_hash, job_verification_tx_hashes, job_request_tx_hash }
    }

    pub fn construct_full_key(tx_hash: TxHash) -> Vec<u8> {
        let mut prefix = "result".to_string().as_bytes().to_vec();
        prefix.extend(tx_hash.0);
        prefix
    }
}

impl DbItem for ComputeResult {
    fn get_key(&self) -> Vec<u8> {
        self.job_request_tx_hash.0.to_vec()
    }

    fn get_cf() -> String {
        "metadata".to_string()
    }

    fn get_prefix(&self) -> String {
        "result".to_string()
    }
}
