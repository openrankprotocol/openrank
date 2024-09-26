use rocksdb::{Error as RocksDBError, Options, DB};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{to_vec, Error as SerdeError};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::usize;

#[derive(Debug)]
/// Errors that can arise while using database.
pub enum DbError {
    /// RocksDB failed.
    RocksDB(RocksDBError),
    /// Error when decoding entries from RocksDB.
    Serde(SerdeError),
    /// Error when column family is not found.
    CfNotFound,
    /// Error when entry is not found.
    NotFound,
    Config(String),
}

impl StdError for DbError {}

impl Display for DbError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::RocksDB(e) => write!(f, "{}", e),
            Self::Serde(e) => write!(f, "{}", e),
            Self::CfNotFound => write!(f, "CfNotFound"),
            Self::NotFound => write!(f, "NotFound"),
            Self::Config(msg) => write!(f, "Config({:?})", msg),
        }
    }
}

pub trait DbItem {
    fn get_key(&self) -> Vec<u8>;
    fn get_prefix(&self) -> String;
    fn get_cf() -> String;
    fn get_full_key(&self) -> Vec<u8> {
        let suffix = self.get_key();
        let mut key = self.get_prefix().as_bytes().to_vec();
        key.extend(suffix);
        key
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    directory: String,
    secondary: Option<String>,
}

impl Config {
    fn get_secondary(&self) -> Result<&String, DbError> {
        self.secondary.as_ref().ok_or(DbError::Config("secondary path not set".into()))
    }
}

/// Wrapper for database connection.
pub struct Db {
    connection: DB,
}

impl Db {
    /// Creates new database connection, given info of local file path and column families.
    pub fn new(config: &Config, cfs: &[&str]) -> Result<Self, DbError> {
        let path = &config.directory;
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        let db = DB::open_cf(&opts, path, cfs).map_err(DbError::RocksDB)?;
        Ok(Self { connection: db })
    }

    /// Creates new read-only database connection, given info of local file path and column families.
    pub fn new_read_only(config: &Config, cfs: &[&str]) -> Result<Self, DbError> {
        let path = &config.directory;
        let db = DB::open_cf_for_read_only(&Options::default(), path, cfs, false)
            .map_err(DbError::RocksDB)?;
        Ok(Self { connection: db })
    }

    /// Creates new secondary database connection, given info of primary and secondary file paths and column families.
    /// Secondary database is used for read-only queries, and should be explicitly refreshed.
    pub fn new_secondary(config: &Config, cfs: &[&str]) -> Result<Self, DbError> {
        let primary_path = &config.directory;
        let secondary_path = config.get_secondary()?;
        let db = DB::open_cf_as_secondary(&Options::default(), primary_path, secondary_path, cfs)
            .map_err(DbError::RocksDB)?;
        Ok(Self { connection: db })
    }

    /// Refreshes secondary database connection, by catching up with primary database.
    pub fn refresh(&self) -> Result<(), DbError> {
        self.connection.try_catch_up_with_primary().map_err(DbError::RocksDB)
    }

    /// Puts value into database.
    pub fn put<I: DbItem + Serialize>(&self, item: I) -> Result<(), DbError> {
        let cf = self.connection.cf_handle(I::get_cf().as_str()).ok_or(DbError::CfNotFound)?;
        let key = item.get_full_key();
        let value = to_vec(&item).map_err(DbError::Serde)?;
        self.connection.put_cf(&cf, key, value).map_err(DbError::RocksDB)
    }

    /// Gets value from database.
    pub fn get<I: DbItem + DeserializeOwned>(&self, key: Vec<u8>) -> Result<I, DbError> {
        let cf = self.connection.cf_handle(I::get_cf().as_str()).ok_or(DbError::CfNotFound)?;
        let item_res = self.connection.get_cf(&cf, key).map_err(DbError::RocksDB)?;
        let item = item_res.ok_or(DbError::NotFound)?;
        let value = serde_json::from_slice(&item).map_err(DbError::Serde)?;
        Ok(value)
    }

    /// Gets values from database from the end, up to `num_elements`, starting from `prefix`.
    pub fn read_from_end<I: DbItem + DeserializeOwned>(
        &self, prefix: String, num_elements: Option<usize>,
    ) -> Result<Vec<I>, DbError> {
        let num_elements = num_elements.unwrap_or(usize::MAX);
        let cf = self.connection.cf_handle(I::get_cf().as_str()).ok_or(DbError::CfNotFound)?;
        let iter = self.connection.prefix_iterator_cf(&cf, prefix);
        let mut elements = Vec::new();
        for (_, db_value) in iter.map(Result::unwrap).take(num_elements) {
            let tx = serde_json::from_slice(db_value.as_ref()).map_err(DbError::Serde)?;
            elements.push(tx);
        }
        Ok(elements)
    }
}

#[cfg(test)]
mod test {
    use super::{Config, Db, DbItem};
    use crate::txs::{JobRunAssignment, JobRunRequest, JobVerification, Tx, TxKind};
    use alloy_rlp::encode;

    fn config_for_dir(directory: &str) -> Config {
        Config { directory: directory.to_string(), secondary: None }
    }

    #[test]
    fn test_put_get() {
        let db = Db::new(&config_for_dir("test-pg-storage"), &[&Tx::get_cf()]).unwrap();
        let tx = Tx::default_with(TxKind::JobRunRequest, encode(JobRunRequest::default()));
        db.put(tx.clone()).unwrap();
        let key = Tx::construct_full_key(TxKind::JobRunRequest, tx.hash());
        let item = db.get::<Tx>(key).unwrap();
        assert_eq!(tx, item);
    }

    #[test]
    fn test_read_from_end() {
        let db = Db::new(&config_for_dir("test-rfs-storage"), &[&Tx::get_cf()]).unwrap();
        let tx1 = Tx::default_with(TxKind::JobRunRequest, encode(JobRunRequest::default()));
        let tx2 = Tx::default_with(
            TxKind::JobRunAssignment,
            encode(JobRunAssignment::default()),
        );
        let tx3 = Tx::default_with(TxKind::JobVerification, encode(JobVerification::default()));
        db.put(tx1.clone()).unwrap();
        db.put(tx2.clone()).unwrap();
        db.put(tx3.clone()).unwrap();

        // FIX: Test fails if you specify reading more than 1 item for a single prefix
        let items1 = db.read_from_end::<Tx>(TxKind::JobRunRequest.into(), Some(1)).unwrap();
        let items2 = db.read_from_end::<Tx>(TxKind::JobRunAssignment.into(), Some(1)).unwrap();
        let items3 = db.read_from_end::<Tx>(TxKind::JobVerification.into(), Some(1)).unwrap();
        assert_eq!(vec![tx1], items1);
        assert_eq!(vec![tx2], items2);
        assert_eq!(vec![tx3], items3);
    }
}
