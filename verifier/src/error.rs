use k256::ecdsa::Error as EcdsaError;
use openrank_common::db::DbError;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

use crate::runner::JobRunnerError;

#[derive(Debug)]
/// Errors that can arise while using the verifier node.
pub enum VerifierNodeError {
    /// The decode error. This can arise when decoding a transaction.
    DecodeError(alloy_rlp::Error),
    /// The database error. The database error can occur when interacting with the database.
    DbError(DbError),
    /// The domain not found error. This can arise when the domain is not found in the config.
    DomainNotFound(String),
    /// The p2p error. This can arise when sending or receiving messages over the p2p network.
    P2PError(String),
    /// The compute internal error. This can arise when there is an internal error in the job runner.
    ComputeInternalError(JobRunnerError),
    /// The signature error. This can arise when verifying a transaction signature.
    SignatureError(EcdsaError),
    /// The invalid tx kind error.
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
