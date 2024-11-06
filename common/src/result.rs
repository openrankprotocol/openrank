use crate::tx;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResultsQuery {
    pub request_tx_hash: tx::TxHash,
    pub start: u32,
    pub size: u32,
}

impl GetResultsQuery {
    pub fn new(request_tx_hash: tx::TxHash, start: u32, size: u32) -> Self {
        Self { request_tx_hash, start, size }
    }
}
