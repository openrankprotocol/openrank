use async_graphql::{Context, Object, Schema, SimpleObject};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

const DEFAULT_LIMIT: i32 = 10;

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn events(
        &self, ctx: &Context<'_>, limit: Option<i32>, offset: Option<i32>, hash: Option<String>,
    ) -> Vec<Event> {
        let pool = ctx.data::<PgPool>().unwrap();

        let limit = limit.unwrap_or(DEFAULT_LIMIT);
        let offset = offset.unwrap_or(0);

        let query = if let Some(ref hash) = hash {
            sqlx::query_as::<_, Event>(
                "SELECT id, event_id, event_body, hash FROM events WHERE hash = $1 LIMIT $2 OFFSET $3"
            )
            .bind(hash)
            .bind(limit)
            .bind(offset)
        } else {
            sqlx::query_as::<_, Event>(
                "SELECT id, event_id, event_body, hash FROM events LIMIT $1 OFFSET $2",
            )
            .bind(limit)
            .bind(offset)
        };

        query.fetch_all(pool).await.unwrap()
    }
}

#[derive(SimpleObject, sqlx::FromRow, Serialize, Deserialize)]
pub struct Event {
    pub id: i32,
    pub event_id: String,
    pub event_body: Value,
    pub hash: String,
}

pub type MySchema =
    Schema<QueryRoot, async_graphql::EmptyMutation, async_graphql::EmptySubscription>;

pub fn build_schema(pool: PgPool) -> MySchema {
    Schema::build(
        QueryRoot,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    )
    .data(pool)
    .finish()
}
