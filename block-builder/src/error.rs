use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

use openrank_common::db::DbError;

#[derive(Debug)]
pub enum BlockBuilderNodeError {
	SerdeError(alloy_rlp::Error),
	DbError(DbError),
	P2PError(String),
}

impl StdError for BlockBuilderNodeError {}

impl Display for BlockBuilderNodeError {
	fn fmt(&self, f: &mut Formatter) -> FmtResult {
		match self {
			Self::SerdeError(err) => err.fmt(f),
			Self::DbError(err) => err.fmt(f),
			Self::P2PError(err) => write!(f, "p2p error: {}", err),
		}
	}
}
