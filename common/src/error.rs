use rocksdb::Error as RocksDBError;
use serde_json::Error as SerdeError;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Debug)]
pub enum CommonError {
	Algo(AlgoError),
	Db(DbError),
	Merkle(MerkleError),
}

impl StdError for CommonError {}

impl Display for CommonError {
	fn fmt(&self, f: &mut Formatter) -> FmtResult {
		match self {
			Self::Algo(e) => write!(f, "{}", e),
			Self::Db(e) => write!(f, "{}", e),
			Self::Merkle(e) => write!(f, "{}", e),
		}
	}
}

#[derive(Debug)]
pub enum AlgoError {
	ZeroSum,
}

impl Display for AlgoError {
	fn fmt(&self, f: &mut Formatter) -> FmtResult {
		match self {
			Self::ZeroSum => write!(f, "ZeroSum"),
		}
	}
}

impl Into<CommonError> for AlgoError {
	fn into(self) -> CommonError {
		CommonError::Algo(self)
	}
}

#[derive(Debug)]
pub enum DbError {
	RocksDB(RocksDBError),
	Serde(SerdeError),
	CfNotFound,
	NotFound,
}

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

impl Into<CommonError> for DbError {
	fn into(self) -> CommonError {
		CommonError::Db(self)
	}
}

#[derive(Debug)]
pub enum MerkleError {
	RootNotFound,
}

impl Display for MerkleError {
	fn fmt(&self, f: &mut Formatter) -> FmtResult {
		match self {
			Self::RootNotFound => write!(f, "RootNotFound"),
		}
	}
}

impl Into<CommonError> for MerkleError {
	fn into(self) -> CommonError {
		CommonError::Merkle(self)
	}
}
