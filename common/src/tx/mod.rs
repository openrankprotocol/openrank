use crate::db::DbItem;
use crate::merkle::hash_leaf;
use alloy_rlp::{encode, BufMut, Decodable, Encodable, Error as RlpError, Result as RlpResult};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use getset::Getters;
use block::{FinalisedBlock, ProposedBlock};
use k256::ecdsa::signature::hazmat::PrehashVerifier;
use k256::ecdsa::{
    Error as EcdsaError, RecoveryId, Signature as EcdsaSignature, SigningKey, VerifyingKey,
};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::io::Read;
use trust::{SeedUpdate, TrustUpdate};

pub mod block;
pub mod compute;
pub mod trust;

pub mod consts {
    pub const TRUST_UPDATE: &str = "trust_update";
    pub const SEED_UPDATE: &str = "seed_update";
    pub const COMPUTE_REQUEST: &str = "compute_request";
    pub const COMPUTE_ASSIGNMENT: &str = "compute_assignment";
    pub const COMPUTE_SCORES: &str = "compute_scores";
    pub const COMPUTE_COMMITMENT: &str = "compute_commitment";
    pub const COMPUTE_VERIFICATION: &str = "compute_verification";
    pub const COMPUTE_RESULT: &str = "compute_result";
    pub const PROPOSED_BLOCK: &str = "proposed_block";
    pub const FINALISED_BLOCK: &str = "finalised_block";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Body {
    TrustUpdate(TrustUpdate),
    SeedUpdate(SeedUpdate),
    ComputeRequest(compute::Request),
    ComputeAssignment(compute::Assignment),
    ComputeScores(compute::Scores),
    ComputeCommitment(compute::Commitment),
    ComputeVerification(compute::Verification),
    ProposedBlock(ProposedBlock),
    FinalisedBlock(FinalisedBlock),
}

impl Encodable for Body {
    fn encode(&self, out: &mut dyn BufMut) {
        let (prefix, bytes) = match self {
            Body::TrustUpdate(trust_update) => (0, encode(trust_update)),
            Body::SeedUpdate(seed_update) => (1, encode(seed_update)),
            Body::ComputeRequest(compute_request) => (2, encode(compute_request)),
            Body::ComputeAssignment(compute_assignment) => (3, encode(compute_assignment)),
            Body::ComputeScores(compute_scores) => (4, encode(compute_scores)),
            Body::ComputeCommitment(compute_commitment) => (5, encode(compute_commitment)),
            Body::ComputeVerification(compute_verification) => (6, encode(compute_verification)),
            Body::ProposedBlock(proposed_block) => (7, encode(proposed_block)),
            Body::FinalisedBlock(finalised_block) => (8, encode(finalised_block)),
        };
        out.put_u8(prefix);
        out.put_slice(&bytes);
    }
}

impl Decodable for Body {
    fn decode(buf: &mut &[u8]) -> RlpResult<Self> {
        let mut bytes = [0; 1];
        let size = buf.read(&mut bytes).map_err(|_| RlpError::Custom("Failed to read bytes"))?;
        if size != 1 {
            return RlpResult::Err(RlpError::UnexpectedLength);
        }

        match bytes[0] {
            0 => Ok(Body::TrustUpdate(TrustUpdate::decode(buf)?)),
            1 => Ok(Body::SeedUpdate(SeedUpdate::decode(buf)?)),
            2 => Ok(Body::ComputeRequest(compute::Request::decode(buf)?)),
            3 => Ok(Body::ComputeAssignment(compute::Assignment::decode(buf)?)),
            4 => Ok(Body::ComputeScores(compute::Scores::decode(buf)?)),
            5 => Ok(Body::ComputeCommitment(compute::Commitment::decode(buf)?)),
            6 => Ok(Body::ComputeVerification(compute::Verification::decode(
                buf,
            )?)),
            7 => Ok(Body::ProposedBlock(ProposedBlock::decode(buf)?)),
            8 => Ok(Body::FinalisedBlock(FinalisedBlock::decode(buf)?)),
            _ => Err(RlpError::Custom("unexpected prefix")),
        }
    }
}

impl Body {
    pub fn prefix(&self) -> &str {
        match self {
            Body::TrustUpdate(_) => consts::TRUST_UPDATE,
            Body::SeedUpdate(_) => consts::SEED_UPDATE,
            Body::ComputeRequest(_) => consts::COMPUTE_REQUEST,
            Body::ComputeAssignment(_) => consts::COMPUTE_ASSIGNMENT,
            Body::ComputeScores(_) => consts::COMPUTE_SCORES,
            Body::ComputeCommitment(_) => consts::COMPUTE_COMMITMENT,
            Body::ComputeVerification(_) => consts::COMPUTE_VERIFICATION,
            Body::ProposedBlock(_) => consts::PROPOSED_BLOCK,
            Body::FinalisedBlock(_) => consts::FINALISED_BLOCK,
        }
    }
}

#[derive(
    Debug, Clone, PartialEq, RlpEncodable, RlpDecodable, Serialize, Deserialize, Getters,
)]
#[rlp(trailing)]
pub struct Tx {
    #[getset(get = "pub")]
    nonce: u64,
    #[getset(get = "pub")]
    from: Address,
    #[getset(get = "pub")]
    // Use 0x0 for transactions intended to be processed by the network
    to: Address,
    #[getset(skip)]
    body: Body,
    #[getset(get = "pub")]
    signature: Signature,
    #[getset(skip)]
    sequence_number: Option<u64>,
}

impl Tx {
    pub fn default_with(body: Body) -> Self {
        Self {
            nonce: 0,
            from: Address::default(),
            to: Address::default(),
            body,
            signature: Signature::default(),
            sequence_number: None,
        }
    }

    pub fn hash(&self) -> TxHash {
        let mut hasher = Keccak256::new();
        hasher.update(self.nonce.to_be_bytes());
        hasher.update(encode(self.from));
        hasher.update(encode(self.to));
        hasher.update(encode(self.body.clone()));
        let result = hasher.finalize();
        let bytes = result.to_vec();

        let mut tx_bytes = [0; 32];
        tx_bytes.copy_from_slice(&bytes);
        TxHash(tx_bytes)
    }

    pub fn construct_full_key(prefix: &str, tx_hash: TxHash) -> Vec<u8> {
        let mut prefix_bytes = prefix.as_bytes().to_vec();
        prefix_bytes.extend(tx_hash.0);
        prefix_bytes
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

    pub fn body(&self) -> Body {
        self.body.clone()
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
        self.body.prefix().to_string()
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
    Debug,
    Clone,
    PartialEq,
    Eq,
    Default,
    RlpDecodable,
    RlpEncodable,
    Serialize,
    Deserialize,
    Getters,
)]
pub struct Signature {
    #[getset(skip)]
    pub s: [u8; 32],
    #[getset(skip)]
    pub r: [u8; 32],
    #[getset(get = "pub")]
    r_id: u8,
}

impl Signature {
    pub fn new(s: [u8; 32], r: [u8; 32], r_id: u8) -> Self {
        Self { s, r, r_id }
    }
}

#[cfg(test)]
mod test {
    use crate::tx::{
        trust::{ScoreEntry, TrustEntry, TrustUpdate},
        Body, Tx,
    };
    use alloy_rlp::{encode, Decodable};

    #[test]
    fn test_tx_to_hash() {
        let tx = Tx::default_with(Body::TrustUpdate(TrustUpdate::default()));
        let tx_hash = tx.hash();
        assert_eq!(
            hex::encode(tx_hash.0),
            "1ab973d26371451a87e6ef9fe5543114adddfeef3921353c6fb363c093d3315a"
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
