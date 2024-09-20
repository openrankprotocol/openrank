use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

pub mod et;

#[derive(Debug)]
/// Error type for EigenTrust algorithm
pub enum AlgoError {
    /// Error when the sum of the trust values is zero
    ZeroSum,
}

impl StdError for AlgoError {}

impl Display for AlgoError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::ZeroSum => write!(f, "ZeroSum"),
        }
    }
}
