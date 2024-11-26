use crate::tx;
use getset::Getters;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct GetResultsQuery {
    request_tx_hash: tx::TxHash,
    start: u32,
    size: u32,
}

impl GetResultsQuery {
    pub fn new(request_tx_hash: tx::TxHash, start: u32, size: u32) -> Self {
        Self { request_tx_hash, start, size }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct GetTrustUpdateQuery {
    tu_tx_hash: tx::TxHash,
}

impl GetTrustUpdateQuery {
    pub fn new(tu_tx_hash: tx::TxHash) -> Self {
        Self { tu_tx_hash }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct GetSeedUpdateQuery {
    su_tx_hash: tx::TxHash,
}

impl GetSeedUpdateQuery {
    pub fn new(su_tx_hash: tx::TxHash) -> Self {
        Self { su_tx_hash }
    }
}
