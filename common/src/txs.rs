use crate::topics::DomainHash;
use alloy_rlp::{BufMut, Decodable, Encodable, Error as RlpError, Result as RlpResult};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use hex::FromHex;
use serde::{Deserialize, Serialize};
use std::io::Read;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
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

#[derive(Debug, Clone, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize)]
pub struct OwnedNamespace(pub [u8; 32]);

#[derive(Debug, Clone, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize)]
pub struct Address(pub [u8; 32]);

impl FromHex for Address {
	type Error = hex::FromHexError;

	fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
		Ok(Address(<[u8; 32]>::from_hex(hex)?))
	}
}

#[derive(Debug, Clone, Default, RlpDecodable, RlpEncodable)]
pub struct TxHash([u8; 32]);

#[derive(Debug, Clone, Default, RlpDecodable, RlpEncodable)]
pub struct RootHash([u8; 32]);

#[derive(Debug, Clone, Default, RlpDecodable, RlpEncodable)]
pub struct Signature {
	s: [u8; 32],
	r: [u8; 32],
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoreEntry {
	id: Address,
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
		let id = Address::decode(buf)?;
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustEntry {
	from: Address,
	to: Address,
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
		let from = Address::decode(buf)?;
		let to = Address::decode(buf)?;
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
	lt_root_hash: RootHash,
	compute_root_hash: RootHash,
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
	domain_id: DomainHash,
	da_block_height: u32,
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
	state_root: RootHash,
	pending_domain_updates: Vec<PendingDomainUpdate>,
	timestamp: u32,
	block_height: u32,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct FinalisedBlock {
	previous_block_hash: TxHash,
	state_root: RootHash,
	domain_updates: Vec<DomainUpdate>,
	timestamp: u32,
	block_height: u32,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct TrustUpdate {
	trust_id: OwnedNamespace,
	entries: Vec<TrustEntry>,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct SeedUpdate {
	seed_id: OwnedNamespace,
	entries: Vec<ScoreEntry>,
}
