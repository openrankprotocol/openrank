use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::env;

pub async fn get_db_pool() -> PgPool {
    let db_host = env::var("DB_HOST").expect("DB_HOST must be set");
    let db_user = env::var("DB_USER").expect("DB_USER must be set");
    let db_password = env::var("DB_PASSWORD").expect("DB_PASSWORD must be set");
    let db_name = env::var("DB_NAME").expect("DB_NAME must be set");

    let database_url = format!(
        "postgres://{}:{}@{}/{}",
        db_user, db_password, db_host, db_name
    );

    PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to create pool.")
}
