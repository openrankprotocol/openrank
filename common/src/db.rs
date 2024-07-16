use alloy_rlp::Decodable;
use rocksdb::{Error as RocksDBError, IteratorMode, DB};
use serde::Serialize;
use serde_json::{to_vec, Error as SerdeError};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Debug)]
pub enum DbError {
	RocksDB(RocksDBError),
	Serde(SerdeError),
}

impl StdError for DbError {}

impl Display for DbError {
	// This trait requires `fmt` with this exact signature.
	fn fmt(&self, f: &mut Formatter) -> FmtResult {
		match self {
			Self::RocksDB(e) => write!(f, "{}", e),
			Self::Serde(e) => write!(f, "{}", e),
		}
	}
}

pub trait DbItem {
	fn get_key(&self) -> Vec<u8>;
}

pub struct Db {
	connection: DB,
}

impl Db {
	pub fn new(path: &str) -> Result<Self, DbError> {
		let db = DB::open_default(path).map_err(|e| DbError::RocksDB(e))?;
		Ok(Self { connection: db })
	}

	pub fn put<I: DbItem + Serialize>(&self, item: I) -> Result<(), DbError> {
		let key = item.get_key();
		let value = to_vec(&item).map_err(|e| DbError::Serde(e))?;
		self.connection.put(key, value).map_err(|e| DbError::RocksDB(e))
	}

	pub fn read_from_start<I: DbItem + Decodable>(&self, num_elements: usize) -> Vec<I> {
		let iter = self.connection.iterator(IteratorMode::Start);
		let mut elements = Vec::new();
		for (_, (_, db_value)) in iter.map(Result::unwrap).enumerate().take(num_elements) {
			let tx = I::decode(&mut db_value.as_ref()).unwrap();
			elements.push(tx);
		}
		elements
	}
}
