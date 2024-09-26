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
/// Kind of transaction.
pub enum TxKind {
    /// Transaction for the trust update.
    TrustUpdate,
    /// Transaction for the seed update.
    SeedUpdate,
    /// Transaction for the job run request.
    JobRunRequest,
    /// Transaction for the job run assignent.
    JobRunAssignment,
    /// Transaction for the scores creation after compute.
    CreateScores,
    /// Transaction for the commitment creation.
    CreateCommitment,
    /// Transaction for the job verification.
    JobVerification,
    /// Transaction for the proposed block.
    ProposedBlock,
    /// Transaction for the finalised block.
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
/// Transaction structure.
pub struct Tx {
    /// Nonce of the transaction.
    nonce: u64,
    /// Address of the sender.
    from: Address,
    /// Address of the receiver.
    /// Use 0x0 for transactions intended to be processed by the network.
    to: Address,
    /// Kind of the transaction.
    kind: TxKind,
    /// Body of the transaction.
    body: Vec<u8>,
    /// Signature of the transaction.
    signature: Signature,
    /// Sequence number of the transaction.
    /// Used for ordering transactions.
    // TODO: Remove this field after trust commit upgrade.
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

    /// Returns the kind of the transaction.
    pub fn kind(&self) -> TxKind {
        self.kind
    }

    /// Returns the body of the transaction.
    pub fn body(&self) -> Vec<u8> {
        self.body.clone()
    }

    /// Returns the hash of the transaction.
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

    /// Constructs the full key for the given tx kind and tx hash.
    pub fn construct_full_key(kind: TxKind, tx_hash: TxHash) -> Vec<u8> {
        let kind_string: String = kind.into();
        let mut prefix = kind_string.as_bytes().to_vec();
        prefix.extend(tx_hash.0);
        prefix
    }

    /// Signs the transaction with the given private key.
    pub fn sign(&mut self, sk: &SigningKey) -> Result<(), EcdsaError> {
        let (sig, rec) = sk.sign_prehash_recoverable(self.hash().as_bytes())?;
        let s: [u8; 32] = sig.s().to_bytes().into();
        let r: [u8; 32] = sig.r().to_bytes().into();
        self.signature = Signature::new(s, r, rec.to_byte());
        Ok(())
    }

    /// Verifies the transaction signature against the given address.
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

    /// Verifies the transaction signature.
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
/// Namespace owned by an address.
pub struct OwnedNamespace(#[serde(with = "hex")] pub [u8; 24]);

impl OwnedNamespace {
    /// Creates a new owned namespace.
    pub fn new(owner: Address, id: u32) -> Self {
        let mut bytes = [0; 24];
        bytes[..20].copy_from_slice(&owner.0);
        bytes[20..24].copy_from_slice(&id.to_be_bytes());
        Self(bytes)
    }

    /// Returns the hex string of the owned namespace.
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    /// Returns the owner of the owned namespace.
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
/// Address of the transaction sender.
pub struct Address(#[serde(with = "hex")] pub [u8; 20]);

impl From<u32> for Address {
    fn from(value: u32) -> Self {
        let mut bytes = [0; 20];
        bytes[..4].copy_from_slice(&value.to_be_bytes());
        Address(bytes)
    }
}

impl Address {
    /// Returns the hex string of the address.
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
/// Hash of the transaction.
pub struct TxHash(#[serde(with = "hex")] pub [u8; 32]);

impl TxHash {
    /// Creates a new tx hash from the given bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let mut inner = [0u8; 32];
        inner.copy_from_slice(bytes.as_slice());
        Self(inner)
    }

    /// Returns the bytes of the tx hash.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// Returns the bytes of the tx hash.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// Returns the hex string of the tx hash.
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize,
)]
/// Ecdsa signature using the secp256k1 curve (https://w3c-ccg.github.io/lds-ecdsa-secp256k1-2019/).
pub struct Signature {
    /// Signature s value.
    pub s: [u8; 32],
    /// Signature r value.
    pub r: [u8; 32],
    /// Signature recovery id.
    r_id: u8,
}

impl Signature {
    pub fn new(s: [u8; 32], r: [u8; 32], r_id: u8) -> Self {
        Self { s, r, r_id }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
/// Score entry, used for the seed scores and the job scores.
pub struct ScoreEntry {
    pub id: String,
    pub value: f32,
}

impl ScoreEntry {
    pub fn new(id: String, value: f32) -> Self {
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
        let id = String::decode(buf)?;
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
/// Trust entry, used for the trust graph.
pub struct TrustEntry {
    /// Identifier of the trustor node.
    pub from: String,
    /// Identifier of the trusted node.
    pub to: String,
    /// Trust value.
    pub value: f32,
}

impl TrustEntry {
    pub fn new(from: String, to: String, value: f32) -> Self {
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
        let from = String::decode(buf)?;
        let to = String::decode(buf)?;
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
/// Create commitment transaction.
pub struct CreateCommitment {
    /// Job run assignment transaction hash.
    pub job_run_assignment_tx_hash: TxHash,
    /// LocalTrust tree root hash.
    pub lt_root_hash: Hash,
    /// Compute tree root hash.
    pub compute_root_hash: Hash,
    /// Tx hashes of the scores transactions created by the compute node.
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
/// Create scores transaction.
pub struct CreateScores {
    /// Scores entries
    pub entries: Vec<ScoreEntry>,
}

impl CreateScores {
    pub fn new(entries: Vec<ScoreEntry>) -> Self {
        Self { entries }
    }
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
/// Job run request transaction.
pub struct JobRunRequest {
    /// Domain hash in which the job run request is created.
    pub domain_id: DomainHash,
    /// Block height limit for the job run request.
    pub block_height: u32,
    /// Job id - random id for the job.
    pub job_id: Hash,
}

impl JobRunRequest {
    pub fn new(domain_id: DomainHash, block_height: u32, job_id: Hash) -> Self {
        Self { domain_id, block_height, job_id }
    }
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
/// Job run assignment transaction.
pub struct JobRunAssignment {
    /// Job run request transaction hash.
    pub job_run_request_tx_hash: TxHash,
    /// Address of the compute node assigned for the job.
    pub assigned_compute_node: Address,
    /// Address of the verifier node assigned for the job.
    pub assigned_verifier_node: Address,
}

impl JobRunAssignment {
    /// Creates a new job run assignment transaction.
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
/// Job verification transaction.
pub struct JobVerification {
    /// Job run assignment transaction hash.
    pub job_run_assignment_tx_hash: TxHash,
    /// Job verification result.
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
/// Domain update pending for the verification.
struct PendingDomainUpdate {
    /// Domain hash.
    domain_id: DomainHash,
    /// Commitment transaction hash created by the compute node.
    commitment_tx_hash: TxHash,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
/// Domain update.
struct DomainUpdate {
    /// Domain hash in which the domain update is created.
    domain_id: DomainHash,
    /// Commitment transaction hash created by the compute node.
    commitment_tx_hash: TxHash,
    /// Verification results transaction hashes.
    verification_results_tx_hashes: Vec<TxHash>,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
/// Proposed block.
pub struct ProposedBlock {
    /// Previous block hash.
    previous_block_hash: TxHash,
    /// State root hash.
    state_root: Hash,
    /// Pending domain updates.
    pending_domain_updates: Vec<PendingDomainUpdate>,
    /// Timestamp of the block.
    timestamp: u32,
    /// Block height.
    block_height: u32,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
/// Finalised block
pub struct FinalisedBlock {
    /// Previous block hash.
    previous_block_hash: TxHash,
    /// State root hash.
    state_root: Hash,
    /// Domain updates.
    domain_updates: Vec<DomainUpdate>,
    /// Timestamp of the block.
    timestamp: u32,
    /// Block height.
    block_height: u32,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
/// Trust update transaction.
pub struct TrustUpdate {
    /// Trust namespace.
    pub trust_id: OwnedNamespace,
    /// Trust entries.
    pub entries: Vec<TrustEntry>,
}

impl TrustUpdate {
    pub fn new(trust_id: OwnedNamespace, entries: Vec<TrustEntry>) -> Self {
        Self { trust_id, entries }
    }
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
/// Seed update transaction.
pub struct SeedUpdate {
    /// Seed namespace
    pub seed_id: OwnedNamespace,
    /// Seed entries.
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
