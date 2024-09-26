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

impl From<TxKind> for String {
    fn from(val: TxKind) -> Self {
        match val {
            TxKind::TrustUpdate => "trust_update".to_string(),
            TxKind::SeedUpdate => "seed_update".to_string(),
            TxKind::JobRunRequest => "job_run_request".to_string(),
            TxKind::JobRunAssignment => "job_run_assignment".to_string(),
            TxKind::CreateScores => "create_scores".to_string(),
            TxKind::CreateCommitment => "create_commitment".to_string(),
            TxKind::JobVerification => "job_verification".to_string(),
            TxKind::ProposedBlock => "proposed_block".to_string(),
            TxKind::FinalisedBlock => "finalised_block".to_string(),
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
        hasher.update(self.nonce.to_be_bytes());
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
    use crate::{
        merkle::Hash,
        topics::DomainHash,
        txs::{
            Address, CreateCommitment, JobRunAssignment, JobRunRequest, JobVerification,
            TrustEntry, TxHash,
        },
    };

    use super::{ScoreEntry, TrustUpdate, Tx, TxKind};
    use alloy_rlp::{encode, Decodable};
    use k256::ecdsa::SigningKey;

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

    #[test]
    fn test_encode_job_run_request() {
        let job_run_request = JobRunRequest::new(DomainHash::from(1), 2, Hash::default());
        let encoded_bytes = encode(job_run_request);
        assert_eq!(
            encoded_bytes,
            hex::decode(
                "e5c10102e1a00000000000000000000000000000000000000000000000000000000000000000"
            )
            .unwrap()
        );
    }

    #[test]
    fn test_encode_job_run_assignment() {
        let job_run_assignment =
            JobRunAssignment::new(TxHash::default(), Address::default(), Address::default());
        let encoded_bytes = encode(job_run_assignment.clone());
        assert_eq!(encoded_bytes, hex::decode("f84ee1a00000000000000000000000000000000000000000000000000000000000000000d5940000000000000000000000000000000000000000d5940000000000000000000000000000000000000000").unwrap());
    }

    #[test]
    fn test_encode_create_commitment() {
        let create_commitment = CreateCommitment::default_with(
            TxHash::default(),
            Hash::default(),
            Hash::default(),
            vec![TxHash::default(), TxHash::default()],
        );
        let encoded_bytes = encode(create_commitment);
        assert_eq!(encoded_bytes, hex::decode("f8aee1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000f844e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000c0c0").unwrap());
    }

    #[test]
    fn test_encode_job_verification() {
        let job_verification = JobVerification::default();
        let encoded_bytes = encode(job_verification);
        assert_eq!(
            encoded_bytes,
            hex::decode("e3e1a0000000000000000000000000000000000000000000000000000000000000000001")
                .unwrap()
        );
    }

    #[test]
    fn test_sign_tx_job_run_request() {
        let sk_bytes_hex = "9E7645D0CFD9C3A04EB7A9DB59A4EB7D359F2E75C9164A9D6B9A7D54E1B6A36F";
        let sk_bytes = hex::decode(sk_bytes_hex).unwrap();
        let sk = SigningKey::from_slice(&sk_bytes).unwrap();

        let mut tx = super::Tx::default_with(
            TxKind::JobRunRequest,
            encode(super::JobRunRequest::default()),
        );
        let _ = tx.sign(&sk).unwrap();

        let tx_hash = tx.hash();
        assert_eq!(
            hex::encode(tx_hash.0),
            "159232adb52f1e32121d4b111339a6959caccfd86ecf2cc7ab8ef4388d314646"
        );

        let pk = tx.verify().unwrap();
        assert_eq!(
            hex::encode(pk.0),
            "8a0a19589531694250d570040a0c4b74576919b8"
        );

        assert_eq!(tx.nonce, 0);
        assert_eq!(
            hex::encode(tx.from.0),
            "0000000000000000000000000000000000000000"
        );
        assert_eq!(
            hex::encode(tx.to.0),
            "0000000000000000000000000000000000000000"
        );
        assert_eq!(
            hex::encode(tx.body.clone()),
            "e5c18080e1a00000000000000000000000000000000000000000000000000000000000000000"
        );
        assert_eq!(
            hex::encode(tx.signature.s),
            "187e6de032ceea6cc1c879fc7a52ec274c93b2be19b6d4fb11bcb806db2f3f4c"
        );
        assert_eq!(
            hex::encode(tx.signature.r),
            "02ca805e1940a90fc3df7b51f66abc4d55cc3866a751b6110e9f2e3c962182b3"
        );
        assert_eq!(tx.signature.r_id, 1);
    }

    #[test]
    fn test_sign_tx_job_run_assignment() {
        use hex::FromHex;

        let sk_bytes_hex = "45A915E4D060149EB4365960E6A7A45F334393093061116B197E3240065FF2D8";
        let sk_bytes = hex::decode(sk_bytes_hex).unwrap();
        let sk = SigningKey::from_slice(&sk_bytes).unwrap();

        let mut tx = Tx::default_with(
            TxKind::JobRunAssignment,
            encode(JobRunAssignment::new(
                TxHash::from_bytes(
                    Tx::default_with(TxKind::JobRunRequest, encode(JobRunRequest::default()))
                        .hash()
                        .0
                        .to_vec(),
                ),
                Address::from_hex("13978aee95f38490e9769c39b2773ed763d9cd5f").unwrap(),
                Address::from_hex("cd2a3d9f938e13cd947ec05abc7fe734df8dd826").unwrap(),
            )),
        );
        let _ = tx.sign(&sk);

        let tx_hash = tx.hash();
        assert_eq!(
            hex::encode(tx_hash.0),
            "43924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c819"
        );

        let pk = tx.verify().unwrap();
        assert_eq!(
            hex::encode(pk.0),
            "a94f5374fce5edbc8e2a8697c15331677e6ebf0b"
        );

        assert_eq!(tx.nonce, 0);
        assert_eq!(
            hex::encode(tx.from.0),
            "0000000000000000000000000000000000000000"
        );
        assert_eq!(
            hex::encode(tx.to.0),
            "0000000000000000000000000000000000000000"
        );
        assert_eq!(hex::encode(tx.body.clone()), "f84ee1a0159232adb52f1e32121d4b111339a6959caccfd86ecf2cc7ab8ef4388d314646d59413978aee95f38490e9769c39b2773ed763d9cd5fd594cd2a3d9f938e13cd947ec05abc7fe734df8dd826");
        assert_eq!(
            hex::encode(tx.signature.s),
            "1de2b58056c12cceae2229750a3975ec10cb56ac1c592bc91422c82fd540bcf0"
        );
        assert_eq!(
            hex::encode(tx.signature.r),
            "7320fdb17e892a361d979f074cb9f9949764796fb68de9f20b7aff45bafe0b8a"
        );
        assert_eq!(tx.signature.r_id, 1);
    }

    #[test]
    fn test_sign_tx_create_commitment() {
        let sk_bytes_hex = "c87f65ff3f271bf5dc8643484f66b200109caffe4bf98c4cb393dc35740b28c0";
        let sk_bytes = hex::decode(sk_bytes_hex).unwrap();
        let sk = SigningKey::from_slice(&sk_bytes).unwrap();

        let mut tx = Tx::default_with(
            TxKind::CreateCommitment,
            encode(CreateCommitment {
                job_run_assignment_tx_hash: TxHash::from_bytes(
                    hex::decode("43924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c819")
                        .unwrap(),
                ),
                lt_root_hash: Hash::default(),
                compute_root_hash: Hash::default(),
                scores_tx_hashes: vec![],
                new_seed_tx_hashes: vec![],
                new_trust_tx_hashes: vec![],
            }),
        );
        let _ = tx.sign(&sk);

        let tx_hash = tx.hash();
        assert_eq!(
            hex::encode(tx_hash.0),
            "9949143b1cabba1079b3f15b000fcb7c030d0fdbfcfff704be1f8917d88582ef"
        );

        let pk = tx.verify().unwrap();
        assert_eq!(
            hex::encode(pk.0),
            "13978aee95f38490e9769c39b2773ed763d9cd5f"
        );

        assert_eq!(tx.nonce, 0);
        assert_eq!(
            hex::encode(tx.from.0),
            "0000000000000000000000000000000000000000"
        );
        assert_eq!(
            hex::encode(tx.to.0),
            "0000000000000000000000000000000000000000"
        );
        assert_eq!(hex::encode(tx.body.clone()), "f869e1a043924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c819e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000c0c0c0");
        assert_eq!(
            hex::encode(tx.signature.s),
            "2a7f69e1c5cc5f11272fa5a2632f8c47c8039f1e19dcf739ad99adad9130fe15"
        );
        assert_eq!(
            hex::encode(tx.signature.r),
            "dac8c2a3d60d7511b008fdc854b8e8156954ff7670991151ae67c303dbc7e28e"
        );
        assert_eq!(tx.signature.r_id, 1);
    }

    #[test]
    fn test_sign_tx_job_verification() {
        let sk_bytes_hex = "c85ef7d79691fe79573b1a7064c19c1a9819ebdbd1faaab1a8ec92344438aaf4";
        let sk_bytes = hex::decode(sk_bytes_hex).unwrap();
        let sk = SigningKey::from_slice(&sk_bytes).unwrap();

        let mut tx = Tx::default_with(
            TxKind::JobVerification,
            encode(JobVerification {
                job_run_assignment_tx_hash: TxHash::from_bytes(
                    hex::decode("43924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c819")
                        .unwrap(),
                ),
                verification_result: true,
            }),
        );
        let _ = tx.sign(&sk);

        let tx_hash = tx.hash();
        assert_eq!(
            hex::encode(tx_hash.0),
            "042a89a8fa63d2af0dbb5248e72c0094b640285d78ef262931ab1550e6e1a4d0"
        );

        let pk = tx.verify().unwrap();
        assert_eq!(
            hex::encode(pk.0),
            "cd2a3d9f938e13cd947ec05abc7fe734df8dd826"
        );

        assert_eq!(tx.nonce, 0);
        assert_eq!(
            hex::encode(tx.from.0),
            "0000000000000000000000000000000000000000"
        );
        assert_eq!(
            hex::encode(tx.to.0),
            "0000000000000000000000000000000000000000"
        );
        assert_eq!(
            hex::encode(tx.body.clone()),
            "e3e1a043924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c81901"
        );
        assert_eq!(
            hex::encode(tx.signature.s),
            "75f3cab53d46d1eb00ceaee6525bbece17878ca9ed8caf6796b969d78329cc92"
        );
        assert_eq!(
            hex::encode(tx.signature.r),
            "f293b710791ceb69d1317ebc0d8952005fc186a2a363bc74004771f183d1d8d5"
        );
        assert_eq!(tx.signature.r_id, 1);
    }
}
