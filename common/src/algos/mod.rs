use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

pub mod et;

#[derive(Debug)]
pub enum AlgoError {
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
