use crate::db::DbItem;
use alloy_rlp::encode;
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

/// Proof of tx inclusion in block.
#[derive(Debug, Clone, Default, RlpDecodable, RlpEncodable, Serialize, Deserialize)]
pub struct InclusionProof([u8; 32]);

/// Transaction event which includes proof of inclusion and custom data.
#[derive(Debug, Clone, RlpDecodable, RlpEncodable, Serialize, Deserialize)]
pub struct TxEvent {
    /// Block height of the DA layer, where the tx was included.
    block_number: u64,
    /// Proof of inclusion in the DA block.
    proof: InclusionProof,
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

    /// Returns the data of the tx event.
    pub fn data(&self) -> Vec<u8> {
        self.data.clone()
    }
}

impl DbItem for TxEvent {
    fn get_key(&self) -> Vec<u8> {
        let mut hasher = Keccak256::new();
        hasher.update(self.block_number.to_be_bytes());
        hasher.update(encode(&self.proof));
        let result = hasher.finalize();
        result.to_vec()
    }

    fn get_cf() -> String {
        "tx_event".to_string()
    }

    fn get_prefix(&self) -> String {
        "tx_event".to_string()
    }
}

#[cfg(test)]
mod test {
    use crate::tx_event::TxEvent;
    use crate::{
        db::DbItem,
        txs::{compute, Kind, Tx},
    };
    use alloy_rlp::encode;

    #[test]
    fn test_tx_event_db_item() {
        let tx_event = TxEvent::default_with_data(encode(Tx::default_with(
            Kind::ComputeRequest,
            encode(compute::Request::default()),
        )));

        let key = tx_event.get_key();
        assert_eq!(
            hex::encode(key),
            "b486c7ce0b8114c45571048c16ebc834f680ca021612d347a6560aa001bdfe97"
        );
    }
}
