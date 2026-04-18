use std::{
    net::SocketAddr,
    sync::{Arc, OnceLock},
    time::Duration,
};

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use serde::Serialize;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard, RwLock};
use tokio::task::JoinHandle;
use tower_http::cors::{Any, CorsLayer};

pub mod handlers;
pub mod services;

use musubi_db_runtime::{DbConfig, MigrationRunner, StartupCheck};

pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub happy_route: services::happy_route::HappyRouteStore,
    pub proof: RwLock<services::proof::ProofState>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

pub async fn new_state() -> musubi_db_runtime::Result<SharedState> {
    load_env_files();
    let config = DbConfig::from_env()?;
    new_state_from_config(&config).await
}

pub async fn new_state_from_config(config: &DbConfig) -> musubi_db_runtime::Result<SharedState> {
    Ok(Arc::new(AppState {
        happy_route: services::happy_route::HappyRouteStore::connect(config).await?,
        proof: RwLock::new(services::proof::ProofState::default()),
    }))
}

pub struct TestState {
    pub state: SharedState,
    _guard: OwnedMutexGuard<()>,
}

static TEST_DB_LOCK: OnceLock<Arc<AsyncMutex<()>>> = OnceLock::new();
static TEST_DB_MIGRATED: OnceLock<Arc<AsyncMutex<bool>>> = OnceLock::new();

pub async fn new_test_state() -> Result<TestState, String> {
    load_env_files();
    let guard = TEST_DB_LOCK
        .get_or_init(|| Arc::new(AsyncMutex::new(())))
        .clone()
        .lock_owned()
        .await;
    let config = test_db_config().map_err(|error| error.to_string())?;
    let migrated = TEST_DB_MIGRATED
        .get_or_init(|| Arc::new(AsyncMutex::new(false)))
        .clone();
    {
        let mut migrated = migrated.lock().await;
        if !*migrated {
            let runner = MigrationRunner::new(config.migrations_dir.clone());
            runner
                .bootstrap(&config)
                .await
                .map_err(|error| error.to_string())?;
            runner
                .migrate(&config)
                .await
                .map_err(|error| error.to_string())?;
            *migrated = true;
        }
    }
    let state = new_state_from_config(&config)
        .await
        .map_err(|error| error.to_string())?;
    state
        .happy_route
        .reset_for_test()
        .await
        .map_err(|error| error.message().to_owned())?;
    Ok(TestState {
        state,
        _guard: guard,
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
            "/api/proof/challenges",
            post(handlers::proof::start_proof_challenge).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/proof/submissions",
            post(handlers::proof::submit_proof_envelope).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/projection/settlement-views/{settlement_case_id}",
            get(handlers::projection::get_settlement_view),
        );
    let app = if unauthenticated_pi_callback_enabled() {
        app.route(
            "/api/payment/callback",
            post(handlers::payments::payment_callback).layer(DefaultBodyLimit::max(16 * 1024)),
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

pub async fn verify_backend_startup() -> musubi_db_runtime::Result<StartupCheck> {
    load_env_files();
    let config = DbConfig::from_env()?;
    MigrationRunner::new(config.migrations_dir.clone())
        .verify_startup(&config)
        .await
}

pub async fn run(state: SharedState) {
    let _background_outbox_worker = if internal_orchestration_drain_enabled() {
        None
    } else {
        Some(start_background_outbox_worker(state.clone()))
    };
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

pub fn start_background_outbox_worker(state: SharedState) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Err(error) = services::happy_route::drain_outbox(&state).await {
                eprintln!("background outbox worker error: {}", error.message());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "musubi-backend",
    })
}

fn internal_orchestration_drain_enabled() -> bool {
    internal_orchestration_drain_enabled_with_flags(
        cfg!(debug_assertions),
        env_flag_enabled("MUSUBI_DISABLE_INTERNAL_ORCHESTRATION_DRAIN"),
        env_flag_enabled("MUSUBI_ENABLE_INTERNAL_ORCHESTRATION_DRAIN"),
    )
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

fn load_env_files() {
    dotenvy::dotenv().ok();
    dotenvy::from_path("apps/backend/.env").ok();
}

fn test_db_config() -> musubi_db_runtime::Result<DbConfig> {
    DbConfig::from_lookup(|name| match name {
        "APP_ENV" => Some("test".to_owned()),
        "DATABASE_URL" => std::env::var("MUSUBI_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok(),
        other => std::env::var(other).ok(),
    })
}

fn internal_orchestration_drain_enabled_with_flags(
    debug_build: bool,
    disable_internal_drain: bool,
    enable_internal_drain: bool,
) -> bool {
    if disable_internal_drain {
        return false;
    }

    debug_build || enable_internal_drain
}

#[cfg(test)]
mod tests {
    use super::internal_orchestration_drain_enabled_with_flags;

    #[test]
    fn debug_build_can_disable_internal_drain() {
        assert!(!internal_orchestration_drain_enabled_with_flags(
            true, true, false,
        ));
    }

    #[test]
    fn release_build_can_enable_internal_drain() {
        assert!(internal_orchestration_drain_enabled_with_flags(
            false, false, true,
        ));
    }
}
