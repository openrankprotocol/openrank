use crate::topics::DomainHash;
use crate::{db::DbItem, merkle::Hash};
use alloy_rlp::{encode, BufMut, Decodable, Encodable, Error as RlpError, Result as RlpResult};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use hex::FromHex;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::io::Read;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum TxKind {
	TrustUpdate,
	SeedUpdate,
	JobRunRequest,
	JobRunAssignment,
	CreateScores,
	CreateCommitment,
	JobVerification,
	ProposedBlock,
	FinalisedBlock,
}

impl Encodable for TxKind {
	fn encode(&self, out: &mut dyn BufMut) {
		out.put_u8(*self as u8);
	}
}

impl Decodable for TxKind {
	fn decode(buf: &mut &[u8]) -> RlpResult<Self> {
		let mut bytes = [0; 1];
		let size = buf.read(&mut bytes).map_err(|_| RlpError::Custom("Failed to read bytes"))?;
		if size != 1 {
			return RlpResult::Err(RlpError::UnexpectedLength);
		}

		Ok(TxKind::from_byte(bytes[0]))
	}
}

impl TxKind {
	pub fn from_byte(byte: u8) -> Self {
		match byte {
			0 => Self::TrustUpdate,
			1 => Self::SeedUpdate,
			2 => Self::JobRunRequest,
			3 => Self::JobRunAssignment,
			4 => Self::CreateScores,
			5 => Self::CreateCommitment,
			6 => Self::JobVerification,
			7 => Self::ProposedBlock,
			8 => Self::FinalisedBlock,
			_ => panic!("Invalid message type"),
		}
	}
}

impl Into<String> for TxKind {
	fn into(self) -> String {
		match self {
			Self::TrustUpdate => "trust_update".to_string(),
			Self::SeedUpdate => "seed_update".to_string(),
			Self::JobRunRequest => "job_run_request".to_string(),
			Self::JobRunAssignment => "job_run_assignment".to_string(),
			Self::CreateScores => "create_scores".to_string(),
			Self::CreateCommitment => "create_commitment".to_string(),
			Self::JobVerification => "job_verification".to_string(),
			Self::ProposedBlock => "proposed_block".to_string(),
			Self::FinalisedBlock => "finalised_block".to_string(),
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable, Serialize, Deserialize)]
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

	pub fn hash(&self) -> Vec<u8> {
		let mut hasher = Keccak256::new();
		hasher.update(&self.nonce.to_be_bytes());
		hasher.update(encode(&self.from));
		hasher.update(encode(&self.to));
		hasher.update(encode(self.kind));
		hasher.update(&self.body);
		let result = hasher.finalize();
		result.to_vec()
	}
}

impl DbItem for Tx {
	fn get_key(&self) -> Vec<u8> {
		self.hash()
	}

	fn get_cf() -> String {
		"tx".to_string()
	}

	fn get_prefix(&self) -> String {
		self.kind.into()
	}
}

#[derive(Debug, Clone, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize)]
pub struct OwnedNamespace(#[serde(with = "hex")] pub [u8; 24]);

impl OwnedNamespace {
	pub fn new(owner: Address, id: u32) -> Self {
		let mut bytes = [0; 24];
		bytes[..20].copy_from_slice(&owner.0);
		bytes[20..24].copy_from_slice(&id.to_be_bytes());
		Self(bytes)
	}

	pub fn to_hex(self) -> String {
		hex::encode(self.0)
	}
}

impl FromHex for OwnedNamespace {
	type Error = hex::FromHexError;

	fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
		Ok(OwnedNamespace(<[u8; 24]>::from_hex(hex)?))
	}
}

#[derive(
	Debug, Clone, PartialEq, Eq, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize,
)]
pub struct Address(#[serde(with = "hex")] pub [u8; 20]);

impl FromHex for Address {
	type Error = hex::FromHexError;

	fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
		Ok(Address(<[u8; 20]>::from_hex(hex)?))
	}
}

#[derive(Debug, Clone, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize)]
pub struct TxHash(#[serde(with = "hex")] [u8; 32]);

#[derive(
	Debug, Clone, PartialEq, Eq, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize,
)]
pub struct Signature {
	s: [u8; 32],
	r: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ScoreEntry {
	id: u128,
	value: f32,
}

impl Encodable for ScoreEntry {
	fn encode(&self, out: &mut dyn BufMut) {
		self.id.encode(out);
		out.put_f32(self.value);
	}
}

impl Decodable for ScoreEntry {
	fn decode(buf: &mut &[u8]) -> RlpResult<Self> {
		let id = u128::decode(buf)?;
		let mut value_bytes = [0; 4];
		let size =
			buf.read(&mut value_bytes).map_err(|_| RlpError::Custom("Failed to read bytes"))?;
		if size != 4 {
			return RlpResult::Err(RlpError::UnexpectedLength);
		}
		let value = f32::from_be_bytes(value_bytes);
		Ok(ScoreEntry { id, value })
	}
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TrustEntry {
	from: u128,
	to: u128,
	value: f32,
}

impl Encodable for TrustEntry {
	fn encode(&self, out: &mut dyn BufMut) {
		self.from.encode(out);
		self.to.encode(out);
		out.put_f32(self.value);
	}
}

impl Decodable for TrustEntry {
	fn decode(buf: &mut &[u8]) -> RlpResult<Self> {
		let from = u128::decode(buf)?;
		let to = u128::decode(buf)?;
		let mut value_bytes = [0; 4];
		let size =
			buf.read(&mut value_bytes).map_err(|_| RlpError::Custom("Failed to read bytes"))?;
		if size != 4 {
			return RlpResult::Err(RlpError::UnexpectedLength);
		}
		let value = f32::from_be_bytes(value_bytes);
		Ok(TrustEntry { from, to, value })
	}
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct CreateCommitment {
	job_run_assignment_tx_hash: TxHash,
	lt_root_hash: Hash,
	compute_root_hash: Hash,
	scores_tx_hashes: Vec<TxHash>,
	new_trust_tx_hashes: Vec<TxHash>,
	new_seed_tx_hashes: Vec<TxHash>,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct CreateScores {
	entries: Vec<ScoreEntry>,
}

// JOB_ID = hash(domain_id, da_block_height, from)
#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct JobRunRequest {
	pub domain_id: DomainHash,
	pub block_height: u32,
}

impl JobRunRequest {
	pub fn new(domain_id: DomainHash, block_height: u32) -> Self {
		Self { domain_id, block_height }
	}
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct JobRunAssignment {
	job_run_request_tx_hash: TxHash,
	assigned_compute_node: Address,
	assigned_verifier_node: Address,
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct JobVerification {
	job_run_assignment_tx_hash: TxHash,
	verification_result: bool,
}

impl Default for JobVerification {
	fn default() -> Self {
		Self { job_run_assignment_tx_hash: TxHash::default(), verification_result: true }
	}
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
struct PendingDomainUpdate {
	domain_id: DomainHash,
	commitment_tx_hash: TxHash,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
struct DomainUpdate {
	domain_id: DomainHash,
	commitment_tx_hash: TxHash,
	verification_results_tx_hashes: Vec<TxHash>,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct ProposedBlock {
	previous_block_hash: TxHash,
	state_root: Hash,
	pending_domain_updates: Vec<PendingDomainUpdate>,
	timestamp: u32,
	block_height: u32,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct FinalisedBlock {
	previous_block_hash: TxHash,
	state_root: Hash,
	domain_updates: Vec<DomainUpdate>,
	timestamp: u32,
	block_height: u32,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct TrustUpdate {
	pub trust_id: OwnedNamespace,
	entries: Vec<TrustEntry>,
}

impl TrustUpdate {
	pub fn new(trust_id: OwnedNamespace, entries: Vec<TrustEntry>) -> Self {
		Self { trust_id, entries }
	}
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct SeedUpdate {
	seed_id: OwnedNamespace,
	entries: Vec<ScoreEntry>,
}

impl SeedUpdate {
	pub fn new(seed_id: OwnedNamespace, entries: Vec<ScoreEntry>) -> Self {
		Self { seed_id, entries }
	}
}

#[cfg(test)]
mod test {
	use crate::txs::TrustEntry;

	use super::{ScoreEntry, TrustUpdate, Tx, TxKind};
	use alloy_rlp::{encode, Decodable};

	#[test]
	fn test_decode_tx_kind() {
		let res = TxKind::decode(&mut [0].as_slice()).unwrap();
		assert_eq!(res, TxKind::TrustUpdate);
		let res = TxKind::decode(&mut [3].as_slice()).unwrap();
		assert_eq!(res, TxKind::JobRunAssignment);
		let res = TxKind::decode(&mut [8].as_slice()).unwrap();
		assert_eq!(res, TxKind::FinalisedBlock);
	}

	#[test]
	fn test_tx_to_hash() {
		let tx = Tx::default_with(TxKind::TrustUpdate, encode(TrustUpdate::default()));
		let tx_hash = tx.hash();
		assert_eq!(
			hex::encode(tx_hash),
			"77b37e9a80c4d4bb476f67b0a6523e6dc41ca8fc255d583db03d62e1b67b73dc"
		);
	}

	#[test]
	fn test_decode_score_entry() {
		let se = ScoreEntry::default();
		let encoded_se = encode(se.clone());
		let decoded_se = ScoreEntry::decode(&mut encoded_se.as_slice()).unwrap();
		assert_eq!(se, decoded_se);
	}

	#[test]
	fn test_decode_trust_entry() {
		let te = TrustEntry::default();
		let encoded_te = encode(te.clone());
		let decoded_te = TrustEntry::decode(&mut encoded_te.as_slice()).unwrap();
		assert_eq!(te, decoded_te);
	}
}
