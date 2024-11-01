use openrank_common::tx::{Tx, TxHash};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct TxWithHash {
    pub tx: Tx,
    pub hash: TxHash,
}
