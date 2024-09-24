use crate::runner::ComputeRunnerError;
use k256::ecdsa::Error as EcdsaError;
use openrank_common::db::DbError;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Debug)]
pub enum ComputeNodeError {
    DecodeError(alloy_rlp::Error),
    DbError(DbError),
    DomainNotFound(String),
    P2PError(String),
    ComputeInternalError(ComputeRunnerError),
    SignatureError(EcdsaError),
    InvalidTxKind,
}

impl StdError for ComputeNodeError {}

impl Display for ComputeNodeError {
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

impl Into<ComputeNodeError> for ComputeRunnerError {
    fn into(self) -> ComputeNodeError {
        ComputeNodeError::ComputeInternalError(self)
    }
}
