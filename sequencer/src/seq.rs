use openrank_common::{
    db::{self, Db},
    tx::{Tx, TxSequence, TxTimestamp},
};
use std::{
    error::Error as StdError,
    fmt::{Formatter, Result as FmtResult},
};
use std::{
    fmt::Display,
    time::{SystemTime, SystemTimeError},
};

#[derive(Debug)]
/// Errors that can arise while using the block builder node.
pub enum Error {
    /// The database error. The error can occur when interacting with the database.
    Db(db::Error),
    /// SystemTime error. The error can occur when computing timestamps.
    SystemTime(SystemTimeError),
}

impl StdError for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::Db(e) => write!(f, "{}", e),
            Self::SystemTime(e) => write!(f, "{}", e),
        }
    }
}

pub struct Sequencer {
    sequence_number: u64,
    db: Db,
}

impl Sequencer {
    pub fn new(db: Db) -> Result<Self, Error> {
        let res = db.get_checkpoint::<TxSequence>();
        let last_seq = match res {
            Ok(tx_seq) => *tx_seq.seq_number(),
            Err(e) => match e {
                db::Error::NotFound => 0,
                db::Error::CfNotFound => 0,
                e => return Err(Error::Db(e)),
            },
        };
        Ok(Self { sequence_number: last_seq, db })
    }

    pub fn sequence_tx(&mut self, mut tx: Tx) -> Result<Tx, Error> {
        assert!(tx.sequence_number().is_none());

        let now = SystemTime::now();
        let timestamp = now.duration_since(SystemTime::UNIX_EPOCH).map_err(Error::SystemTime)?;
        let tx_sequence = TxSequence::new(tx.hash(), self.sequence_number);
        let tx_timestamp = TxTimestamp::new(tx.hash(), timestamp.as_secs());
        self.db.put(tx_sequence).map_err(Error::Db)?;
        self.db.put(tx_timestamp).map_err(Error::Db)?;

        tx.set_sequence_number(self.sequence_number);
        self.sequence_number += 1;

        Ok(tx)
    }
}
