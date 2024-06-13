pub mod topics;
pub mod txs;

pub struct InclusionProof([u8; 32]);

impl InclusionProof {
	pub fn from_bytes(data: Vec<u8>) -> Self {
		let mut bytes = [0; 32];
		bytes.copy_from_slice(data.as_slice());
		Self(bytes)
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		self.0.to_vec()
	}
}

impl Default for InclusionProof {
	fn default() -> Self {
		Self([0; 32])
	}
}

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

	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let mut blob_id_bytes = [0; 8];
		blob_id_bytes.copy_from_slice(&data.drain(..8).as_slice());
		let blob_id = u64::from_be_bytes(blob_id_bytes);

		let proof = InclusionProof::from_bytes(data.drain(..32).into_iter().collect());

		Self { blob_id, proof, data }
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.blob_id.to_be_bytes());
		bytes.extend(self.proof.to_bytes());
		bytes.extend(&self.data);
		bytes
	}
}
