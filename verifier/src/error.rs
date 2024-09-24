use k256::ecdsa::Error as EcdsaError;
use openrank_common::db::DbError;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

use crate::runner::VerificationRunnerError;

#[derive(Debug)]
pub enum VerifierNodeError {
    DecodeError(alloy_rlp::Error),
    DbError(DbError),
    DomainNotFound(String),
    P2PError(String),
    InternalError(VerificationRunnerError),
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
            Self::InternalError(err) => write!(f, "internal error: {}", err),
            Self::SignatureError(err) => err.fmt(f),
            Self::InvalidTxKind => write!(f, "InvalidTxKind"),
        }
    }
}

impl Into<VerifierNodeError> for VerificationRunnerError {
    fn into(self) -> VerifierNodeError {
        VerifierNodeError::InternalError(self)
    }
}
