use k256::ecdsa::Error as EcdsaError;
use openrank_common::db::DbError;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

use crate::runner::JobRunnerError;

#[derive(Debug)]
pub enum VerifierNodeError {
    DecodeError(alloy_rlp::Error),
    DbError(DbError),
    DomainNotFound(String),
    P2PError(String),
    ComputeInternalError(JobRunnerError),
    SignatureError(EcdsaError),
    InvalidTxKind,
}

impl StdError for VerifierNodeError {}

impl Display for VerifierNodeError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::DecodeError(err) => err.fmt(f),
            Self::DbError(err) => err.fmt(f),
            Self::DomainNotFound(domain) => write!(f, "Domain not found: {}", domain),
            Self::P2PError(err) => write!(f, "p2p error: {}", err),
            Self::ComputeInternalError(err) => write!(f, "internal error: {}", err),
            Self::SignatureError(err) => err.fmt(f),
            Self::InvalidTxKind => write!(f, "InvalidTxKind"),
        }
    }
}

impl Into<VerifierNodeError> for JobRunnerError {
    fn into(self) -> VerifierNodeError {
        VerifierNodeError::ComputeInternalError(self)
    }
}
