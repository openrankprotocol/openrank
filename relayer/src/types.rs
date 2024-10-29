use openrank_common::txs::{Tx, TxHash};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct TxWithHash {
    pub tx: Tx,
    pub hash: TxHash,
}
