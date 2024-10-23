use crate::api::connect::get_db_pool;
use crate::api::schema::build_schema;
use crate::api::schema::MySchema;
use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql_warp::{graphql, GraphQLResponse};
use dotenv::dotenv;
use sqlx::PgPool;
use std::sync::Arc;
use warp::http::Method;
use warp::Filter;

use crate::api::schema;

pub async fn serve() {
    dotenv().ok();

    let pool = Arc::new(get_db_pool().await);
    let schema = build_schema((*pool).clone());

    let graphql_filter = async_graphql_warp::graphql(schema.clone()).and_then(
        |(schema, request): (MySchema, async_graphql::Request)| async move {
            let resp = schema.execute(request).await;
            Ok::<_, warp::Rejection>(GraphQLResponse::from(resp))
        },
    );

    let playground = warp::path::end()
        .map(|| warp::reply::html(playground_source(GraphQLPlaygroundConfig::new("/graphql"))));

    let routes = warp::path("graphql")
        .and(warp::post().and(graphql_filter.clone()))
        .or(warp::get().and(playground));

    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(&[Method::GET, Method::POST])
        .allow_headers(vec!["content-type", "authorization"]);

    let host = ([127, 0, 0, 1], 3030);

    warp::serve(routes.with(cors)).run(host).await;
}
