use std::fmt;
use std::sync::{Arc, Mutex};

use getset::Getters;
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::ErrorObjectOwned;
use openrank_common::misc::{LocalTrustStateResponse, SeedTrustStateResponse};
use openrank_common::runners::compute_runner::ComputeRunner;
use openrank_common::topics::Domain;
use tracing::error;

#[derive(Debug, Clone, Copy)]
pub enum ErrorCode {
    GetStateFailed = -32020,
    ComputeRunnerLockFailed = -32021,
}
impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            ErrorCode::GetStateFailed => "Get lt/seed state failed",
            ErrorCode::ComputeRunnerLockFailed => "ComputeRunner lock failed",
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
        &self, domain: Domain, page_size: Option<usize>, next_token: Option<String>,
    ) -> Result<LocalTrustStateResponse, ErrorObjectOwned>;

    #[method(name = "get_seed_state")]
    async fn get_seed_state(
        &self, domain: Domain, page_size: Option<usize>, next_token: Option<String>,
    ) -> Result<SeedTrustStateResponse, ErrorObjectOwned>;
}

#[derive(Getters)]
#[getset(get = "pub")]
/// The Computer JsonRPC server. It contains the "ComputeRunner".
pub struct ComputerServer {
    runner: Arc<Mutex<ComputeRunner>>,
}

impl ComputerServer {
    pub fn new(runner: Arc<Mutex<ComputeRunner>>) -> Self {
        Self { runner }
    }
}

#[async_trait]
impl RpcServer for ComputerServer {
    /// Fetch TrustUpdate contents
    async fn get_lt_state(
        &self, domain: Domain, page_size: Option<usize>, next_token: Option<String>,
    ) -> Result<LocalTrustStateResponse, ErrorObjectOwned> {
        let compute_runner = self
            .runner
            .lock()
            .map_err(|e| to_error_object(ErrorCode::ComputeRunnerLockFailed, Some(e)))?;
        let lt_state =
            compute_runner.base().get_lt_state(&domain, page_size, next_token).map_err(|e| {
                error!("{}", e);
                to_error_object(ErrorCode::GetStateFailed, Some(e))
            })?;
        Ok(lt_state)
    }

    /// Fetch SeedUpdate contents
    async fn get_seed_state(
        &self, domain: Domain, page_size: Option<usize>, next_token: Option<String>,
    ) -> Result<SeedTrustStateResponse, ErrorObjectOwned> {
        let compute_runner = self
            .runner
            .lock()
            .map_err(|e| to_error_object(ErrorCode::ComputeRunnerLockFailed, Some(e)))?;
        let st_state =
            compute_runner.base().get_st_state(&domain, page_size, next_token).map_err(|e| {
                error!("{}", e);
                to_error_object(ErrorCode::GetStateFailed, Some(e))
            })?;
        Ok(st_state)
    }
}
