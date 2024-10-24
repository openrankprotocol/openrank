use log::{debug, error, info};
use openrank_common::{
    topics::{Domain, Topic},
    tx_event::TxEvent,
    txs::{
        Address,
        Kind,
        // CreateCommitment,
        // CreateScores,
        // JobRunAssignment,
        // JobRunRequest,
        // JobVerification,
        Tx,
        TxHash,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct TxWithHash {
    pub tx: Tx,
    pub hash: TxHash,
}
