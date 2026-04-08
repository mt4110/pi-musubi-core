use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    routing::{get, post},
};
use serde::Serialize;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

pub mod handlers;
pub mod services;

pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub happy_route: RwLock<services::happy_route::HappyRouteState>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

pub fn new_state() -> SharedState {
    Arc::new(AppState {
        happy_route: RwLock::new(services::happy_route::HappyRouteState::default()),
    })
}

pub fn build_app(state: SharedState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/auth/pi", post(handlers::auth::authenticate_pi))
        .route(
            "/api/promise/intents",
            post(handlers::promise_intents::create_promise_intent),
        )
        .route(
            "/api/projection/settlement-views/{settlement_case_id}",
            get(handlers::projection::get_settlement_view),
        );
    let app = if unauthenticated_pi_callback_enabled() {
        app.route(
            "/api/payment/callback",
            post(handlers::payments::payment_callback),
        )
    } else {
        app
    };
    let app = if internal_orchestration_drain_enabled() {
        app.route(
            "/api/internal/orchestration/drain",
            post(handlers::orchestration::drain_outbox),
        )
    } else {
        app
    };

    app.layer(cors).with_state(state)
}

pub async fn run(state: SharedState) {
    let host = std::env::var("APP_HOST").unwrap_or_else(|_| "0.0.0.0".to_owned());
    let port = std::env::var("PORT")
        .or_else(|_| std::env::var("APP_PORT"))
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8088);

    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("failed to parse listen address");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind tcp listener");

    println!("musubi backend listening on http://{address}");

    axum::serve(listener, build_app(state))
        .await
        .expect("backend server exited unexpectedly");
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "musubi-backend",
    })
}

fn internal_orchestration_drain_enabled() -> bool {
    cfg!(debug_assertions) || env_flag_enabled("MUSUBI_ENABLE_INTERNAL_ORCHESTRATION_DRAIN")
}

fn unauthenticated_pi_callback_enabled() -> bool {
    cfg!(debug_assertions) || env_flag_enabled("MUSUBI_ENABLE_UNAUTHENTICATED_PI_CALLBACK")
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes"
        })
        .unwrap_or(false)
}
