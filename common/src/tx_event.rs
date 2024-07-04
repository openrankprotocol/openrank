use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, RlpDecodable, RlpEncodable, Serialize, Deserialize)]
pub struct InclusionProof([u8; 32]);

impl Default for InclusionProof {
	fn default() -> Self {
		Self([0; 32])
	}
}

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
