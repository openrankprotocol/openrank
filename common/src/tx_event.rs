use crate::db::DbItem;
use alloy_rlp::encode;
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

#[derive(Debug, Clone, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize)]
pub struct InclusionProof([u8; 32]);

#[derive(Debug, Clone, RlpDecodable, RlpEncodable, Serialize, Deserialize)]
pub struct TxEvent {
	blob_id: u64,
	proof: InclusionProof,
	data: Vec<u8>,
}

impl TxEvent {
	pub fn new(blob_id: u64, proof: InclusionProof, data: Vec<u8>) -> Self {
		Self { blob_id, proof, data }
	}

	pub fn default_with_data(data: Vec<u8>) -> Self {
		Self { blob_id: 0, proof: InclusionProof::default(), data }
	}

	pub fn data(&self) -> Vec<u8> {
		self.data.clone()
	}
}

impl DbItem for TxEvent {
	fn get_key(&self) -> Vec<u8> {
		let mut hasher = Keccak256::new();
		hasher.update(&self.blob_id.to_be_bytes());
		hasher.update(encode(&self.proof));
		let result = hasher.finalize();
		result.to_vec()
	}
}
