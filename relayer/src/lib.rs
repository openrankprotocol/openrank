use log::{debug, error, info};
use openrank_common::{
    db::Db,
    // result::JobResult,
    topics::{Domain, Topic},
    tx_event::TxEvent,
    txs::{
        Address,
        Kind,
        // CreateCommitment,
        // CreateScores,
        // JobRunAssignment,
        // JobRunRequest,
        // JobVerification,
        Tx,
        TxHash,
    },
};
// use alloy_rlp::decode::Decodable;
use openrank_common::db::DbItem;
use rocksdb::{ColumnFamilyDescriptor, DBWithThreadMode, MultiThreaded, Options, DB};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::time::{sleep, Duration};
// use alloy_rlp::decode::Decodable;
use alloy_rlp::Decodable;
use std::panic;

mod postgres;

#[derive(Serialize, Deserialize, Debug)]
struct TxWithHash {
    tx: Tx,
    hash: TxHash,
}

pub struct SQLRelayer {
    // todo use only common db, here because common lib db does not expose iterator
    dbs: HashMap<String, (DBWithThreadMode<MultiThreaded>, String)>,
    last_processed_keys: HashMap<String, Option<usize>>,
    target_db: postgres::SQLDatabase,
}

impl SQLRelayer {
    pub async fn init(db_configs: Vec<(&str, &str)>, is_reindex: bool) -> Self {
        let target_db = postgres::SQLDatabase::connect().await.expect("Connect to Postgres db");

        if is_reindex {
            log::info!("Reindexing: dropping tables.");
            target_db.drop_tables().await;
        }

        target_db.init().await;

        let mut dbs = HashMap::new();
        let mut last_processed_keys = HashMap::new();

        for (db_path, topic) in db_configs {
            let last_processed_key = target_db
                .load_last_processed_key(&format!("relayer_last_key_{}_{}", db_path, topic))
                .await
                .expect("Failed to load last processed key");

            let path = Path::new(db_path);
            let mut opts = Options::default();
            opts.create_if_missing(false);
            opts.create_missing_column_families(true);

            let cfs = &["tx", "metadata"];
            let db =
                DBWithThreadMode::<MultiThreaded>::open_cf_for_read_only(&opts, path, cfs, false)
                    .expect("Failed to open RocksDB with column families");

            dbs.insert(db_path.to_string(), (db, topic.to_string()));
            last_processed_keys.insert(db_path.to_string(), last_processed_key);
        }

        SQLRelayer { dbs, last_processed_keys, target_db }
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

    async fn index(&mut self) {
        for (db_path, (db, topic)) in &self.dbs {
            let cf = db.cf_handle(topic).expect(&format!("Column family '{}' not found", topic));
            let iter = db.iterator_cf(&cf, rocksdb::IteratorMode::Start);

            let mut count = 0;

            debug!(
                "last_processed_key for {} and topic {}: {:?}",
                db_path, topic, self.last_processed_keys[db_path]
            );

            for item in iter {
                match item {
                    Ok((key, value)) => {
                        let key_str = String::from_utf8_lossy(&key);
                        let value_str = String::from_utf8_lossy(&value);

                        if self.last_processed_keys[db_path].is_none()
                            || self.last_processed_keys[db_path].unwrap() < count
                        {
                            let tx: Tx = serde_json::from_str(value_str.as_ref())
                                .expect("Failed to deserialize JSON");
                            //let tx: JobResult = serde_json::from_str(value_str.as_ref())
                            // .expect("Failed to deserialize JSON");

                            // println!("tx kind {:?}", tx.kind());
                            let mut tx_body_raw = tx.body();

                            let result =
                                panic::catch_unwind(|| Kind::decode(&mut tx_body_raw.as_slice()));

                            match result {
                                Ok(Ok(tx_body)) => {
                                    println!("tx body decoded successfully");
                                },
                                Ok(Err(e)) => {
                                    eprintln!("Failed to decode tx body: {:?}", e);
                                },
                                Err(_) => {
                                    eprintln!("Panic occurred while decoding the tx body");
                                },
                            }

                            let tx_with_hash = TxWithHash { tx: tx.clone(), hash: tx.hash() };
                            let serialized_tx = serde_json::to_string(&tx_with_hash)
                                .expect("Failed to serialize TxWithHash");

                            self.target_db.insert_events(&key_str, &serialized_tx).await;

                            self.last_processed_keys.insert(db_path.clone(), Some(count));
                            self.save_last_processed_key(db_path, topic, count).await;
                        }
                    },
                    Err(e) => {
                        error!(
                            "Error iterating over DB {} and topic {}: {}",
                            db_path, topic, e
                        );
                    },
                }
                count += 1;
            }

            info!(
                "Total records processed for {} and topic {}: {}",
                db_path, topic, count
            );
        }
    }

    pub async fn start(&mut self) {
        let INTERVAL_SECONDS = 10;
        let mut interval = tokio::time::interval(Duration::from_secs(INTERVAL_SECONDS));

        loop {
            interval.tick().await;
            info!("Running periodic index check...");
            self.index().await;
        }
    }
}
