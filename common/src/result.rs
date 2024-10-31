use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResultsQuery {
    pub seq_number: u64,
    pub start: u32,
    pub size: u32,
}

impl GetResultsQuery {
    pub fn new(seq_number: u64, start: u32, size: u32) -> Self {
        Self { seq_number, start, size }
    }
}
