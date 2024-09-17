use log::{debug, error, info};
use rocksdb::{ColumnFamilyDescriptor, DBWithThreadMode, MultiThreaded, Options, DB};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use tokio::time::{sleep, Duration};

mod postgres;

pub struct SQLRelayer {
    db: Option<DBWithThreadMode<MultiThreaded>>,
    last_processed_key: Option<usize>,
    target_db: postgres::SQLDatabase,
}

impl SQLRelayer {
    pub async fn init(db_path: &str) -> Self {
        let target_db = postgres::SQLDatabase::connect().await.expect("Connect to Postgres db");

        let last_processed_key = target_db
            .load_last_processed_key("relayer_last_key")
            .await
            .expect("Failed to load last processed key");

        let path = Path::new(db_path);
        let mut opts = Options::default();
        opts.create_if_missing(false);
        opts.create_missing_column_families(true);

        let cfs = vec![ColumnFamilyDescriptor::new("tx", Options::default())];

        let db = DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(&opts, path, cfs)
            .expect("Failed to open RocksDB with column families");

        SQLRelayer { db: Some(db), last_processed_key, target_db }
    }

    async fn save_last_processed_key(&self) {
        if let Some(last_processed_key) = self.last_processed_key {
            self.target_db
                .save_last_processed_key("relayer_last_key", last_processed_key as i32)
                .await
                .expect("Failed to save last processed key");
        }
    }

    async fn index(&mut self) {
        if let Some(ref db) = self.db {
            let cf = db.cf_handle("tx").expect("Column family 'tx' not found");
            let iter = db.iterator_cf(&cf, rocksdb::IteratorMode::Start);

            let mut count = 0;

            log::debug!("last_processed_key {:?}", self.last_processed_key);

            for item in iter {
                match item {
                    Ok((key, value)) => {
                        let key_index = key.to_vec();

                        if self.last_processed_key.is_none()
                            || self.last_processed_key.unwrap() < count
                        {
                            let key_str = String::from_utf8_lossy(&key);
                            let value_str = String::from_utf8_lossy(&value);
                            debug!("New Key: {}, Value length: {}", key_str, value_str.len());
                            self.target_db.insert_events(&key_str, &value_str).await;

                            // Update the last processed key
                            self.last_processed_key = Some(count);
                            self.save_last_processed_key().await;
                        }
                    },
                    Err(e) => {
                        error!("Error iterating over DB: {}", e);
                    },
                }
                count += 1;
            }

            info!("Total records processed: {}", count);
        } else {
            error!("No connection to RocksDB.");
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
