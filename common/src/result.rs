use crate::{db::DbItem, txs::TxHash};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, RlpEncodable, RlpDecodable, Serialize, Deserialize)]
pub struct JobResult {
	pub create_commitment_tx_hash: TxHash,
	pub job_verification_tx_hashes: Vec<TxHash>,
	job_run_request_tx_hash: TxHash,
}

impl JobResult {
	pub fn new(
		create_commitment_tx_hash: TxHash, job_verification_tx_hashes: Vec<TxHash>,
		job_run_request_tx_hash: TxHash,
	) -> Self {
		Self { create_commitment_tx_hash, job_verification_tx_hashes, job_run_request_tx_hash }
	}

	pub fn construct_full_key(tx_hash: Vec<u8>) -> Vec<u8> {
		let mut prefix = "result".to_string().as_bytes().to_vec();
		prefix.extend(tx_hash);
		prefix
	}
}

impl DbItem for JobResult {
	fn get_key(&self) -> Vec<u8> {
		self.job_run_request_tx_hash.0.to_vec()
	}

	fn get_cf() -> String {
		"metadata".to_string()
	}

	fn get_prefix(&self) -> String {
		"result".to_string()
	}
}
