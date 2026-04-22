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
    pub ops_observability: services::ops_observability::OpsObservabilityStore,
    pub operator_review: services::operator_review::OperatorReviewStore,
    pub realm_bootstrap: services::realm_bootstrap::RealmBootstrapStore,
    pub room_progression: services::room_progression::RoomProgressionStore,
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
        ops_observability: services::ops_observability::OpsObservabilityStore::connect(config)
            .await?,
        operator_review: services::operator_review::OperatorReviewStore::connect(config).await?,
        realm_bootstrap: services::realm_bootstrap::RealmBootstrapStore::connect(config).await?,
        room_progression: services::room_progression::RoomProgressionStore::connect(config).await?,
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
        .ops_observability
        .reset_for_test()
        .await
        .map_err(|error| error.message().to_owned())?;
    state
        .happy_route
        .reset_for_test()
        .await
        .map_err(|error| error.message().to_owned())?;
    state
        .operator_review
        .reset_for_test()
        .await
        .map_err(|error| error.message().to_owned())?;
    state
        .realm_bootstrap
        .reset_for_test()
        .await
        .map_err(|error| error.message().to_owned())?;
    state
        .room_progression
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
        )
        .route(
            "/api/projection/settlement-views/{settlement_case_id}/expanded",
            get(handlers::projection::get_expanded_settlement_view),
        )
        .route(
            "/api/projection/promise-views/{promise_intent_id}",
            get(handlers::projection::get_promise_projection),
        )
        .route(
            "/api/projection/trust-snapshots/{account_id}",
            get(handlers::projection::get_trust_snapshot),
        )
        .route(
            "/api/projection/realm-trust-snapshots/{realm_id}/{account_id}",
            get(handlers::projection::get_realm_trust_snapshot),
        )
        .route(
            "/api/review-cases/{review_case_id}/appeals",
            post(handlers::operator_review::create_appeal_case)
                .get(handlers::operator_review::list_appeal_cases),
        )
        .route(
            "/api/review-cases/{review_case_id}/status",
            get(handlers::operator_review::get_review_status),
        )
        .route(
            "/api/realms/requests",
            post(handlers::realm_bootstrap::create_realm_request)
                .layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/realms/requests/{realm_request_id}",
            get(handlers::realm_bootstrap::get_realm_request),
        )
        .route(
            "/api/projection/room-progression-views/{room_progression_id}",
            get(handlers::room_progression::get_room_progression_view),
        )
        .route(
            "/api/projection/realms/{realm_id}/bootstrap-summary",
            get(handlers::realm_bootstrap::get_bootstrap_summary),
        )
        .route(
            "/api/internal/ops/health",
            get(handlers::ops_observability::get_ops_health),
        )
        .route(
            "/api/internal/ops/readiness",
            get(handlers::ops_observability::get_ops_readiness),
        )
        .route(
            "/api/internal/ops/observability/snapshot",
            get(handlers::ops_observability::get_ops_snapshot),
        )
        .route(
            "/api/internal/ops/observability/slo",
            get(handlers::ops_observability::get_ops_slo),
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
        .route(
            "/api/internal/projection/rebuild",
            post(handlers::projection::rebuild_projection_read_models),
        )
        .route(
            "/api/internal/operator/review-cases",
            post(handlers::operator_review::create_review_case)
                .get(handlers::operator_review::list_review_cases),
        )
        .route(
            "/api/internal/operator/review-cases/{review_case_id}",
            get(handlers::operator_review::read_review_case),
        )
        .route(
            "/api/internal/operator/review-cases/{review_case_id}/evidence-bundles",
            post(handlers::operator_review::attach_evidence_bundle),
        )
        .route(
            "/api/internal/operator/review-cases/{review_case_id}/evidence-access-grants",
            post(handlers::operator_review::grant_evidence_access),
        )
        .route(
            "/api/internal/operator/review-cases/{review_case_id}/decisions",
            post(handlers::operator_review::record_operator_decision),
        )
        .route(
            "/api/internal/operator/realms/requests",
            get(handlers::realm_bootstrap::list_realm_requests),
        )
        .route(
            "/api/internal/operator/realms/requests/{realm_request_id}",
            get(handlers::realm_bootstrap::read_realm_request),
        )
        .route(
            "/api/internal/operator/realms/requests/{realm_request_id}/approve",
            post(handlers::realm_bootstrap::approve_realm_request)
                .layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/internal/operator/realms/requests/{realm_request_id}/reject",
            post(handlers::realm_bootstrap::reject_realm_request)
                .layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/internal/realms/{realm_id}/sponsor-records",
            post(handlers::realm_bootstrap::create_realm_sponsor_record)
                .layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/internal/realms/{realm_id}/admissions",
            post(handlers::realm_bootstrap::create_realm_admission)
                .layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/internal/operator/realms/{realm_id}/review-summary",
            get(handlers::realm_bootstrap::get_review_summary),
        )
        .route(
            "/api/internal/projection/realms/rebuild",
            post(handlers::realm_bootstrap::rebuild_realm_bootstrap_views)
                .layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/internal/room-progressions",
            post(handlers::room_progression::create_room_progression),
        )
        .route(
            "/api/internal/room-progressions/{room_progression_id}/facts",
            post(handlers::room_progression::append_room_progression_fact),
        )
        .route(
            "/api/internal/projection/room-progressions/rebuild",
            post(handlers::room_progression::rebuild_room_progression_views),
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
