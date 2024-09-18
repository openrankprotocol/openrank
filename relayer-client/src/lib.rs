//! Example of generating code from ABI file using the `sol!` macro to interact with the contract.

use std::str::FromStr;

use alloy::{primitives::Address, providers::ProviderBuilder, sol, transports::http::reqwest::Url};
use eyre::Result;

use openrank_common::txs::{Tx, TxKind};
use JobManager::{OpenrankTx, Signature};

// Codegen from ABI file to interact with the contract.
sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    JobManager,
    "./abi/JobManager.json"
);

#[derive(Debug)]
struct JobManagerClient {
    contract_address: Address,
    rpc_url: Url,
}

impl JobManagerClient {
    pub fn new(contract_address: &str, rpc_url: &str) -> Self {
        Self {
            contract_address: Address::from_str(contract_address).unwrap(),
            rpc_url: Url::parse(rpc_url).unwrap(),
        }
    }

    pub async fn call_function(&self, tx: Tx) -> Result<()> {
        let provider = ProviderBuilder::new().on_http(self.rpc_url.clone());
        let contract = JobManager::new(self.contract_address, provider);

        let converted_tx = OpenrankTx {
            nonce: tx.nonce(),
            from: tx.from().0.into(),
            to: tx.to().0.into(),
            kind: tx.kind() as u8,
            body: tx.body().into(),
            signature: Signature {
                s: tx.signature().s.into(),
                r: tx.signature().r.into(),
                r_id: tx.signature().r_id(),
            },
            sequence_number: 0,
        };
        match tx.kind() {
            TxKind::JobRunRequest => {
                let _ = contract.sendJobRunRequest(converted_tx).send().await?;
            },
            TxKind::JobRunAssignment => {
                let _ = contract.submitJobRunAssignment(converted_tx).send().await?;
            },
            TxKind::CreateCommitment => {
                let _ = contract.submitCreateCommitment(converted_tx).send().await?;
            },
            TxKind::JobVerification => {
                let _ = contract.submitJobVerification(converted_tx).send().await?;
            },
            _ => {},
        }
        Ok(())
    }
}
