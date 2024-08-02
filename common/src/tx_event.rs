use crate::db::DbItem;
use alloy_rlp::encode;
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

#[derive(Debug, Clone, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize)]
pub struct InclusionProof([u8; 32]);

#[derive(Debug, Clone, RlpDecodable, RlpEncodable, Serialize, Deserialize)]
pub struct TxEvent {
	// Supposed to be block height
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

	fn get_cf() -> String {
		"tx_event".to_string()
	}

	fn get_prefix(&self) -> String {
		"tx_event".to_string()
	}
}

#[cfg(test)]
mod test {
	use super::TxEvent;
	use crate::{
		db::DbItem,
		txs::{JobRunRequest, Tx, TxKind},
	};
	use alloy_rlp::encode;

	#[test]
	fn test_tx_event_db_item() {
		let tx_event = TxEvent::default_with_data(encode(Tx::default_with(
			TxKind::JobRunRequest,
			encode(JobRunRequest::default()),
		)));

		let key = tx_event.get_key();
		assert_eq!(
			hex::encode(key),
			"b486c7ce0b8114c45571048c16ebc834f680ca021612d347a6560aa001bdfe97"
		);
	}
}
