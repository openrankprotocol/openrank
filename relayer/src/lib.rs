use crate::types::TxWithHash;
use alloy_rlp::Decodable;
use log::info;
use openrank_common::db::{self, Db, DbItem};
use openrank_common::txs::{self, compute, trust};
use std::collections::HashMap;
use tokio::time::Duration;

mod postgres;
mod types;

const INTERVAL_SECONDS: u64 = 10;

pub struct SQLRelayer {
    // todo use only common db, here because common lib db does not expose iterator
    db: Db,
    last_processed_keys: HashMap<String, Option<usize>>,
    target_db: postgres::SQLDatabase,
}

impl SQLRelayer {
    pub async fn init(db_config: db::Config, is_reindex: bool) -> Self {
        let target_db = postgres::SQLDatabase::connect().await.expect("Connect to Postgres db");

        if is_reindex {
            log::info!("Reindexing: dropping tables.");
            target_db.drop_tables().await.unwrap();
        }

        target_db.init().await.unwrap();

        let mut last_processed_keys = HashMap::new();

        let path = db_config.clone().secondary.expect("No secondary path found");
        let last_processed_key = target_db
            .load_last_processed_key(&format!("relayer_last_key_{}_{}", path, "tx"))
            .await
            .expect("Failed to load last processed key");

        let db = Db::new_secondary(
            &db_config,
            &[txs::Tx::get_cf().as_str(), compute::Result::get_cf().as_str()],
        )
        .unwrap();
        last_processed_keys.insert(path, last_processed_key);

        SQLRelayer { db, last_processed_keys, target_db }
    }

    async fn save_last_processed_key(&self, db_path: &str, topic: &str, last_processed_key: usize) {
        self.target_db
            .save_last_processed_key(
                &format!("relayer_last_key_{}_{}", db_path, topic),
                last_processed_key as i32,
            )
            .await
            .expect("Failed to save last processed key");
    }

    fn get_tx_with_hash(&self, kind: txs::Kind, hash: txs::TxHash) -> (Vec<u8>, TxWithHash) {
        let tx_key = txs::Tx::construct_full_key(kind, hash);
        let tx = self.db.get::<txs::Tx>(tx_key.clone()).unwrap();
        let tx_with_hash = TxWithHash { tx: tx.clone(), hash: tx.hash() };
        (tx_key, tx_with_hash)
    }

    async fn index(&mut self) {
        let results = self.db.read_from_end::<compute::Result>("result".to_string(), None).unwrap();

        let dir = self.db.get_config().secondary.expect("Secondary path missing");
        let mut last_count = self.last_processed_keys[dir.as_str()].unwrap_or(0);

        for res in &results[last_count..] {
            // Ensure Result's are loaded in sequence
            assert_eq!(last_count as u64, res.seq_number.unwrap());

            // ComputeRequest
            let (request_key, request_tx_with_hash) = self.get_tx_with_hash(
                txs::Kind::ComputeRequest,
                res.compute_request_tx_hash.clone(),
            );

            // Example for decoding the body of ComputeRequest --------------------------------
            let request_body =
                compute::Request::decode(&mut request_tx_with_hash.tx.body().as_slice()).unwrap();
            // println!("{:?}", request_body);
            // ---------------------------------------------------------------------------------

            let request_key_str = String::from_utf8_lossy(&request_key);
            self.target_db.insert_events(&request_key_str, &request_tx_with_hash).await.unwrap();

            // ComputeCommitment
            let (commitment_key, commitment_tx_with_hash) = self.get_tx_with_hash(
                txs::Kind::ComputeCommitment,
                res.compute_commitment_tx_hash.clone(),
            );

            // Example for decoding the body of ComputeCommitment ----------------------------
            let commitment_body =
                compute::Commitment::decode(&mut commitment_tx_with_hash.tx.body().as_slice())
                    .unwrap();
            // println!("{:?}", commitment_body);
            // ---------------------------------------------------------------------------------

            let commitment_key_str = String::from_utf8_lossy(&commitment_key);
            self.target_db
                .insert_events(&commitment_key_str, &commitment_tx_with_hash)
                .await
                .unwrap();

            // ComputeVerification
            for verification_tx_hash in res.compute_verification_tx_hashes.clone() {
                let (verification_key, verification_tx_with_hash) =
                    self.get_tx_with_hash(txs::Kind::ComputeVerification, verification_tx_hash);

                // Example for decoding the body of ComputeVerification ----------------------------
                let verification_body = compute::Verification::decode(
                    &mut verification_tx_with_hash.tx.body().as_slice(),
                )
                .unwrap();
                // println!("{:?}", verification_body);
                // ---------------------------------------------------------------------------------

                let verification_key_str = String::from_utf8_lossy(&verification_key);
                self.target_db
                    .insert_events(&verification_key_str, &verification_tx_with_hash)
                    .await
                    .unwrap();
            }

            self.last_processed_keys.insert(dir.clone(), Some(last_count));
            self.save_last_processed_key(dir.as_str(), "tx", last_count).await;
            last_count += 1;
        }

        // Example on how to read trust updates
        let trust_updates =
            self.db.read_from_end::<txs::Tx>(txs::Kind::TrustUpdate.into(), None).unwrap();
        // println!("{:?}", trust_updates);

        for update in trust_updates {
            // Example for decoding the body of TrustUpdate ----------------------------
            let trust_update_body =
                trust::TrustUpdate::decode(&mut update.body().as_slice()).unwrap();
            println!("{:?}", trust_update_body);
            // ---------------------------------------------------------------------------------
        }

        // Example on how to read seed updates
        let seed_updates =
            self.db.read_from_end::<txs::Tx>(txs::Kind::SeedUpdate.into(), None).unwrap();
        // println!("{:?}", seed_updates);
    }

    pub async fn start(&mut self) {
        let mut interval = tokio::time::interval(Duration::from_secs(INTERVAL_SECONDS));

        loop {
            interval.tick().await;
            info!("Running periodic index check...");
            self.index().await;
        }
    }
}
