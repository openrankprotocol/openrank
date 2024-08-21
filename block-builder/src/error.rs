use k256::ecdsa::Error as EcdsaError;
use openrank_common::db::DbError;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Debug)]
pub enum BlockBuilderNodeError {
    DecodeError(alloy_rlp::Error),
    DbError(DbError),
    P2PError(String),
    SignatureError(EcdsaError),
    InvalidTxKind,
}

impl StdError for BlockBuilderNodeError {}

impl Display for BlockBuilderNodeError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::DecodeError(err) => err.fmt(f),
            Self::DbError(err) => err.fmt(f),
            Self::P2PError(err) => write!(f, "p2p error: {}", err),
            Self::SignatureError(err) => err.fmt(f),
            Self::InvalidTxKind => write!(f, "InvalidTxKind"),
        }
    }
}
