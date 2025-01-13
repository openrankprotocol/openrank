use std::fmt;
use std::sync::{Arc, Mutex};

use getset::Getters;
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::ErrorObjectOwned;
use openrank_common::merkle::Hash;
use openrank_common::runners::verification_runner::VerificationRunner;
use openrank_common::topics::Domain;
use tracing::error;

#[derive(Debug, Clone, Copy)]
pub enum ErrorCode {
    GetStateFailed = -32020,
    VerificationRunnerLockFailed = -32021,
}
impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            ErrorCode::GetStateFailed => "Get lt/seed state failed",
            ErrorCode::VerificationRunnerLockFailed => "VerificationRunner lock failed",
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

#[rpc(server, namespace = "verifier")]
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
pub struct VerifierServer {
    runner: Arc<Mutex<VerificationRunner>>,
}

impl VerifierServer {
    pub fn new(runner: Arc<Mutex<VerificationRunner>>) -> Self {
        Self { runner }
    }
}

#[async_trait]
impl RpcServer for VerifierServer {
    /// Fetch TrustUpdate contents
    async fn get_lt_state(&self, domain: Domain) -> Result<Hash, ErrorObjectOwned> {
        let verification_runner = self.runner.lock().map_err(|e| to_error_object(ErrorCode::VerificationRunnerLockFailed, Some(e)))?;
        let lt_tree_root = verification_runner.base().get_lt_tree_root(&domain).map_err(|e| {
            error!("{}", e);
            to_error_object(ErrorCode::GetStateFailed, Some(e))
        })?;
        Ok(lt_tree_root)
    }

    /// Fetch SeedUpdate contents
    async fn get_seed_state(&self, domain: Domain) -> Result<Hash, ErrorObjectOwned> {
        let verification_runner = self.runner.lock().map_err(|e| to_error_object(ErrorCode::VerificationRunnerLockFailed, Some(e)))?;
        let st_tree_root = verification_runner.base().get_st_tree_root(&domain).map_err(|e| {
            error!("{}", e);
            to_error_object(ErrorCode::GetStateFailed, Some(e))
        })?;
        Ok(st_tree_root)
    }
}
