use crate::topics::DomainHash;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum TxKind {
	JobRunRequest,
	JobRunAssignment,
	CreateScores,
	CreateCommitment,
	JobVerification,
	ProposedBlock,
	FinalisedBlock,
}

impl TxKind {
	pub fn from_byte(byte: u8) -> Self {
		match byte {
			0 => Self::JobRunRequest,
			1 => Self::JobRunAssignment,
			2 => Self::CreateScores,
			3 => Self::CreateCommitment,
			4 => Self::JobVerification,
			5 => Self::ProposedBlock,
			6 => Self::FinalisedBlock,
			_ => panic!("Invalid message type"),
		}
	}
}

#[derive(Debug, Clone)]
pub struct Tx {
	nonce: u64,
	from: Address,
	// Use 0x0 for transactions intended to be processed by the network
	to: Address,
	kind: TxKind,
	body: Vec<u8>,
	signature: Signature,
}

impl Tx {
	pub fn default_with(kind: TxKind, body: Vec<u8>) -> Self {
		Self {
			nonce: 0,
			from: Address::default(),
			to: Address::default(),
			kind,
			body,
			signature: Signature::default(),
		}
	}

	pub fn kind(&self) -> TxKind {
		self.kind
	}

	pub fn body(&self) -> Vec<u8> {
		self.body.clone()
	}
}

impl Tx {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let mut nonce_bytes = [0; 8];
		nonce_bytes.copy_from_slice(data.drain(..8).as_slice());
		let nonce = u64::from_be_bytes(nonce_bytes);

		let from = Address::from_bytes(data.drain(..32).into_iter().collect());
		let to = Address::from_bytes(data.drain(..32).into_iter().collect());

		let kind = TxKind::from_byte(data.drain(..1).as_slice()[0]);
		let body_len = data.drain(..1).as_slice()[0] as usize;
		let body = data.drain(..body_len).into_iter().collect();

		let signature = Signature::from_bytes(data.drain(..).into_iter().collect());

		Self { nonce, from, to, kind, body, signature }
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.nonce.to_be_bytes());
		bytes.extend(self.from.to_bytes());
		bytes.extend(self.to.to_bytes());
		bytes.push(self.kind as u8);
		bytes.push(self.body.len() as u8);
		bytes.extend(self.body.clone());
		bytes.extend(self.signature.to_bytes());
		bytes
	}
}

#[derive(Debug, Clone, Default)]
pub struct Address(pub [u8; 32]);

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

#[derive(Debug, Clone, Default)]
pub struct TxHash([u8; 32]);

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

#[derive(Debug, Clone, Default)]
pub struct RootHash([u8; 32]);

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

#[derive(Debug, Clone, Default)]
pub struct Signature {
	s: [u8; 32],
	r: [u8; 32],
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

#[derive(Debug, Clone, Default)]
pub struct Entry {
	id: Address,
	value: f32,
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

#[derive(Debug, Clone, Default)]
pub struct CreateCommitment {
	job_run_assignment_tx_hash: TxHash,
	lt_root_hash: RootHash,
	compute_root_hash: RootHash,
	scores_tx_hashes: Vec<TxHash>,
	new_trust_tx_hashes: Vec<TxHash>,
	new_seed_tx_hashes: Vec<TxHash>,
}

impl CreateCommitment {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
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

		Self {
			job_run_assignment_tx_hash: jra_tx_hash,
			lt_root_hash,
			compute_root_hash: c_root_hash,
			scores_tx_hashes: scores_tx,
			new_trust_tx_hashes: new_trust_txs,
			new_seed_tx_hashes: new_seed_txs,
		}
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();

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

		bytes
	}
}

#[derive(Debug, Clone, Default)]
pub struct CreateScores {
	entries: Vec<Entry>,
}

impl CreateScores {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
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

		Self { entries: entries_txs }
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();

		let entries_len = self.entries.len() as u8;
		bytes.push(entries_len);
		for tx in &self.entries {
			bytes.extend(tx.to_bytes());
		}

		bytes
	}
}

// JOB_ID = hash(domain_id, da_block_height, from)
#[derive(Debug, Clone, Default)]
pub struct JobRunRequest {
	domain_id: DomainHash,
	da_block_height: u32,
}

impl JobRunRequest {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let domain_id = DomainHash::from_bytes(data.drain(..8).into_iter().collect());

		let mut da_block_height_bytes = [0; 4];
		da_block_height_bytes.copy_from_slice(data.drain(..4).as_slice());
		let da_block_height = u32::from_be_bytes(da_block_height_bytes);

		Self { domain_id, da_block_height }
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.domain_id.to_bytes());
		bytes.extend(self.da_block_height.to_be_bytes());
		bytes
	}
}

#[derive(Debug, Clone, Default)]
pub struct JobRunAssignment {
	job_run_request_tx_hash: TxHash,
	assigned_compute_node: Address,
	assigned_verifier_node: Address,
}

impl JobRunAssignment {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let job_run_request_tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());

		let assigned_compute_node = Address::from_bytes(data.drain(..32).into_iter().collect());
		let assigned_verifier_node = Address::from_bytes(data.drain(..32).into_iter().collect());

		Self { job_run_request_tx_hash, assigned_compute_node, assigned_verifier_node }
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.job_run_request_tx_hash.to_bytes());
		bytes.extend(self.assigned_compute_node.to_bytes());
		bytes.extend(self.assigned_verifier_node.to_bytes());
		bytes
	}
}

#[derive(Debug, Clone)]
pub struct JobVerification {
	job_run_assignment_tx_hash: TxHash,
	verification_result: bool,
}

impl Default for JobVerification {
	fn default() -> Self {
		Self { job_run_assignment_tx_hash: TxHash::default(), verification_result: true }
	}
}

impl JobVerification {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let job_run_assignment_tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());

		let verification_result_byte = data.drain(..1).as_slice()[0];
		let verification_result = if verification_result_byte == 1 { true } else { false };

		Self { job_run_assignment_tx_hash, verification_result }
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.job_run_assignment_tx_hash.to_bytes());
		bytes.push(if self.verification_result { 1 } else { 0 });
		bytes
	}
}

#[derive(Debug, Clone, Default)]
struct PendingDomainUpdate {
	domain_id: DomainHash,
	commitment_tx_hash: TxHash,
}

impl PendingDomainUpdate {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let domain_hash = DomainHash::from_bytes(data.drain(..8).into_iter().collect());
		let commitment_tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());

		Self { domain_id: domain_hash, commitment_tx_hash }
	}
	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.domain_id.to_bytes());
		bytes.extend(self.commitment_tx_hash.to_bytes());
		bytes
	}
}

#[derive(Debug, Clone, Default)]
struct DomainUpdate {
	domain_id: DomainHash,
	commitment_tx_hash: TxHash,
	verification_results_tx_hashes: Vec<TxHash>,
}

impl DomainUpdate {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let domain_hash = DomainHash::from_bytes(data.drain(..8).into_iter().collect());
		let commitment_tx_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());

		let mut verification_txs = Vec::new();
		for chunk in data.chunks(32) {
			let verification_tx = TxHash::from_bytes(chunk.to_vec());
			verification_txs.push(verification_tx);
		}

		Self {
			domain_id: domain_hash,
			commitment_tx_hash,
			verification_results_tx_hashes: verification_txs,
		}
	}
	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.domain_id.to_bytes());
		bytes.extend(self.commitment_tx_hash.to_bytes());
		for tx in &self.verification_results_tx_hashes {
			bytes.extend(tx.to_bytes());
		}
		bytes
	}
}

#[derive(Debug, Clone, Default)]
pub struct ProposedBlock {
	previous_block_hash: TxHash,
	state_root: RootHash,
	pending_domain_updates: Vec<PendingDomainUpdate>,
	timestamp: u32,
	block_height: u32,
}

impl ProposedBlock {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let previous_block_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());
		let state_root = RootHash::from_bytes(data.drain(..32).into_iter().collect());
		let pending_domain_updates_len = data.drain(..1).as_slice()[0];
		let mut pending_domain_updates = Vec::new();
		for _ in 0..pending_domain_updates_len {
			let pending_domain_update_len = 8 + 32;
			let pending_domain_update = PendingDomainUpdate::from_bytes(
				data.drain(..pending_domain_update_len as usize).into_iter().collect(),
			);
			pending_domain_updates.push(pending_domain_update);
		}

		let mut timestamp_bytes = [0; 4];
		timestamp_bytes.copy_from_slice(data.drain(..4).as_slice());
		let timestamp = u32::from_be_bytes(timestamp_bytes);

		let mut block_height_bytes = [0; 4];
		block_height_bytes.copy_from_slice(data.drain(..4).as_slice());
		let block_height = u32::from_be_bytes(block_height_bytes);

		Self { previous_block_hash, state_root, pending_domain_updates, timestamp, block_height }
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.previous_block_hash.to_bytes());
		bytes.extend(self.state_root.to_bytes());

		let pending_domain_updates_len = (8 + 32) * self.pending_domain_updates.len() as u8;
		bytes.push(pending_domain_updates_len);

		for pdu in &self.pending_domain_updates {
			bytes.extend(pdu.to_bytes());
		}

		bytes.extend(&self.timestamp.to_be_bytes());
		bytes.extend(&self.block_height.to_be_bytes());

		bytes
	}
}

#[derive(Debug, Clone, Default)]
pub struct FinalisedBlock {
	previous_block_hash: TxHash,
	state_root: RootHash,
	domain_updates: Vec<DomainUpdate>,
	timestamp: u32,
	block_height: u32,
}

impl FinalisedBlock {
	pub fn from_bytes(mut data: Vec<u8>) -> Self {
		let previous_block_hash = TxHash::from_bytes(data.drain(..32).into_iter().collect());
		let state_root = RootHash::from_bytes(data.drain(..32).into_iter().collect());
		let domain_updates_len = data.drain(..1).as_slice()[0];
		let mut domain_updates = Vec::new();
		for _ in 0..domain_updates_len {
			let domain_update_len = data.drain(..1).as_slice()[0];
			let domain_update = DomainUpdate::from_bytes(
				data.drain(..domain_update_len as usize).into_iter().collect(),
			);
			domain_updates.push(domain_update);
		}

		let mut timestamp_bytes = [0; 4];
		timestamp_bytes.copy_from_slice(data.drain(..4).as_slice());
		let timestamp = u32::from_be_bytes(timestamp_bytes);

		let mut block_height_bytes = [0; 4];
		block_height_bytes.copy_from_slice(data.drain(..4).as_slice());
		let block_height = u32::from_be_bytes(block_height_bytes);

		Self { previous_block_hash, state_root, domain_updates, timestamp, block_height }
	}
	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.previous_block_hash.to_bytes());
		bytes.extend(self.state_root.to_bytes());

		let mut pending_domain_updates_len = 0;
		for pdu in &self.domain_updates {
			let domain_id_len = 8;
			let commitment_tx_hash_len = 32;
			let ver_results_len = pdu.verification_results_tx_hashes.len() as u8 * 32;
			pending_domain_updates_len += domain_id_len + commitment_tx_hash_len + ver_results_len;
		}
		bytes.push(pending_domain_updates_len);

		for pdu in &self.domain_updates {
			let domain_id_len = 8;
			let commitment_tx_hash_len = 32;
			let ver_results_len = pdu.verification_results_tx_hashes.len() as u8 * 32;
			bytes.push(domain_id_len + commitment_tx_hash_len + ver_results_len);
			bytes.extend(pdu.to_bytes());
		}

		bytes.extend(&self.timestamp.to_be_bytes());
		bytes.extend(&self.block_height.to_be_bytes());

		bytes
	}
}
