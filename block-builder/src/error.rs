use k256::ecdsa::Error as EcdsaError;
use openrank_common::db::DbError;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Debug)]
/// Errors that can arise while using the block builder node.
pub enum BlockBuilderNodeError {
    /// The decode error. This can arise when decoding a transaction.
    DecodeError(alloy_rlp::Error),
    /// The database error. The error can occur when interacting with the database.
    DbError(DbError),
    /// The p2p error. This can arise when sending or receiving messages over the p2p network.
    P2PError(String),
    /// The signature error. This can arise when verifying a transaction signature.
    SignatureError(EcdsaError),
    /// The invalid tx kind error. This can arise when the transaction kind is not valid.
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
