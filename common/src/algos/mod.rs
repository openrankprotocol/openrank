use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

pub mod et;

#[derive(Debug)]
/// Errors that can arise in EigenTrust algorithm.
pub enum Error {
    /// The sum of the trust values is zero.
    ZeroSum,
}

impl StdError for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::ZeroSum => write!(f, "ZeroSum"),
        }
    }
}
