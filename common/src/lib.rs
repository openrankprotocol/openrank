pub mod topics;
pub mod txs;

struct InclusionProof([u8; 32]);

struct TxEvent {
	blob_id: u64,
	proof: InclusionProof,
	data: Vec<u8>,
}

impl TxEvent {
	pub fn new(blob_id: u64, proof: InclusionProof, data: Vec<u8>) -> Self {
		Self { blob_id, proof, data }
	}
}
