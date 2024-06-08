pub struct Address(pub [u8; 32]);

impl Address {
	fn from_bytes(mut data: Vec<u8>) -> Self {
		let mut bytes = [0; 32];
		bytes.copy_from_slice(data.drain(..32).as_slice());
		Self(bytes)
	}
}

struct TxHash([u8; 32]);

impl TxHash {
	fn from_bytes(mut data: Vec<u8>) -> Self {
		let mut bytes = [0; 32];
		bytes.copy_from_slice(data.drain(..32).as_slice());
		Self(bytes)
	}
}

struct RootHash([u8; 32]);

impl RootHash {
	fn from_bytes(mut data: Vec<u8>) -> Self {
		let mut bytes = [0; 32];
		bytes.copy_from_slice(data.drain(..32).as_slice());
		Self(bytes)
	}
}

struct Signature {
	s: [u8; 32],
	r: [u8; 32],
}

impl Signature {
	fn from_bytes(mut data: Vec<u8>) -> Self {
		let mut s_bytes = [0; 32];
		let mut r_bytes = [0; 32];
		s_bytes.copy_from_slice(data.drain(..32).as_slice());
		r_bytes.copy_from_slice(data.drain(..32).as_slice());
		Self { s: s_bytes, r: r_bytes }
	}
}

struct Entry {
	id: Address,
	value: f32,
}

impl Entry {
	fn from_bytes(mut data: Vec<u8>) -> Self {
		let address = Address::from_bytes(data.drain(..32).into_iter().collect());
		let mut value_bytes = [0; 4];
		value_bytes.copy_from_slice(data.drain(..4).as_slice());
		let value = f32::from_be_bytes(value_bytes);
		Self { id: address, value }
	}
}

struct CreateCommitment {
	tx_hash: TxHash,
	job_run_assignment_tx_hash: TxHash,
	lt_root_hash: RootHash,
	compute_root_hash: RootHash,
	scores_tx_hashes: Vec<TxHash>,
	new_trust_tx_hashes: Vec<TxHash>,
	new_seed_tx_hashes: Vec<TxHash>,
	signature: Signature,
}

impl CreateCommitment {
	fn from_bytes(mut data: Vec<u8>) -> Self {
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
}

struct CreateScores {
	tx_hash: TxHash,
	entries: Vec<Entry>,
	signature: Signature,
}

impl CreateScores {
	fn from_bytes(mut data: Vec<u8>) -> Self {
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
}

struct JobRunAssignment {
	tx_hash: TxHash,
	job_run_request_tx_hash: TxHash,
	assigned_compute_node: Address,
	assigned_verifier_node: Address,
	signature: Signature,
}

impl JobRunAssignment {
	fn from_bytes(mut data: Vec<u8>) -> Self {
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
}

struct JobVerification {
	tx_hash: TxHash,
	job_run_assignment_tx_hash: TxHash,
	verification_result: bool,
	signature: Signature,
}

impl JobVerification {
	fn from_bytes(mut data: Vec<u8>) -> Self {
		let tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());
		let job_run_assignment_tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());

		let verification_result_byte = data.drain(..1).as_slice()[0];
		let verification_result = if verification_result_byte == 1 { true } else { false };

		let signature = Signature::from_bytes(data.drain(..64).into_iter().collect());

		Self { tx_hash, job_run_assignment_tx_hash, verification_result, signature }
	}
}
