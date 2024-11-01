use crate::types::TxWithHash;
use log::info;
use openrank_common::db::{self, Db, DbItem};
use openrank_common::tx::{self, compute, consts};
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
            &[tx::Tx::get_cf().as_str(), compute::Result::get_cf().as_str()],
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

    fn get_tx_with_hash(&self, kind: &str, hash: tx::TxHash) -> (Vec<u8>, TxWithHash) {
        let tx_key = tx::Tx::construct_full_key(kind, hash);
        let tx = self.db.get::<tx::Tx>(tx_key.clone()).unwrap();
        let tx_with_hash = TxWithHash { tx: tx.clone(), hash: tx.hash() };
        (tx_key, tx_with_hash)
    }

    async fn index(&mut self) {
        // Refresh secondary instance before starting
        self.db.refresh().unwrap();

        let results = self.db.read_from_end::<compute::Result>("result", None).unwrap();
        let dir = self.db.get_config().secondary.expect("Secondary path missing");
        let last_count = self.last_processed_keys[dir.as_str()].unwrap_or(0);
        let mut current_count = 0;

        log::info!("Indexing db, last_count: {:?}", last_count);
        for res in &results[0..] {
            // Ensure Result's are loaded in sequence
            // assert_eq!(last_count as u64, res.seq_number.unwrap());

            // ComputeRequest
            let (request_key, request_tx_with_hash) =
                self.get_tx_with_hash(consts::COMPUTE_REQUEST, res.compute_request_tx_hash.clone());

            current_count += 1;

            let request_key_str = String::from_utf8_lossy(&request_key);

            if current_count > last_count {
                self.target_db
                    .insert_events(&request_key_str, &request_tx_with_hash)
                    .await
                    .unwrap();
            }

            // ComputeCommitment
            let (commitment_key, commitment_tx_with_hash) = self.get_tx_with_hash(
                consts::COMPUTE_COMMITMENT,
                res.compute_commitment_tx_hash.clone(),
            );

            current_count += 1;

            let commitment_key_str = String::from_utf8_lossy(&commitment_key);

            if current_count > last_count {
                self.target_db
                    .insert_events(&commitment_key_str, &commitment_tx_with_hash)
                    .await
                    .unwrap();
            }

            // ComputeVerification
            for verification_tx_hash in res.compute_verification_tx_hashes.clone() {
                current_count += 1;

                let (verification_key, verification_tx_with_hash) =
                    self.get_tx_with_hash(consts::COMPUTE_VERIFICATION, verification_tx_hash);

                let verification_key_str = String::from_utf8_lossy(&verification_key);

                if current_count > last_count {
                    self.target_db
                        .insert_events(&verification_key_str, &verification_tx_with_hash)
                        .await
                        .unwrap();
                }
            }
        }

        let trust_updates = self.db.read_from_end::<tx::Tx>(consts::TRUST_UPDATE, None).unwrap();

        for update in trust_updates {
            current_count += 1;

            let trust_update_key = tx::Tx::construct_full_key(consts::TRUST_UPDATE, update.hash());
            let trust_update_tx_with_hash = TxWithHash { tx: update.clone(), hash: update.hash() };

            let trust_update_key_str = String::from_utf8_lossy(&trust_update_key);

            if current_count > last_count {
                self.target_db
                    .insert_events(&trust_update_key_str, &trust_update_tx_with_hash)
                    .await
                    .unwrap();
            }
        }

        let seed_updates = self.db.read_from_end::<tx::Tx>(consts::SEED_UPDATE, None).unwrap();

        for update in seed_updates {
            current_count += 1;

            let seed_update_key = tx::Tx::construct_full_key(consts::SEED_UPDATE, update.hash());
            let seed_update_tx_with_hash = TxWithHash { tx: update.clone(), hash: update.hash() };

            let seed_update_key_str = String::from_utf8_lossy(&seed_update_key);
            if current_count > last_count {
                self.target_db
                    .insert_events(&seed_update_key_str, &seed_update_tx_with_hash)
                    .await
                    .unwrap();
            }
        }

        if last_count < current_count {
            self.last_processed_keys.insert(dir.clone(), Some(current_count));
            self.save_last_processed_key(dir.as_str(), "tx", current_count).await;
        }
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
