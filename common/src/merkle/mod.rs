use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use serde::{Deserialize, Serialize};
use sha3::Digest;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[cfg(test)]
use rand::Rng;

pub mod fixed;
pub mod incremental;

#[derive(
    Debug, Clone, Default, PartialEq, Eq, RlpDecodable, RlpEncodable, Serialize, Deserialize,
)]
pub struct Hash(#[serde(with = "hex")] pub [u8; 32]);

impl Hash {
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }
}

#[cfg(test)]
impl Hash {
    pub fn random<R: Rng>(rng: &mut R) -> Self {
        Hash(rng.gen::<[u8; 32]>())
    }
}

fn next_index(i: u32) -> u32 {
    if i % 2 == 1 {
        (i - 1) / 2
    } else {
        i / 2
    }
}

/// Converts given bytes to the bits.
pub fn to_bits(num: &[u8]) -> Vec<bool> {
    let len = num.len() * 8;
    let mut bits = Vec::new();
    for i in 0..len {
        let bit = num[i / 8] & (1 << (i % 8)) != 0;
        bits.push(bit);
    }
    bits
}

/// Converts given field element to the bits.
pub fn num_to_bits_vec(num: u32) -> Vec<bool> {
    let bits = to_bits(&num.to_be_bytes());

    bits[..u32::BITS as usize].to_vec()
}

pub fn hash_two<H: Digest>(left: Hash, right: Hash) -> Hash {
    let mut hasher = H::new();
    hasher.update(left.0);
    hasher.update(right.0);
    let hash = hasher.finalize().to_vec();
    let mut bytes: [u8; 32] = [0; 32];
    bytes.copy_from_slice(&hash);
    Hash(bytes)
}

pub fn hash_leaf<H: Digest>(preimage: Vec<u8>) -> Hash {
    let mut hasher = H::new();
    hasher.update(preimage);
    let hash = hasher.finalize().to_vec();
    let mut bytes: [u8; 32] = [0; 32];
    bytes.copy_from_slice(&hash);
    Hash(bytes)
}

#[derive(Debug)]
pub enum MerkleError {
    RootNotFound,
    NodesNotFound,
}

impl StdError for MerkleError {}

impl Display for MerkleError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::RootNotFound => write!(f, "RootNotFound"),
            Self::NodesNotFound => write!(f, "NodesNotFound"),
        }
    }
}
