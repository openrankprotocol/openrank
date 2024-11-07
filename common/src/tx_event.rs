use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use getset::Getters;
use serde::{Deserialize, Serialize};

/// Proof of tx inclusion in block.
#[derive(Debug, Clone, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize)]
pub struct InclusionProof([u8; 32]);

/// Transaction event which includes proof of inclusion and custom data.
#[derive(Debug, Clone, RlpDecodable, RlpEncodable, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct TxEvent {
    /// Block height of the DA layer, where the tx was included.
    pub block_number: u64,
    /// Proof of inclusion in the DA block.
    pub proof: InclusionProof,
    /// Data of the transaction.
    data: Vec<u8>,
}

impl TxEvent {
    pub fn new(block_number: u64, proof: InclusionProof, data: Vec<u8>) -> Self {
        Self { block_number, proof, data }
    }

    /// Constructs the TxEvent with default data.
    pub fn default_with_data(data: Vec<u8>) -> Self {
        Self { block_number: 0, proof: InclusionProof::default(), data }
    }
}
