use std::fmt;
use std::sync::Arc;

use getset::Getters;
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::ErrorObjectOwned;
use openrank_common::merkle::Hash;
use openrank_common::runners::compute_runner::ComputeRunner;
use openrank_common::topics::Domain;
use tracing::error;

#[derive(Debug, Clone, Copy)]
pub enum ErrorCode {
    DomainNotFound = -32020,
}
impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            ErrorCode::DomainNotFound => "Domain not found",
        };
        write!(f, "{}", message)
    }
}
impl ErrorCode {
    pub fn code(&self) -> i32 {
        *self as i32
    }
}

fn to_error_object<T: ToString>(code: ErrorCode, data: Option<T>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(code.code(), code.to_string(), data.map(|d| d.to_string()))
}

#[rpc(server, namespace = "computer")]
pub trait Rpc {
    #[method(name = "get_lt_state")]
    async fn get_lt_state(
        &self, domain: Domain,
    ) -> Result<Hash, ErrorObjectOwned>;

    #[method(name = "get_seed_state")]
    async fn get_seed_state(
        &self, domain: Domain,
    ) -> Result<Hash, ErrorObjectOwned>;
}


#[derive(Getters)]
#[getset(get = "pub")]
/// The Sequencer JsonRPC server. It contains the sender, the whitelisted users, and the database connection.
pub struct ComputerServer {
    runner: Arc<ComputeRunner>,
}

impl ComputerServer {
    pub fn new(runner: Arc<ComputeRunner>) -> Self {
        Self { runner }
    }
}

#[async_trait]
impl RpcServer for ComputerServer {
    /// Fetch TrustUpdate contents
    async fn get_lt_state(&self, domain: Domain) -> Result<Hash, ErrorObjectOwned> {
        let lt_tree_root = self.runner.base().get_lt_tree_root(&domain).map_err(|e| {
            error!("{}", e);
            to_error_object(ErrorCode::DomainNotFound, Some(e))
        })?;
        Ok(lt_tree_root)
    }

    /// Fetch SeedUpdate contents
    async fn get_seed_state(&self, domain: Domain) -> Result<Hash, ErrorObjectOwned> {
        let st_tree_root = self.runner.base().get_st_tree_root(&domain).map_err(|e| {
            error!("{}", e);
            to_error_object(ErrorCode::DomainNotFound, Some(e))
        })?;
        Ok(st_tree_root)
    }
}
