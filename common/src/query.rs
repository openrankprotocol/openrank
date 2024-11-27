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
    from: tx::TxHash,
    size: Option<usize>,
}

impl GetTrustUpdateQuery {
    pub fn new(from: tx::TxHash, size: Option<usize>) -> Self {
        Self { from, size }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct GetSeedUpdateQuery {
    from: tx::TxHash,
    size: Option<usize>,
}

impl GetSeedUpdateQuery {
    pub fn new(from: tx::TxHash, size: Option<usize>) -> Self {
        Self { from, size }
    }
}
