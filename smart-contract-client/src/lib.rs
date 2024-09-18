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
pub struct JobManagerClient {
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

    pub async fn call_with_openrank_tx(&self, tx: Tx) -> Result<()> {
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

        let _tx_hash = match tx.kind() {
            TxKind::JobRunRequest => {
                contract.sendJobRunRequest(converted_tx).send().await?.watch().await?
            },
            TxKind::JobRunAssignment => {
                contract.submitJobRunAssignment(converted_tx).send().await?.watch().await?
            },
            TxKind::CreateCommitment => {
                contract.submitCreateCommitment(converted_tx).send().await?.watch().await?
            },
            TxKind::JobVerification => {
                contract.submitJobVerification(converted_tx).send().await?.watch().await?
            },
            _ => unreachable!(),
        };

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloy::{network::EthereumWallet, node_bindings::Anvil, signers::local::PrivateKeySigner};
    use alloy_rlp::encode;

    use openrank_common::txs::JobRunRequest;

    use super::*;

    #[tokio::test]
    async fn test_call_with_openrank_tx() -> Result<()> {
        // Spin up a local Anvil node.
        // Ensure `anvil` is available in $PATH.
        let anvil = Anvil::new().try_spawn()?;

        // Set up signer from the first default Anvil account (Alice).
        let signer: PrivateKeySigner = anvil.keys()[0].clone().into();
        let wallet = EthereumWallet::from(signer);

        // Create a provider with the wallet.
        let rpc_url: Url = anvil.endpoint().parse()?;
        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(rpc_url.clone());

        println!("Anvil running at `{}`", anvil.endpoint());

        // Deploy the `JobManager` contract.
        let contract = JobManager::deploy(&provider, vec![], vec![], vec![]).await?;

        println!("Deployed contract at address: {}", contract.address());

        // Create a contract instance.
        let contract_address = format!("{}", contract.address());
        let rpc_url_str = rpc_url.as_str();
        let client = JobManagerClient::new(&contract_address, rpc_url_str);

        // Call the `submitJobRunRequest` function for testing.
        let _ = client
            .call_with_openrank_tx(Tx::default_with(
                TxKind::JobRunRequest,
                encode(JobRunRequest::default()),
            ))
            .await?;

        Ok(())
    }
}
