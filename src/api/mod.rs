use crate::state::AppState;
use axum::Router;
use axum::routing::get;
use std::sync::{Arc, RwLock};

pub mod handlers;
pub mod responses;

pub fn router(state: Arc<RwLock<AppState>>) -> Router {
    Router::new()
        .route("/api/queue", get(handlers::get_queue))
        .route("/api/health", get(handlers::get_health))
        .route("/api/sensors", get(handlers::get_sensors))
        .route("/api/debug/readings", get(handlers::get_debug_readings))
        .with_state(state)
}
