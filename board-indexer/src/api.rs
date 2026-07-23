use crate::store::Store;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub fn router(store: Arc<Store>) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/v1/status", get(status))
        .route("/v1/boards", get(boards))
        .route("/v1/catalog", get(catalog))
        .route("/v1/thread/:op_txid", get(thread))
        .with_state(store)
}

#[derive(Serialize)]
struct Health {
    ok: bool,
}

async fn health() -> Json<Health> {
    Json(Health { ok: true })
}

async fn status(State(store): State<Arc<Store>>) -> impl IntoResponse {
    match store.status() {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => internal(error),
    }
}

#[derive(Serialize)]
struct Boards {
    boards: Vec<String>,
}

async fn boards(State(store): State<Arc<Store>>) -> impl IntoResponse {
    match store.boards() {
        Ok(boards) => (StatusCode::OK, Json(Boards { boards })).into_response(),
        Err(error) => internal(error),
    }
}

#[derive(Deserialize)]
struct CatalogQuery {
    board: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn catalog(
    State(store): State<Arc<Store>>,
    Query(query): Query<CatalogQuery>,
) -> impl IntoResponse {
    match store.catalog(
        query.board.as_deref(),
        query.limit.unwrap_or(100).clamp(1, 1000),
        query.offset.unwrap_or(0),
    ) {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(error) => internal(error),
    }
}

#[derive(Deserialize)]
struct ThreadQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn thread(
    State(store): State<Arc<Store>>,
    Path(op_txid): Path<String>,
    Query(query): Query<ThreadQuery>,
) -> impl IntoResponse {
    match store.thread(
        &op_txid,
        query.limit.unwrap_or(200).clamp(1, 10_000),
        query.offset.unwrap_or(0),
    ) {
        Ok(Some(value)) => (StatusCode::OK, Json(value)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal(error),
    }
}

fn internal(error: anyhow::Error) -> axum::response::Response {
    eprintln!("read API error: {error:#}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}
