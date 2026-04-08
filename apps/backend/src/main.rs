//! Backend app crate for the Day 1 PoC.
//!
//! This crate owns HTTP wiring, in-memory request state, and runtime bootstrap.
//! MUSUBI domain concepts should move into workspace crates as they become pure.

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    routing::{get, post},
};
use serde::Serialize;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

mod handlers;
mod services;

pub type SharedState = Arc<AppState>;

/// Temporary app-state wiring for the PoC.
///
/// This remains in the app crate until Issue #3 and later introduce durable
/// truth boundaries. It should not become long-term domain ownership.
pub struct AppState {
    pub escrows: RwLock<HashMap<String, services::escrow::EscrowRecord>>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let host = std::env::var("APP_HOST").unwrap_or_else(|_| "0.0.0.0".to_owned());
    let port = std::env::var("PORT")
        .or_else(|_| std::env::var("APP_PORT"))
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8088);

    let state = Arc::new(AppState {
        escrows: RwLock::new(HashMap::new()),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/auth/pi", post(handlers::auth::authenticate_pi))
        .route(
            "/api/payment/callback",
            post(handlers::payments::payment_callback),
        )
        .layer(cors)
        .with_state(state);

    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("failed to parse listen address");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind tcp listener");

    println!("musubi backend listening on http://{address}");

    axum::serve(listener, app)
        .await
        .expect("backend server exited unexpectedly");
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "musubi-backend",
    })
}
