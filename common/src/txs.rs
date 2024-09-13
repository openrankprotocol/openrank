use crate::merkle::hash_leaf;
use crate::topics::DomainHash;
use crate::{db::DbItem, merkle::Hash};
use alloy_rlp::{encode, BufMut, Decodable, Encodable, Error as RlpError, Result as RlpResult};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use hex::FromHex;
use k256::ecdsa::signature::hazmat::PrehashVerifier;
use k256::ecdsa::{
    Error as EcdsaError, RecoveryId, Signature as EcdsaSignature, SigningKey, VerifyingKey,
};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::fmt::Display;
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
#[rlp(trailing)]
pub struct Tx {
    nonce: u64,
    from: Address,
    // Use 0x0 for transactions intended to be processed by the network
    to: Address,
    kind: TxKind,
    body: Vec<u8>,
    signature: Signature,
    sequence_number: Option<u64>,
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
            sequence_number: None,
        }
    }

    pub fn kind(&self) -> TxKind {
        self.kind
    }

    pub fn body(&self) -> Vec<u8> {
        self.body.clone()
    }

    pub fn hash(&self) -> TxHash {
        let mut hasher = Keccak256::new();
        hasher.update(&self.nonce.to_be_bytes());
        hasher.update(encode(&self.from));
        hasher.update(encode(&self.to));
        hasher.update(encode(self.kind));
        hasher.update(&self.body);
        let result = hasher.finalize();
        let bytes = result.to_vec();

        let mut tx_bytes = [0; 32];
        tx_bytes.copy_from_slice(&bytes);
        TxHash(tx_bytes)
    }

    pub fn construct_full_key(kind: TxKind, tx_hash: TxHash) -> Vec<u8> {
        let kind_string: String = kind.into();
        let mut prefix = kind_string.as_bytes().to_vec();
        prefix.extend(tx_hash.0);
        prefix
    }

    pub fn sign(&mut self, sk: &SigningKey) -> Result<(), EcdsaError> {
        let (sig, rec) = sk.sign_prehash_recoverable(self.hash().as_bytes())?;
        let s: [u8; 32] = sig.s().to_bytes().into();
        let r: [u8; 32] = sig.r().to_bytes().into();
        self.signature = Signature::new(s, r, rec.to_byte());
        Ok(())
    }

    pub fn verify_against(&self, address: Address) -> Result<(), EcdsaError> {
        let mut bytes = Vec::new();
        bytes.extend(self.signature.r);
        bytes.extend(self.signature.s);
        let message = self.hash().to_bytes();

        let sig = EcdsaSignature::try_from(bytes.as_slice())?;
        let rec_id = RecoveryId::from_byte(self.signature.r_id).ok_or(EcdsaError::new())?;
        let verifying_key = VerifyingKey::recover_from_prehash(&message, &sig, rec_id)?;

        let uncompressed_point = verifying_key.to_encoded_point(false);
        let vk_bytes = uncompressed_point.as_bytes();

        let hash = hash_leaf::<Keccak256>(vk_bytes[1..].to_vec());
        let mut address_bytes = [0u8; 20];
        address_bytes.copy_from_slice(&hash.0[12..]);

        if Address(address_bytes) != address {
            return Err(EcdsaError::new());
        }

        verifying_key.verify_prehash(&message, &sig)
    }

    pub fn verify(&self) -> Result<Address, EcdsaError> {
        let mut bytes = Vec::new();
        bytes.extend(self.signature.r);
        bytes.extend(self.signature.s);
        let message = self.hash().to_bytes();

        let sig = EcdsaSignature::try_from(bytes.as_slice())?;
        let rec_id = RecoveryId::from_byte(self.signature.r_id).ok_or(EcdsaError::new())?;
        let verifying_key = VerifyingKey::recover_from_prehash(&message, &sig, rec_id)?;
        verifying_key.verify_prehash(&message, &sig)?;

        let uncompressed_point = verifying_key.to_encoded_point(false);
        let vk_bytes = uncompressed_point.as_bytes();

        let hash = hash_leaf::<Keccak256>(vk_bytes[1..].to_vec());
        let mut address_bytes = [0u8; 20];
        address_bytes.copy_from_slice(&hash.0[12..]);
        let address = Address(address_bytes);

        Ok(address)
    }

    pub fn set_sequence_number(&mut self, sequence_number: u64) {
        self.sequence_number = Some(sequence_number);
    }

    pub fn sequence_number(&self) -> u64 {
        self.sequence_number.unwrap_or_default()
    }
}

impl DbItem for Tx {
    fn get_key(&self) -> Vec<u8> {
        self.hash().0.to_vec()
    }

    fn get_cf() -> String {
        "tx".to_string()
    }

    fn get_prefix(&self) -> String {
        self.kind.into()
    }
}

#[derive(
    Debug, Clone, Hash, Default, PartialEq, Eq, RlpDecodable, RlpEncodable, Serialize, Deserialize,
)]
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

    pub fn owner(&self) -> Address {
        let mut bytes = [0; 20];
        bytes.copy_from_slice(&self.0[..20]);
        Address(bytes)
    }
}

impl FromHex for OwnedNamespace {
    type Error = hex::FromHexError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        Ok(OwnedNamespace(<[u8; 24]>::from_hex(hex)?))
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize, Hash,
)]
pub struct Address(#[serde(with = "hex")] pub [u8; 20]);

impl From<u32> for Address {
    fn from(value: u32) -> Self {
        let mut bytes = [0; 20];
        bytes[..4].copy_from_slice(&value.to_be_bytes());
        Address(bytes)
    }
}

impl Address {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("Address({})", self.to_hex()))
    }
}

impl FromHex for Address {
    type Error = hex::FromHexError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        Ok(Address(<[u8; 20]>::from_hex(hex)?))
    }
}

#[derive(
    Debug, Clone, Hash, PartialEq, Eq, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize,
)]
pub struct TxHash(#[serde(with = "hex")] pub [u8; 32]);

impl TxHash {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let mut inner = [0u8; 32];
        inner.copy_from_slice(bytes.as_slice());
        Self(inner)
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize,
)]
pub struct Signature {
    pub s: [u8; 32],
    pub r: [u8; 32],
    r_id: u8,
}

impl Signature {
    pub fn new(s: [u8; 32], r: [u8; 32], r_id: u8) -> Self {
        Self { s, r, r_id }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ScoreEntry {
    pub id: Address,
    pub value: f32,
}

impl ScoreEntry {
    pub fn new(id: Address, value: f32) -> Self {
        Self { id, value }
    }
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

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TrustEntry {
    pub from: Address,
    pub to: Address,
    pub value: f32,
}

impl TrustEntry {
    pub fn new(from: Address, to: Address, value: f32) -> Self {
        Self { from, to, value }
    }
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
    pub job_run_assignment_tx_hash: TxHash,
    pub lt_root_hash: Hash,
    pub compute_root_hash: Hash,
    pub scores_tx_hashes: Vec<TxHash>,
    new_trust_tx_hashes: Vec<TxHash>,
    new_seed_tx_hashes: Vec<TxHash>,
}

impl CreateCommitment {
    pub fn default_with(
        job_run_assignment_tx_hash: TxHash, lt_root_hash: Hash, compute_root_hash: Hash,
        scores_tx_hashes: Vec<TxHash>,
    ) -> Self {
        Self {
            job_run_assignment_tx_hash,
            lt_root_hash,
            compute_root_hash,
            scores_tx_hashes,
            new_trust_tx_hashes: Vec::new(),
            new_seed_tx_hashes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct CreateScores {
    pub entries: Vec<ScoreEntry>,
}

impl CreateScores {
    pub fn new(entries: Vec<ScoreEntry>) -> Self {
        Self { entries }
    }
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct JobRunRequest {
    pub domain_id: DomainHash,
    pub block_height: u32,
    pub job_id: Hash,
}

impl JobRunRequest {
    pub fn new(domain_id: DomainHash, block_height: u32, job_id: Hash) -> Self {
        Self { domain_id, block_height, job_id }
    }
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct JobRunAssignment {
    pub job_run_request_tx_hash: TxHash,
    pub assigned_compute_node: Address,
    pub assigned_verifier_node: Address,
}

impl JobRunAssignment {
    pub fn default_with(job_run_request_tx_hash: TxHash) -> Self {
        Self {
            job_run_request_tx_hash,
            assigned_compute_node: Address::default(),
            assigned_verifier_node: Address::default(),
        }
    }

    pub fn new(
        job_run_request_tx_hash: TxHash, assigned_compute_node: Address,
        assigned_verifier_node: Address,
    ) -> Self {
        Self { job_run_request_tx_hash, assigned_compute_node, assigned_verifier_node }
    }
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct JobVerification {
    pub job_run_assignment_tx_hash: TxHash,
    pub verification_result: bool,
}

impl JobVerification {
    pub fn new(job_run_assignment_tx_hash: TxHash, verification_result: bool) -> Self {
        Self { job_run_assignment_tx_hash, verification_result }
    }
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
    pub entries: Vec<TrustEntry>,
}

impl TrustUpdate {
    pub fn new(trust_id: OwnedNamespace, entries: Vec<TrustEntry>) -> Self {
        Self { trust_id, entries }
    }
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct SeedUpdate {
    pub seed_id: OwnedNamespace,
    pub entries: Vec<ScoreEntry>,
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
            hex::encode(tx_hash.0),
            "1e0b2851b535b9f656dff05d63cd82dff31c6dc31120fde49295e9b797021c2b"
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
