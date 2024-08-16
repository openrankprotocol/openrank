use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

use openrank_common::algos::AlgoError;
use openrank_common::db::DbError;
use openrank_common::merkle::MerkleError;

#[derive(Debug)]
pub enum ComputeNodeError {
	ComputeError(MerkleError),
	ComputeAlgoError(AlgoError),
	SerdeError(alloy_rlp::Error),
	DbError(DbError),
	DomainNotFound(String),
	P2PError(String),
}

impl StdError for ComputeNodeError {}

impl Display for ComputeNodeError {
	fn fmt(&self, f: &mut Formatter) -> FmtResult {
		match self {
			Self::ComputeError(err) => err.fmt(f),
			Self::ComputeAlgoError(err) => err.fmt(f),
			Self::SerdeError(err) => err.fmt(f),
			Self::DbError(err) => err.fmt(f),
			Self::DomainNotFound(domain) => write!(f, "Domain not found: {}", domain),
			Self::P2PError(err) => write!(f, "p2p error: {}", err),
		}
	}
}
