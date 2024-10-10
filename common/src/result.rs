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

/// Combination of several tx hashes representing the result of a job run by `Computer`.
#[derive(Debug, Clone, RlpEncodable, RlpDecodable, Serialize, Deserialize)]
#[rlp(trailing)]
pub struct JobResult {
    /// Hash of the create commitment transaction.
    pub create_commitment_tx_hash: TxHash,
    /// Hashes of the job verification transactions.
    pub job_verification_tx_hashes: Vec<TxHash>,
    /// Hash of the original job run request transaction.
    pub job_run_request_tx_hash: TxHash,
    /// Sequence number assigned by the block builder.
    pub seq_number: Option<u64>,
}

impl JobResult {
    pub fn new(
        create_commitment_tx_hash: TxHash, job_verification_tx_hashes: Vec<TxHash>,
        job_run_request_tx_hash: TxHash,
    ) -> Self {
        Self {
            create_commitment_tx_hash,
            job_verification_tx_hashes,
            job_run_request_tx_hash,
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
}

impl DbItem for JobResult {
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
#[derive(Debug, Clone, RlpEncodable, RlpDecodable, Serialize, Deserialize)]
pub struct JobResultReference {
    /// Hash of the original job run request transaction.
    pub job_run_request_tx_hash: TxHash,
    /// Sequence number assigned by the block builder.
    pub seq_number: u64,
}

impl JobResultReference {
    pub fn new(job_run_request_tx_hash: TxHash, seq_number: u64) -> Self {
        Self { job_run_request_tx_hash, seq_number }
    }
}

impl DbItem for JobResultReference {
    fn get_key(&self) -> Vec<u8> {
        self.job_run_request_tx_hash.0.to_vec()
    }

    fn get_prefix(&self) -> String {
        String::new()
    }

    fn get_cf() -> String {
        "result_reference".to_string()
    }
}
