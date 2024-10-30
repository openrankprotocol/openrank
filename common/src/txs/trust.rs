use super::{Address, TxHash};
use crate::{merkle::Hash, topics::DomainHash};
use alloy_rlp::{BufMut, Decodable, Encodable, Error as RlpError, Result as RlpResult};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use core::result::Result as CoreResult;
use getset::Getters;
use hex::FromHex;
use serde::{Deserialize, Serialize};
use std::io::Read;

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

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> CoreResult<Self, Self::Error> {
        Ok(OwnedNamespace(<[u8; 24]>::from_hex(hex)?))
    }
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

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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
pub struct TrustEntry {
    pub from: String,
    pub to: String,
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

#[derive(Getters)]
pub struct AcceptedTrustUpdates {
    #[get = "pub with_prefix"]
    sequence_number: u64,
    #[getset(skip)]
    trust_update_tx_hashes: Vec<TxHash>,
    #[getset(skip)]
    seed_update_tx_hashes: Vec<TxHash>,
}

impl AcceptedTrustUpdates {
    pub fn new(
        sequence_number: u64, trust_update_tx_hashes: Vec<TxHash>,
        seed_update_tx_hashes: Vec<TxHash>,
    ) -> Self {
        Self { sequence_number, trust_update_tx_hashes, seed_update_tx_hashes }
    }

    pub fn get_trust_update_tx_hashes(&self) -> &Vec<TxHash> {
        &self.trust_update_tx_hashes
    }

    pub fn get_seed_update_tx_hashes(&self) -> &Vec<TxHash> {
        &self.seed_update_tx_hashes
    }
}

#[derive(Getters)]
pub struct Assignment {
    #[get = "pub with_prefix"]
    to_sequence: u64,
    #[get = "pub with_prefix"]
    domain_id: DomainHash,
    #[get = "pub with_prefix"]
    trust_builder: Address,
    #[getset(skip)]
    trust_verifier: Vec<Address>,
}

impl Assignment {
    pub fn new(
        to_sequence: u64, domain_id: DomainHash, trust_builder: Address,
        trust_verifier: Vec<Address>,
    ) -> Self {
        Self { to_sequence, domain_id, trust_builder, trust_verifier }
    }

    pub fn get_trust_verifier(&self) -> &Vec<Address> {
        &self.trust_verifier
    }
}

#[derive(Getters)]
pub struct Commitment {
    #[get = "pub with_prefix"]
    trust_assignment_tx_hash: TxHash,
    #[get = "pub with_prefix"]
    root_hash: Hash,
}

impl Commitment {
    pub fn new(trust_assignment_tx_hash: TxHash, root_hash: Hash) -> Self {
        Self { trust_assignment_tx_hash, root_hash }
    }
}

#[derive(Getters)]
pub struct Verification {
    #[get = "pub with_prefix"]
    trust_commitment_tx_hash: TxHash,
    #[get = "pub with_prefix"]
    verification_result: bool,
}

impl Verification {
    pub fn new(trust_commitment_tx_hash: TxHash, verification_result: bool) -> Self {
        Self { trust_commitment_tx_hash, verification_result }
    }
}

#[derive(Getters)]
pub struct Result {
    #[get = "pub with_prefix"]
    trust_commitment_tx_hash: TxHash,
    #[get = "pub with_prefix"]
    trust_verification_tx_hashes: Vec<TxHash>,
    #[get = "pub with_prefix"]
    timestamp: u64,
}

impl Result {
    pub fn new(
        trust_commitment_tx_hash: TxHash, trust_verification_tx_hashes: Vec<TxHash>, timestamp: u64,
    ) -> Self {
        Self { trust_commitment_tx_hash, trust_verification_tx_hashes, timestamp }
    }
}
