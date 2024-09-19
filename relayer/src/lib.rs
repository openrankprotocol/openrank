use log::{debug, error, info};
use rocksdb::{ColumnFamilyDescriptor, DBWithThreadMode, MultiThreaded, Options, DB};
use std::collections::HashMap;
use std::path::Path;
use tokio::time::{sleep, Duration};

mod postgres;

pub struct SQLRelayer {
    dbs: HashMap<String, (DBWithThreadMode<MultiThreaded>, String)>, // Store multiple databases with paths and topics
    last_processed_keys: HashMap<String, Option<usize>>, // Track last processed key per database
    target_db: postgres::SQLDatabase,
}

impl SQLRelayer {
    pub async fn init(db_configs: Vec<(&str, &str)>) -> Self {
        let target_db = postgres::SQLDatabase::connect().await.expect("Connect to Postgres db");

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

            let cfs = vec![ColumnFamilyDescriptor::new(topic, Options::default())];

            let db = DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(&opts, path, cfs)
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
                        let key_index = key.to_vec();

                        if self.last_processed_keys[db_path].is_none()
                            || self.last_processed_keys[db_path].unwrap() < count
                        {
                            let key_str = String::from_utf8_lossy(&key);
                            let value_str = String::from_utf8_lossy(&value);
                            debug!("New Key: {}, Value length: {}", key_str, value_str.len());
                            self.target_db.insert_events(&key_str, &value_str).await;

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
