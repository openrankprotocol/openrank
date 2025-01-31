use openrank_common::{
    db::{self, Db},
    tx::{
        compute::{self, RequestSequence},
        Body, Tx, TxSequence,
    },
};
use std::sync::Arc;
use std::time::{SystemTime, SystemTimeError};

#[derive(thiserror::Error, Debug)]
/// Errors that can arise while using the block builder node.
pub enum Error {
    /// The database error. The error can occur when interacting with the database.
    #[error("DB Error: {0}")]
    Db(db::Error),
    /// SystemTime error. The error can occur when computing timestamps.
    #[error("System time error: {0}")]
    SystemTime(SystemTimeError),
}

pub struct Sequencer {
    tx_sequence_number: u64,
    request_sequence_number: u64,
    db: Arc<Db>,
}

impl Sequencer {
    pub fn new(db: Arc<Db>) -> Result<Self, Error> {
        let res = db.get_checkpoint::<TxSequence>();
        let last_tx_seq_number = match res {
            Ok(tx_seq) => *tx_seq.seq_number(),
            Err(e) => match e {
                db::Error::NotFound => 0,
                db::Error::CfNotFound => 0,
                e => return Err(Error::Db(e)),
            },
        };
        let res = db.get_checkpoint::<compute::RequestSequence>();
        let request_seq_number = match res {
            Ok(tx_seq) => *tx_seq.seq_number(),
            Err(e) => match e {
                db::Error::NotFound => 0,
                db::Error::CfNotFound => 0,
                e => return Err(Error::Db(e)),
            },
        };
        Ok(Self {
            tx_sequence_number: last_tx_seq_number,
            request_sequence_number: request_seq_number,
            db,
        })
    }

    pub fn sequence_tx(&mut self, mut tx: Tx) -> Result<Tx, Error> {
        assert!(tx.sequence_number().is_none());
        let tx_hash = tx.hash();

        if let Body::ComputeRequest(request) = tx.body_mut() {
            assert!(request.seq_number().is_none());
            let request_sequence = RequestSequence::new(tx_hash, self.request_sequence_number);
            self.db.put(request_sequence).map_err(Error::Db)?;
            request.set_seq_number(self.request_sequence_number);
            self.request_sequence_number += 1;
        }

        let now = SystemTime::now();
        let timestamp = now.duration_since(SystemTime::UNIX_EPOCH).map_err(Error::SystemTime)?;
        let tx_sequence = TxSequence::new(tx.hash(), self.tx_sequence_number, timestamp.as_secs());
        self.db.put(tx_sequence).map_err(Error::Db)?;

        tx.set_sequence_number(self.tx_sequence_number);
        self.tx_sequence_number += 1;

        Ok(tx)
    }
}
