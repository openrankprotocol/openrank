use crate::txs::TxHash;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResultsQuery {
    pub compute_request_tx_hash: TxHash,
    pub start: u32,
    pub size: u32,
}

impl GetResultsQuery {
    pub fn new(compute_request_tx_hash: TxHash, start: u32, size: u32) -> Self {
        Self { compute_request_tx_hash, start, size }
    }
}
