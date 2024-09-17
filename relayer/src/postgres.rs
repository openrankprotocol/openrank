use dotenv::dotenv;
use log::{error, info, warn};
use serde_json::Value;
use std::env;
use tokio_postgres::{Client, Error, NoTls};

pub struct SQLDatabase {
    client: Client,
}

impl SQLDatabase {
    pub async fn connect() -> Result<Self, Error> {
        let host = env::var("DB_HOST").expect("DB_HOST is not set");
        let user = env::var("DB_USER").expect("DB_USER is not set");
        let password = env::var("DB_PASSWORD").expect("DB_PASSWORD is not set");
        let dbname = env::var("DB_NAME").expect("DB_NAME is not set");

        let conn_str = format!(
            "host={} user={} password={} dbname={}",
            host, user, password, dbname
        );
        info!("Connecting to database: {}", conn_str);

        let (client, connection) = tokio_postgres::connect(&conn_str, NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Connection error: {}", e);
            }
        });

        Ok(SQLDatabase { client })
    }

    pub async fn insert_events(&self, event_id: &str, event_body: &str) -> Result<(), Error> {
        let event_body_json: Value = serde_json::from_str(event_body).unwrap();
        let event_body_json_str = event_body_json.to_string();

        if let Some(name) = event_body_json.get("to") {
            println!("Name: {}", name.as_str().unwrap());
        }

        let result = self.client.execute(
            "INSERT INTO events (event_id, event_body) VALUES ($1, $2) ON CONFLICT (event_id) DO NOTHING",
            &[&event_id, &event_body_json]
        ).await;

        match result {
            Ok(rows) => {
                if rows == 0 {
                    log::warn!(
                        "No rows inserted, possibly due to conflict with event_id '{}'",
                        event_id
                    );
                } else {
                    log::info!("Inserted {} row(s) into events table.", rows);
                }
                Ok(())
            },
            Err(e) => {
                if let Some(db_error) = e.as_db_error() {
                    if db_error.message().contains("duplicate key value violates unique constraint")
                    {
                        log::warn!("Conflict occurred: event_id '{}' already exists", event_id);
                    } else {
                        log::error!("Error inserting event: {}", db_error.message());
                    }
                } else {
                    log::error!("Error inserting event: {}", e);
                }
                Err(e)
            },
        }
    }

    pub async fn load_last_processed_key(&self, key_name: &str) -> Result<Option<usize>, Error> {
        let row = self
            .client
            .query_opt(
                "SELECT last_processed_key FROM state WHERE key_name = $1",
                &[&key_name],
            )
            .await?;

        if let Some(row) = row {
            let last_processed_key: usize = row.get::<usize, i32>(0) as usize;
            Ok(Some(last_processed_key))
        } else {
            Ok(None)
        }
    }

    pub async fn save_last_processed_key(&self, key_name: &str, key: i32) -> Result<(), Error> {
        self.client
            .execute(
                "INSERT INTO state (key_name, last_processed_key, updated_at) 
                 VALUES ($1, $2, NOW()) 
                 ON CONFLICT (key_name) 
                 DO UPDATE SET last_processed_key = $2, updated_at = NOW()",
                &[&key_name, &key],
            )
            .await?;
        Ok(())
    }
}
