use rocksdb::{Error as RocksDBError, Options, DB};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{to_vec, Error as SerdeError};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Debug)]
pub enum DbError {
	RocksDB(RocksDBError),
	Serde(SerdeError),
	CfNotFound,
	NotFound,
}

impl StdError for DbError {}

impl Display for DbError {
	fn fmt(&self, f: &mut Formatter) -> FmtResult {
		match self {
			Self::RocksDB(e) => write!(f, "{}", e),
			Self::Serde(e) => write!(f, "{}", e),
			Self::CfNotFound => write!(f, "CfNotFound"),
			Self::NotFound => write!(f, "NotFound"),
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

pub struct Db {
	connection: DB,
}

impl Db {
	pub fn new(path: &str, cfs: &[&str]) -> Result<Self, DbError> {
		assert!(path.ends_with("-storage"));
		let mut opts = Options::default();
		opts.create_if_missing(true);
		opts.create_missing_column_families(true);
		let db = DB::open_cf(&opts, path, cfs).map_err(|e| DbError::RocksDB(e).into())?;
		Ok(Self { connection: db })
	}

	pub fn new_read_only(path: &str, cfs: &[&str]) -> Result<Self, DbError> {
		assert!(path.ends_with("-storage"));
		let mut opts = Options::default();
		opts.create_if_missing(true);
		opts.create_missing_column_families(true);
		let db = DB::open_cf_for_read_only(&opts, path, cfs, false)
			.map_err(|e| DbError::RocksDB(e).into())?;
		Ok(Self { connection: db })
	}

	pub fn put<I: DbItem + Serialize>(&self, item: I) -> Result<(), DbError> {
		let cf =
			self.connection.cf_handle(I::get_cf().as_str()).ok_or(DbError::CfNotFound.into())?;
		let key = item.get_full_key();
		let value = to_vec(&item).map_err(|e| DbError::Serde(e).into())?;
		self.connection.put_cf(&cf, key, value).map_err(|e| DbError::RocksDB(e).into())
	}

	pub fn get<I: DbItem + DeserializeOwned>(&self, key: Vec<u8>) -> Result<I, DbError> {
		let cf =
			self.connection.cf_handle(I::get_cf().as_str()).ok_or(DbError::CfNotFound.into())?;
		let item_res = self.connection.get_cf(&cf, key).map_err(|e| DbError::RocksDB(e).into())?;
		let item = item_res.ok_or(DbError::NotFound.into())?;
		let value = serde_json::from_slice(&item).map_err(|e| DbError::Serde(e).into())?;
		Ok(value)
	}

	pub fn read_from_end<I: DbItem + DeserializeOwned>(
		&self, num_elements: usize, prefix: String,
	) -> Result<Vec<I>, DbError> {
		let cf =
			self.connection.cf_handle(I::get_cf().as_str()).ok_or(DbError::CfNotFound.into())?;
		let iter = self.connection.prefix_iterator_cf(&cf, prefix);
		let mut elements = Vec::new();
		for (_, db_value) in iter.map(Result::unwrap).take(num_elements) {
			let tx =
				serde_json::from_slice(db_value.as_ref()).map_err(|e| DbError::Serde(e).into())?;
			elements.push(tx);
		}
		Ok(elements)
	}
}

#[cfg(test)]
mod test {
	use super::{Db, DbItem};
	use crate::txs::{JobRunAssignment, JobRunRequest, JobVerification, Tx, TxKind};
	use alloy_rlp::encode;

	#[test]
	fn test_put_get() {
		let db = Db::new("test-pg-storage", &[&Tx::get_cf()]).unwrap();
		let tx = Tx::default_with(TxKind::JobRunRequest, encode(JobRunRequest::default()));
		db.put(tx.clone()).unwrap();
		let key = Tx::construct_full_key(TxKind::JobRunRequest, tx.hash());
		let item = db.get::<Tx>(key).unwrap();
		assert_eq!(tx, item);
	}

	#[test]
	fn test_read_from_end() {
		let db = Db::new("test-rfs-storage", &[&Tx::get_cf()]).unwrap();
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
		let items1 = db.read_from_end::<Tx>(1, TxKind::JobRunRequest.into()).unwrap();
		let items2 = db.read_from_end::<Tx>(1, TxKind::JobRunAssignment.into()).unwrap();
		let items3 = db.read_from_end::<Tx>(1, TxKind::JobVerification.into()).unwrap();
		assert_eq!(vec![tx1], items1);
		assert_eq!(vec![tx2], items2);
		assert_eq!(vec![tx3], items3);
	}
}
