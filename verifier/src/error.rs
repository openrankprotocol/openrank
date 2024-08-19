use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

use openrank_common::db::DbError;

use crate::runner::JobRunnerError;

#[derive(Debug)]
pub enum VerifierNodeError {
	SerdeError(alloy_rlp::Error),
	DbError(DbError),
	DomainNotFound(String),
	P2PError(String),
	ComputeInternalError(JobRunnerError),
}

impl StdError for VerifierNodeError {}

impl Display for VerifierNodeError {
	fn fmt(&self, f: &mut Formatter) -> FmtResult {
		match self {
			Self::SerdeError(err) => err.fmt(f),
			Self::DbError(err) => err.fmt(f),
			Self::DomainNotFound(domain) => write!(f, "Domain not found: {}", domain),
			Self::P2PError(err) => write!(f, "p2p error: {}", err),
			Self::ComputeInternalError(err) => write!(f, "internal error: {}", err),
		}
	}
}

impl Into<VerifierNodeError> for JobRunnerError {
	fn into(self) -> VerifierNodeError {
		VerifierNodeError::ComputeInternalError(self)
	}
}
