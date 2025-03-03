use getset::Getters;
use rocksdb::{self, Direction, IteratorMode, Options, ReadOptions, DB};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{self, to_vec};

mod items;

pub use rocksdb::ErrorKind as RocksDBErrorKind;

pub const CHECKPOINTS_CF: &str = "checkpoints";

#[derive(thiserror::Error, Debug)]
/// Errors that can arise while using database.
pub enum Error {
    /// RocksDB failed.
    #[error("DB Error: {0}")]
    RocksDB(rocksdb::Error),
    /// Error when decoding entries from RocksDB.
    #[error("Serde Error: {0}")]
    Serde(serde_json::Error),
    /// Error when column family is not found.
    #[error("CF Not Found")]
    CfNotFound,
    /// Error when entry is not found.
    #[error("Entry not found")]
    NotFound,
    /// Config parsing error.
    #[error("Config Error: {0}")]
    Config(String),
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

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct Config {
    directory: String,
    secondary: Option<String>,
}

impl Config {
    fn get_secondary(&self) -> Result<&String, Error> {
        self.secondary.as_ref().ok_or(Error::Config("secondary path not set".into()))
    }
}

#[derive(Getters)]
#[getset(get = "pub")]
/// Wrapper for database connection.
pub struct Db {
    connection: DB,
    config: Config,
}

impl Db {
    /// Creates new database connection, given info of local file path and column families.
    pub fn new<I: IntoIterator<Item = N>, N: AsRef<str>>(
        config: &Config, cfs: I,
    ) -> Result<Self, Error> {
        let path = &config.directory;
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        let db = DB::open_cf(&opts, path, cfs).map_err(Error::RocksDB)?;
        Ok(Self { connection: db, config: config.clone() })
    }

    pub fn get_config(&self) -> Config {
        self.config.clone()
    }

    /// Creates new read-only database connection, given info of local file path and column families.
    pub fn new_read_only(config: &Config, cfs: &[&str]) -> Result<Self, Error> {
        let path = &config.directory;
        let db = DB::open_cf_for_read_only(&Options::default(), path, cfs, false)
            .map_err(Error::RocksDB)?;
        Ok(Self { connection: db, config: config.clone() })
    }

    /// Creates new secondary database connection, given info of primary and secondary file paths and column families.
    /// Secondary database is used for read-only queries, and should be explicitly refreshed.
    pub fn new_secondary<I: IntoIterator<Item = N>, N: AsRef<str>>(
        config: &Config, cfs: I,
    ) -> Result<Self, Error> {
        let primary_path = &config.directory;
        let secondary_path = config.get_secondary()?;
        let db = DB::open_cf_as_secondary(&Options::default(), primary_path, secondary_path, cfs)
            .map_err(Error::RocksDB)?;
        Ok(Self { connection: db, config: config.clone() })
    }

    /// Refreshes secondary database connection, by catching up with primary database.
    pub fn refresh(&self) -> Result<(), Error> {
        self.connection.try_catch_up_with_primary().map_err(Error::RocksDB)
    }

    /// Puts checkpoint into database.
    pub fn put_checkpoint<I: DbItem + Serialize>(&self, item: I) -> Result<(), Error> {
        let cf = self.connection.cf_handle(CHECKPOINTS_CF).ok_or(Error::CfNotFound)?;
        let key = I::get_cf();
        let value = to_vec(&item).map_err(Error::Serde)?;
        self.connection.put_cf(&cf, key, value).map_err(Error::RocksDB)
    }

    /// Gets checkpoint from database.
    pub fn get_checkpoint<I: DbItem + DeserializeOwned>(&self) -> Result<I, Error> {
        let cf = self.connection.cf_handle(CHECKPOINTS_CF).ok_or(Error::CfNotFound)?;
        let item_res = self.connection.get_cf(&cf, I::get_cf()).map_err(Error::RocksDB)?;
        let item = item_res.ok_or(Error::NotFound)?;
        let value = serde_json::from_slice(&item).map_err(Error::Serde)?;
        Ok(value)
    }

    /// Puts value into database.
    pub fn put<I: DbItem + Serialize>(&self, item: I) -> Result<(), Error> {
        let cf = self.connection.cf_handle(I::get_cf().as_str()).ok_or(Error::CfNotFound)?;
        let key = item.get_full_key();
        let value = to_vec(&item).map_err(Error::Serde)?;
        // Save the checkpoint
        self.put_checkpoint(item)?;
        // Save item to DB
        self.connection.put_cf(&cf, key, value).map_err(Error::RocksDB)
    }

    /// Gets value from database.
    pub fn get<I: DbItem + DeserializeOwned>(&self, key: Vec<u8>) -> Result<I, Error> {
        let cf = self.connection.cf_handle(I::get_cf().as_str()).ok_or(Error::CfNotFound)?;
        let item_res = self.connection.get_cf(&cf, key).map_err(Error::RocksDB)?;
        let item = item_res.ok_or(Error::NotFound)?;
        let value = serde_json::from_slice(&item).map_err(Error::Serde)?;
        Ok(value)
    }

    /// Gets multiple values from database.
    pub fn get_multi<I: DbItem + DeserializeOwned>(
        &self, keys: Vec<Vec<u8>>,
    ) -> Result<Vec<I>, Error> {
        let cf = self.connection.cf_handle(I::get_cf().as_str()).ok_or(Error::CfNotFound)?;
        let items = self.connection.batched_multi_get_cf(&cf, &keys, true);
        let mut values = Vec::new();
        for item_res in items {
            let item = item_res.map_err(Error::RocksDB)?.ok_or(Error::NotFound)?;
            let value = serde_json::from_slice(&item).map_err(Error::Serde)?;
            values.push(value);
        }
        Ok(values)
    }

    /// Gets values from database from the start, up to `num_elements`, starting from `from`, with `prefix`.
    pub fn get_range_from_start<I: DbItem + DeserializeOwned>(
        &self, prefix: &str, from: Option<Vec<u8>>, num_elements: Option<usize>,
    ) -> Result<Vec<I>, Error> {
        let num_elements = num_elements.unwrap_or(usize::MAX);
        let cf = self.connection.cf_handle(I::get_cf().as_str()).ok_or(Error::CfNotFound)?;

        let mut readopts = ReadOptions::default();
        readopts.set_prefix_same_as_start(true);
        if let Some(from) = from {
            readopts.set_iterate_range(from..);
        }
        let iter = self.connection.iterator_cf_opt(
            &cf,
            readopts,
            IteratorMode::From(prefix.as_ref(), Direction::Forward),
        );

        let mut elements = Vec::new();
        for (_, db_value) in iter.map(Result::unwrap).take(num_elements) {
            let tx = serde_json::from_slice(db_value.as_ref()).map_err(Error::Serde)?;
            elements.push(tx);
        }
        Ok(elements)
    }
}

#[cfg(test)]
mod test {
    use crate::db::{Config, Db, DbItem, CHECKPOINTS_CF};
    use crate::tx::{compute, consts, Body, Tx};

    fn config_for_dir(directory: &str) -> Config {
        Config { directory: directory.to_string(), secondary: None }
    }

    #[test]
    fn test_put_get() {
        let db = Db::new(
            &config_for_dir("test-pg-storage"),
            [Tx::get_cf(), CHECKPOINTS_CF.to_string()],
        )
        .unwrap();
        let tx = Tx::default_with(Body::ComputeRequest(compute::Request::default()));
        db.put(tx.clone()).unwrap();
        let key = Tx::construct_full_key(consts::COMPUTE_REQUEST, tx.hash());
        let item = db.get::<Tx>(key).unwrap();
        assert_eq!(tx, item);
    }

    #[test]
    fn test_get_range_from_start() {
        let db = Db::new(
            &config_for_dir("test-rfs-storage"),
            [Tx::get_cf(), CHECKPOINTS_CF.to_string()],
        )
        .unwrap();
        let tx1 = Tx::default_with(Body::ComputeRequest(compute::Request::default()));
        let tx2 = Tx::default_with(Body::ComputeAssignment(compute::Assignment::default()));
        let tx3 = Tx::default_with(Body::ComputeVerification(compute::Verification::default()));
        db.put(tx1.clone()).unwrap();
        db.put(tx2.clone()).unwrap();
        db.put(tx3.clone()).unwrap();

        // FIX: Test fails if you specify reading more than 1 item for a single prefix
        let items1 = db.get_range_from_start::<Tx>(consts::COMPUTE_REQUEST, None, Some(1)).unwrap();
        let items2 =
            db.get_range_from_start::<Tx>(consts::COMPUTE_ASSIGNMENT, None, Some(1)).unwrap();
        let items3 =
            db.get_range_from_start::<Tx>(consts::COMPUTE_VERIFICATION, None, Some(1)).unwrap();
        assert_eq!(vec![tx1], items1);
        assert_eq!(vec![tx2], items2);
        assert_eq!(vec![tx3], items3);
    }

    #[test]
    fn test_put_get_multi() {
        let db = Db::new(
            &config_for_dir("test-pgm-storage"),
            [Tx::get_cf(), CHECKPOINTS_CF.to_string()],
        )
        .unwrap();
        let tx1 = Tx::default_with(Body::ComputeRequest(compute::Request::default()));
        let tx2 = Tx::default_with(Body::ComputeAssignment(compute::Assignment::default()));
        let tx3 = Tx::default_with(Body::ComputeVerification(compute::Verification::default()));
        db.put(tx1.clone()).unwrap();
        db.put(tx2.clone()).unwrap();
        db.put(tx3.clone()).unwrap();

        let key1 = Tx::construct_full_key(consts::COMPUTE_REQUEST, tx1.hash());
        let key2 = Tx::construct_full_key(consts::COMPUTE_ASSIGNMENT, tx2.hash());
        let key3 = Tx::construct_full_key(consts::COMPUTE_VERIFICATION, tx3.hash());
        let items = db.get_multi::<Tx>(vec![key1, key2, key3]).unwrap();
        assert_eq!(vec![tx1, tx2, tx3], items);
    }
}
