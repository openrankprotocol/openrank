use openrank_common::{
    db::{self, Db},
    tx::{
        compute::{self, RequestSequence},
        Body, Tx, TxSequence, TxTimestamp,
    },
};
use std::{
    error::Error as StdError,
    fmt::{Formatter, Result as FmtResult},
    sync::Arc,
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

        match tx.body_mut() {
            Body::ComputeRequest(request) => {
                assert!(request.seq_number().is_none());
                let request_sequence = RequestSequence::new(tx_hash, self.request_sequence_number);
                self.db.put(request_sequence).map_err(Error::Db)?;
                request.set_seq_number(self.request_sequence_number);
                self.request_sequence_number += 1;
            },
            _ => {},
        }

        let now = SystemTime::now();
        let timestamp = now.duration_since(SystemTime::UNIX_EPOCH).map_err(Error::SystemTime)?;
        let tx_sequence = TxSequence::new(tx.hash(), self.tx_sequence_number);
        let tx_timestamp = TxTimestamp::new(tx.hash(), timestamp.as_secs());
        self.db.put(tx_sequence).map_err(Error::Db)?;
        self.db.put(tx_timestamp).map_err(Error::Db)?;

        tx.set_sequence_number(self.tx_sequence_number);
        self.tx_sequence_number += 1;

        Ok(tx)
    }
}
