#[derive(Debug, Clone)]
pub struct Address(pub [u8; 32]);

impl Default for Address {
	fn default() -> Self {
		Self([0; 32])
	}
}

impl Address {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let mut bytes = [0; 32];
		bytes.copy_from_slice(data.drain(..32).as_slice());
		Self(bytes)
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		self.0.to_vec()
	}
}

pub struct TxHash([u8; 32]);

impl Default for TxHash {
	fn default() -> Self {
		Self([0; 32])
	}
}

impl TxHash {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let mut bytes = [0; 32];
		bytes.copy_from_slice(data.drain(..32).as_slice());
		Self(bytes)
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		self.0.to_vec()
	}
}

pub struct RootHash([u8; 32]);

impl Default for RootHash {
	fn default() -> Self {
		Self([0; 32])
	}
}

impl RootHash {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let mut bytes = [0; 32];
		bytes.copy_from_slice(data.drain(..32).as_slice());
		Self(bytes)
	}

	fn to_bytes(&self) -> Vec<u8> {
		self.0.to_vec()
	}
}

pub struct Signature {
	s: [u8; 32],
	r: [u8; 32],
}

impl Default for Signature {
	fn default() -> Self {
		Self { s: [0; 32], r: [0; 32] }
	}
}

impl Signature {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let mut s_bytes = [0; 32];
		let mut r_bytes = [0; 32];
		s_bytes.copy_from_slice(data.drain(..32).as_slice());
		r_bytes.copy_from_slice(data.drain(..32).as_slice());
		Self { s: s_bytes, r: r_bytes }
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend_from_slice(&self.s);
		bytes.extend_from_slice(&self.r);
		bytes
	}
}

pub struct Entry {
	id: Address,
	value: f32,
}

impl Default for Entry {
	fn default() -> Self {
		Self { id: Address::default(), value: 0. }
	}
}

impl Entry {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let address = Address::from_bytes(data.drain(..32).into_iter().collect());
		let mut value_bytes = [0; 4];
		value_bytes.copy_from_slice(data.drain(..4).as_slice());
		let value = f32::from_be_bytes(value_bytes);
		Self { id: address, value }
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.id.to_bytes());
		bytes.extend_from_slice(&self.value.to_be_bytes());
		bytes
	}
}

pub struct CreateCommitment {
	tx_hash: TxHash,
	job_run_assignment_tx_hash: TxHash,
	lt_root_hash: RootHash,
	compute_root_hash: RootHash,
	scores_tx_hashes: Vec<TxHash>,
	new_trust_tx_hashes: Vec<TxHash>,
	new_seed_tx_hashes: Vec<TxHash>,
	signature: Signature,
}

impl Default for CreateCommitment {
	fn default() -> Self {
		Self {
			tx_hash: TxHash::default(),
			job_run_assignment_tx_hash: TxHash::default(),
			lt_root_hash: RootHash::default(),
			compute_root_hash: RootHash::default(),
			scores_tx_hashes: Vec::new(),
			new_trust_tx_hashes: Vec::new(),
			new_seed_tx_hashes: Vec::new(),
			signature: Signature::default(),
		}
	}
}

impl CreateCommitment {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());
		let jra_tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());
		let lt_root_hash = RootHash::from_bytes(data.drain(..32).into_iter().collect());
		let c_root_hash = RootHash::from_bytes(data.drain(..32).into_iter().collect());

		let scores_len = data.drain(..1).as_slice()[0];
		let scores_txs = data.drain(..(scores_len * 32) as usize);

		let scores_tx_bytes: Vec<u8> = scores_txs.into_iter().collect();
		let mut scores_tx = Vec::new();
		for chunk in scores_tx_bytes.chunks(32) {
			let score_tx_hash = TxHash::from_bytes(chunk.to_vec());
			scores_tx.push(score_tx_hash);
		}

		let new_trust_len = data.drain(..1).as_slice()[0];
		let new_trust_txs = data.drain(..(new_trust_len * 32) as usize);

		let new_trust_tx_bytes: Vec<u8> = new_trust_txs.into_iter().collect();
		let mut new_trust_txs = Vec::new();
		for chunk in new_trust_tx_bytes.chunks(32) {
			let new_trust_tx_hash = TxHash::from_bytes(chunk.to_vec());
			new_trust_txs.push(new_trust_tx_hash);
		}

		let new_seed_len = data.drain(..1).as_slice()[0];
		let new_seed_txs = data.drain(..(new_seed_len * 32) as usize);

		let new_seed_txs_bytes: Vec<u8> = new_seed_txs.into_iter().collect();
		let mut new_seed_txs = Vec::new();
		for chunk in new_seed_txs_bytes.chunks(32) {
			let new_seed_tx_hash = TxHash::from_bytes(chunk.to_vec());
			new_seed_txs.push(new_seed_tx_hash);
		}

		let signature = Signature::from_bytes(data.drain(..64).into_iter().collect());
		Self {
			tx_hash,
			job_run_assignment_tx_hash: jra_tx_hash,
			lt_root_hash,
			compute_root_hash: c_root_hash,
			scores_tx_hashes: scores_tx,
			new_trust_tx_hashes: new_trust_txs,
			new_seed_tx_hashes: new_seed_txs,
			signature,
		}
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.tx_hash.to_bytes());
		bytes.extend(self.job_run_assignment_tx_hash.to_bytes());
		bytes.extend(self.lt_root_hash.to_bytes());
		bytes.extend(self.compute_root_hash.to_bytes());

		let scores_len = self.scores_tx_hashes.len() as u8;
		bytes.push(scores_len);
		for tx in &self.scores_tx_hashes {
			bytes.extend(tx.to_bytes());
		}

		let new_trust_len = self.new_trust_tx_hashes.len() as u8;
		bytes.push(new_trust_len);
		for tx in &self.new_trust_tx_hashes {
			bytes.extend(tx.to_bytes());
		}

		let new_seed_len = self.new_seed_tx_hashes.len() as u8;
		bytes.push(new_seed_len);
		for tx in &self.new_seed_tx_hashes {
			bytes.extend(tx.to_bytes());
		}

		bytes.extend(self.signature.to_bytes());

		bytes
	}
}

pub struct CreateScores {
	tx_hash: TxHash,
	entries: Vec<Entry>,
	signature: Signature,
}

impl Default for CreateScores {
	fn default() -> Self {
		Self { tx_hash: TxHash::default(), entries: Vec::new(), signature: Signature::default() }
	}
}

impl CreateScores {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());

		let entries_len = data.drain(..1).as_slice()[0];
		let entries_txs = data.drain(..(entries_len * (32 + 4)) as usize);

		let entries_txs_bytes: Vec<u8> = entries_txs.into_iter().collect();
		let mut entries_txs = Vec::new();
		for chunk in entries_txs_bytes.chunks(32 + 4) {
			let mut chunk_vec = chunk.to_vec();
			let entry_address = Address::from_bytes(chunk_vec.drain(..32).into_iter().collect());
			let mut entry_value_bytes: [u8; 4] = [0; 4];
			entry_value_bytes.copy_from_slice(chunk_vec.drain(..4).as_slice());
			let entry_value = f32::from_be_bytes(entry_value_bytes);
			entries_txs.push(Entry { id: entry_address, value: entry_value });
		}

		let signature = Signature::from_bytes(data.drain(..64).into_iter().collect());

		Self { tx_hash, entries: entries_txs, signature }
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.tx_hash.to_bytes());

		let entries_len = self.entries.len() as u8;
		bytes.push(entries_len);
		for tx in &self.entries {
			bytes.extend(tx.to_bytes());
		}

		bytes.extend(self.signature.to_bytes());

		bytes
	}
}

pub struct JobRunAssignment {
	tx_hash: TxHash,
	job_run_request_tx_hash: TxHash,
	assigned_compute_node: Address,
	assigned_verifier_node: Address,
	signature: Signature,
}

impl Default for JobRunAssignment {
	fn default() -> Self {
		Self {
			tx_hash: TxHash::default(),
			job_run_request_tx_hash: TxHash::default(),
			assigned_compute_node: Address::default(),
			assigned_verifier_node: Address::default(),
			signature: Signature::default(),
		}
	}
}

impl JobRunAssignment {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());
		let job_run_request_tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());

		let assigned_compute_node = Address::from_bytes(data.drain(..32).into_iter().collect());
		let assigned_verifier_node = Address::from_bytes(data.drain(..32).into_iter().collect());

		let signature = Signature::from_bytes(data.drain(..64).into_iter().collect());

		Self {
			tx_hash,
			job_run_request_tx_hash,
			assigned_compute_node,
			assigned_verifier_node,
			signature,
		}
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.tx_hash.to_bytes());
		bytes.extend(self.job_run_request_tx_hash.to_bytes());
		bytes.extend(self.assigned_compute_node.to_bytes());
		bytes.extend(self.assigned_verifier_node.to_bytes());
		bytes.extend(self.signature.to_bytes());
		bytes
	}
}

pub struct JobVerification {
	tx_hash: TxHash,
	job_run_assignment_tx_hash: TxHash,
	verification_result: bool,
	signature: Signature,
}

impl Default for JobVerification {
	fn default() -> Self {
		Self {
			tx_hash: TxHash::default(),
			job_run_assignment_tx_hash: TxHash::default(),
			verification_result: true,
			signature: Signature::default(),
		}
	}
}

impl JobVerification {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());
		let job_run_assignment_tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());

		let verification_result_byte = data.drain(..1).as_slice()[0];
		let verification_result = if verification_result_byte == 1 { true } else { false };

		let signature = Signature::from_bytes(data.drain(..64).into_iter().collect());

		Self { tx_hash, job_run_assignment_tx_hash, verification_result, signature }
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.tx_hash.to_bytes());
		bytes.extend(self.job_run_assignment_tx_hash.to_bytes());
		bytes.push(if self.verification_result { 1 } else { 0 });
		bytes.extend(self.signature.to_bytes());
		bytes
	}
}
