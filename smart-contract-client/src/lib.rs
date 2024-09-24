use serde::{Deserialize, Serialize};
use std::{error::Error, str::FromStr};

use alloy::{
    hex,
    network::EthereumWallet,
    primitives::Address,
    providers::ProviderBuilder,
    signers::{k256::ecdsa::SigningKey, local::LocalSigner},
    sol,
    transports::http::reqwest::Url,
};
use dotenv::dotenv;
use eyre::Result;

use openrank_common::{
    db::{Db, DbItem},
    txs::{Tx, TxKind},
};
use JobManager::{OpenrankTx, Signature};

// Codegen from ABI file to interact with the contract.
sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    JobManager,
    "./abi/JobManager.json"
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub contract_address: String,
    pub rpc_url: String,
}

pub struct JobManagerClient {
    contract_address: Address,
    rpc_url: Url,
    signer: LocalSigner<SigningKey>,
    db: Db,
}

impl JobManagerClient {
    pub fn init() -> Result<Self, Box<dyn Error>> {
        dotenv().ok();
        let secret_key_hex = std::env::var("SMC_SECRET_KEY")?;
        let secret_key_bytes = hex::decode(secret_key_hex)?;
        let secret_key = SigningKey::from_slice(secret_key_bytes.as_slice())?;

        let config: Config = toml::from_str(include_str!("../config.toml"))?;

        let contract_address = Address::from_str(&config.contract_address)?;
        let rpc_url = Url::parse(&config.rpc_url)?;
        let db = Db::new_secondary(
            "./local-storage",
            "./local-secondary-storage",
            &[&Tx::get_cf()],
        )?;
        let client = Self::new(contract_address, rpc_url, secret_key.into(), db);
        Ok(client)
    }

    pub fn new(
        contract_address: Address, rpc_url: Url, signer: LocalSigner<SigningKey>, db: Db,
    ) -> Self {
        Self { contract_address, rpc_url, signer, db }
    }

    pub async fn submit_openrank_tx(&self, tx: Tx) -> Result<()> {
        // create a contract instance
        let wallet = EthereumWallet::from(self.signer.clone());
        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(self.rpc_url.clone());
        let contract = JobManager::new(self.contract_address, provider);

        // check if tx already exists
        let is_tx_exists = contract.hasTx(tx.hash().0.into()).call().await?._0;
        if is_tx_exists {
            return Ok(());
        }

        // submit tx
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

    fn read_txs(&self) -> Result<Vec<Tx>> {
        // collect all txs
        let mut txs = Vec::new();
        let mut job_run_request_txs: Vec<Tx> = self
            .db
            .read_from_end(TxKind::JobRunRequest.into(), None)
            .map_err(|e| eyre::eyre!(e))?;
        txs.append(&mut job_run_request_txs);
        drop(job_run_request_txs);

        let mut job_run_assignment_txs: Vec<Tx> = self
            .db
            .read_from_end(TxKind::JobRunAssignment.into(), None)
            .map_err(|e| eyre::eyre!(e))?;
        txs.append(&mut job_run_assignment_txs);
        drop(job_run_assignment_txs);

        let mut create_commitment_txs: Vec<Tx> = self
            .db
            .read_from_end(TxKind::CreateCommitment.into(), None)
            .map_err(|e| eyre::eyre!(e))?;
        txs.append(&mut create_commitment_txs);
        drop(create_commitment_txs);

        let mut job_verification_txs: Vec<Tx> = self
            .db
            .read_from_end(TxKind::JobVerification.into(), None)
            .map_err(|e| eyre::eyre!(e))?;
        txs.append(&mut job_verification_txs);
        drop(job_verification_txs);

        // sort txs by sequence_number
        txs.sort_unstable_by_key(|tx| tx.sequence_number());

        Ok(txs)
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            let txs = self.read_txs()?;
            for tx in txs {
                self.submit_openrank_tx(tx).await?;
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use alloy::{network::EthereumWallet, node_bindings::Anvil, signers::local::PrivateKeySigner};
    use alloy_rlp::encode;

    use openrank_common::txs::JobRunRequest;

    use super::*;

    #[tokio::test]
    async fn test_submit_openrank_tx() -> Result<()> {
        let test_mnemonic = String::from(
            "work man father plunge mystery proud hollow address reunion sauce theory bonus",
        );

        // Spin up a local Anvil node.
        // Ensure `anvil` is available in $PATH.
        let anvil = Anvil::new().mnemonic(&test_mnemonic).try_spawn()?;

        // Set up signer from the first default Anvil account (Alice).
        let signer: PrivateKeySigner = anvil.keys()[0].clone().into();
        let wallet = EthereumWallet::from(signer.clone());

        // Create a provider with the wallet.
        let rpc_url: Url = anvil.endpoint().parse()?;
        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(rpc_url.clone());

        // Deploy the `JobManager` contract.
        let contract = JobManager::deploy(&provider, vec![], vec![], vec![]).await?;

        // Create a contract instance.
        let contract_address = contract.address().clone();
        let db = Db::new("test-pg-storage", &[&Tx::get_cf()]).unwrap();
        let client = JobManagerClient::new(contract_address, rpc_url, signer, db);

        // Call the `submitJobRunRequest` function for testing.
        let _ = client
            .submit_openrank_tx(Tx::default_with(
                TxKind::JobRunRequest,
                encode(JobRunRequest::default()),
            ))
            .await?;

        Ok(())
    }
}
