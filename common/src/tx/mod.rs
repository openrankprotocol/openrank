use crate::db::DbItem;
use crate::merkle::hash_leaf;
use alloy_rlp::{encode, BufMut, Decodable, Encodable, Error as RlpError, Result as RlpResult};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use k256::ecdsa::signature::hazmat::PrehashVerifier;
use k256::ecdsa::{
    Error as EcdsaError, RecoveryId, Signature as EcdsaSignature, SigningKey, VerifyingKey,
};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::io::Read;

pub mod block;
pub mod compute;
pub mod trust;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Kind {
    TrustUpdate,
    SeedUpdate,
    ComputeRequest,
    ComputeAssignment,
    ComputeScores,
    ComputeCommitment,
    ComputeVerification,
    ProposedBlock,
    FinalisedBlock,
}

impl Encodable for Kind {
    fn encode(&self, out: &mut dyn BufMut) {
        out.put_u8(*self as u8);
    }
}

impl Decodable for Kind {
    fn decode(buf: &mut &[u8]) -> RlpResult<Self> {
        let mut bytes = [0; 1];
        let size = buf.read(&mut bytes).map_err(|_| RlpError::Custom("Failed to read bytes"))?;
        if size != 1 {
            return RlpResult::Err(RlpError::UnexpectedLength);
        }

        Ok(Kind::from_byte(bytes[0]))
    }
}

impl Kind {
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::TrustUpdate,
            1 => Self::SeedUpdate,
            2 => Self::ComputeRequest,
            3 => Self::ComputeAssignment,
            4 => Self::ComputeScores,
            5 => Self::ComputeCommitment,
            6 => Self::ComputeVerification,
            7 => Self::ProposedBlock,
            8 => Self::FinalisedBlock,
            _ => panic!("Invalid message type"),
        }
    }
}

impl From<Kind> for String {
    fn from(val: Kind) -> Self {
        match val {
            Kind::TrustUpdate => "trust_update".to_string(),
            Kind::SeedUpdate => "seed_update".to_string(),
            Kind::ComputeRequest => "compute_request".to_string(),
            Kind::ComputeAssignment => "compute_assignment".to_string(),
            Kind::ComputeScores => "compute_scores".to_string(),
            Kind::ComputeCommitment => "compute_commitment".to_string(),
            Kind::ComputeVerification => "compute_verification".to_string(),
            Kind::ProposedBlock => "proposed_block".to_string(),
            Kind::FinalisedBlock => "finalised_block".to_string(),
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
    kind: Kind,
    body: Vec<u8>,
    signature: Signature,
    sequence_number: Option<u64>,
}

impl Tx {
    pub fn default_with(kind: Kind, body: Vec<u8>) -> Self {
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

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn body(&self) -> Vec<u8> {
        self.body.clone()
    }

    pub fn signature(&self) -> Signature {
        self.signature.clone()
    }

    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    pub fn from(&self) -> Address {
        self.from.clone()
    }

    pub fn to(&self) -> Address {
        self.to.clone()
    }

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

    pub fn construct_full_key(kind: Kind, tx_hash: TxHash) -> Vec<u8> {
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

        if Address::from_slice(&address_bytes) != address {
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
        let address = Address::from_slice(&address_bytes);

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

pub type Address = alloy_primitives::Address;

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

    pub fn r_id(&self) -> u8 {
        self.r_id
    }
}

#[cfg(test)]
mod test {
    use crate::tx::{
        trust::{ScoreEntry, TrustEntry, TrustUpdate},
        Kind, Tx,
    };
    use alloy_rlp::{encode, Decodable};

    #[test]
    fn test_decode_tx_kind() {
        let res = Kind::decode(&mut [0].as_slice()).unwrap();
        assert_eq!(res, Kind::TrustUpdate);
        let res = Kind::decode(&mut [3].as_slice()).unwrap();
        assert_eq!(res, Kind::ComputeAssignment);
        let res = Kind::decode(&mut [8].as_slice()).unwrap();
        assert_eq!(res, Kind::FinalisedBlock);
    }

    #[test]
    fn test_tx_to_hash() {
        let tx = Tx::default_with(Kind::TrustUpdate, encode(TrustUpdate::default()));
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