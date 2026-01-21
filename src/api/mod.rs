use crate::state::AppState;
use axum::routing::get;
use axum::Router;
use std::sync::{Arc, RwLock};

pub mod handlers;
pub mod responses;

pub fn router(state: Arc<RwLock<AppState>>) -> Router {
    Router::new()
        .route("/api/queue", get(handlers::get_queue))
        .with_state(state)
}
