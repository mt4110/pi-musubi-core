use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use musubi_backend::{build_app, new_test_state};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TEST_RESPONSE_BODY_LIMIT: usize = 4 * 1024 * 1024;
const MIGRATION_LOCK_KEY: i64 = 411_000_008;

#[tokio::test]
async fn ops_health_returns_ok_when_db_available() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

    let response = get_json(&app, "/api/internal/ops/health", None).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["status"], "ok");
    assert_eq!(response.body["database"]["status"], "ok");
}

#[tokio::test]
async fn ops_readiness_reports_migration_status_without_mutation() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let before = migration_tracking_count(&client).await;

    let response = get_json(&app, "/api/internal/ops/readiness", None).await;

    let after = migration_tracking_count(&client).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["migrations"]["pending_count"], 0);
    assert_eq!(response.body["migrations"]["failed_count"], 0);
    assert_eq!(before, after);
}

#[tokio::test]
async fn ops_readiness_does_not_probe_migration_advisory_lock() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    client
        .execute("SELECT pg_advisory_lock($1)", &[&MIGRATION_LOCK_KEY])
        .await
        .expect("migration advisory lock must be held");

    let response = get_json(&app, "/api/internal/ops/readiness", None).await;

    client
        .query_one(
            "SELECT pg_advisory_unlock($1) AS unlocked",
            &[&MIGRATION_LOCK_KEY],
        )
        .await
        .expect("migration advisory lock must unlock");
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["status"], "ready");
    assert_eq!(
        response.body["migrations"]["migration_lock_available"],
        true
    );
}

#[tokio::test]
async fn ops_snapshot_requires_internal_gate() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

    let response = get_json(&app, "/api/ops/observability/snapshot", None).await;

    assert_eq!(response.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn participant_cannot_read_ops_snapshot() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let participant = sign_in(&app, "pi-user-ops-observability", "ops-observability").await;

    let response = get_json(
        &app,
        "/api/internal/ops/observability/snapshot",
        Some(participant.token.as_str()),
    )
    .await;

    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ops_endpoints_reject_participant_bearer_tokens() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let participant = sign_in(
        &app,
        "pi-user-ops-observability-all",
        "ops-observability-all",
    )
    .await;

    for path in [
        "/api/internal/ops/health",
        "/api/internal/ops/readiness",
        "/api/internal/ops/observability/snapshot",
        "/api/internal/ops/observability/slo",
    ] {
        let response = get_json(&app, path, Some(participant.token.as_str())).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[tokio::test]
async fn ops_snapshot_redacts_operator_notes_and_raw_evidence() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    create_review_case_with_private_fields(&app, &client, "redaction").await;

    let response = get_json(&app, "/api/internal/ops/observability/snapshot", None).await;

    assert_eq!(response.status, StatusCode::OK);
    let body = response.body.to_string();
    assert!(!body.contains("private operator note must not leak"));
    assert!(!body.contains("private-raw-callback-uri"));
    assert!(!body.contains("internal evidence summary detail"));
}

#[tokio::test]
async fn ops_snapshot_classifies_stale_projection_as_warning_without_rebuilding() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let review_case_id =
        create_review_case_with_private_fields(&app, &client, "projection-lag").await;
    let before_projected_at = force_stale_review_status_projection(&client, &review_case_id).await;

    let response = get_json(&app, "/api/internal/ops/observability/snapshot", None).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["status"], "warning");
    let review_status_metric = projection_metric(&response.body, "review_status_views");
    assert_eq!(review_status_metric["status"], "warning");
    let lag = review_status_metric["max_projection_lag_ms"]
        .as_i64()
        .expect("review_status_views lag must be reported");
    assert!(lag >= 60_000, "expected stale projection lag, got {lag}");
    assert!(
        lag < 1_800_000,
        "expected warning-level projection lag below critical threshold, got {lag}"
    );
    let after_projected_at = review_status_projected_at(&client, &review_case_id).await;
    assert_eq!(before_projected_at, after_projected_at);
}

#[tokio::test]
async fn ops_snapshot_does_not_drift_idle_projection_lag() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let review_case_id =
        create_review_case_with_private_fields(&app, &client, "idle-projection").await;
    force_idle_review_status_projection(&client, &review_case_id).await;

    let response = get_json(&app, "/api/internal/ops/observability/snapshot", None).await;

    assert_eq!(response.status, StatusCode::OK);
    let review_status_metric = projection_metric(&response.body, "review_status_views");
    assert_eq!(review_status_metric["status"], "ok");
    assert_eq!(review_status_metric["max_projection_lag_ms"], 0);
}

#[tokio::test]
async fn ops_snapshot_aggregates_warning_status() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let review_case_id =
        create_review_case_with_private_fields(&app, &client, "aggregate-warning").await;
    force_stale_review_status_projection(&client, &review_case_id).await;

    let response = get_json(&app, "/api/internal/ops/observability/snapshot", None).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["status"], "warning");
}

#[tokio::test]
async fn ops_snapshot_reports_operator_review_queue_summary() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    create_review_case_with_private_fields(&app, &client, "queue").await;

    let response = get_json(&app, "/api/internal/ops/observability/snapshot", None).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["operator_review_queue"]["status"], "ok");
    assert!(
        response.body["operator_review_queue"]["open_case_count"]
            .as_i64()
            .expect("open case count must be numeric")
            >= 1
    );
}

#[tokio::test]
async fn ops_snapshot_reports_realm_review_trigger_summary() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    insert_realm_review_trigger(&client, "realm-trigger-summary").await;

    let response = get_json(&app, "/api/internal/ops/observability/snapshot", None).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["realm_review_triggers"]["status"], "ok");
    assert!(
        response.body["realm_review_triggers"]["open_trigger_count"]
            .as_i64()
            .expect("open trigger count must be numeric")
            >= 1
    );
    assert_eq!(
        response.body["realm_review_triggers"]["latest_redacted_reason_code"],
        "bootstrap_capacity_reached"
    );
}

#[tokio::test]
async fn unsupported_sli_returns_unknown_not_zero() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

    let response = get_json(&app, "/api/internal/ops/observability/snapshot", None).await;

    assert_eq!(response.status, StatusCode::OK);
    let unsupported = response.body["unsupported_metrics"]
        .as_array()
        .expect("unsupported metrics must be an array")
        .iter()
        .find(|metric| metric["metric_name"] == "idempotency_replay_mismatch_count")
        .expect("idempotency mismatch metric must be present");
    assert_eq!(unsupported["status"], "unknown");
    assert!(unsupported["value"].is_null());
}

#[tokio::test]
async fn ops_snapshot_does_not_expose_source_fact_ids() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    create_review_case_with_private_fields(&app, &client, "source-id").await;

    let response = get_json(&app, "/api/internal/ops/observability/snapshot", None).await;

    assert_eq!(response.status, StatusCode::OK);
    let body = response.body.to_string();
    assert!(!body.contains("private-source-fact-id-source-id"));
    assert!(!body.contains("source_fact_id"));
    assert!(!body.contains("source_fact_count"));
}

#[tokio::test]
async fn ops_snapshot_get_is_side_effect_free() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    create_review_case_with_private_fields(&app, &client, "side-effect-free").await;
    insert_realm_review_trigger(&client, "side-effect-free").await;
    let before = side_effect_counts(&client).await;

    let first = get_json(&app, "/api/internal/ops/observability/snapshot", None).await;
    let second = get_json(&app, "/api/internal/ops/observability/snapshot", None).await;

    let after = side_effect_counts(&client).await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(before, after);
}

#[tokio::test]
async fn ops_snapshot_handles_empty_database() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

    let response = get_json(&app, "/api/internal/ops/observability/snapshot", None).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["status"], "ok");
    assert_eq!(response.body["operator_review_queue"]["status"], "ok");
    assert_eq!(response.body["operator_review_queue"]["open_case_count"], 0);
    assert_eq!(response.body["realm_review_triggers"]["status"], "ok");
    assert_eq!(
        response.body["realm_review_triggers"]["open_trigger_count"],
        0
    );
}

async fn create_review_case_with_private_fields(
    app: &Router,
    client: &tokio_postgres::Client,
    suffix: &str,
) -> String {
    let subject = sign_in(
        app,
        &format!("pi-user-ops-review-{suffix}"),
        &format!("ops-review-{suffix}"),
    )
    .await;
    let reviewer_id = insert_operator_account(client, "reviewer").await;
    let approver_id = insert_operator_account(client, "approver").await;

    let create_case = operator_post_json(
        app,
        "/api/internal/operator/review-cases",
        &reviewer_id,
        json!({
            "case_type": "safety_escalation",
            "severity": "sev2",
            "subject_account_id": subject.account_id,
            "related_realm_id": format!("realm-ops-{suffix}"),
            "opened_reason_code": "safety_review",
            "source_fact_kind": "ops_observability_fixture",
            "source_fact_id": format!("private-source-fact-id-{suffix}"),
            "source_snapshot_json": {
                "raw_source_identifier": format!("private-source-fact-id-{suffix}"),
                "operator_note": "source snapshot must not leak"
            },
            "request_idempotency_key": format!("ops-review-case-{suffix}")
        }),
    )
    .await;
    assert_eq!(create_case.status, StatusCode::OK);
    let review_case_id = create_case.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let evidence = operator_post_json(
        app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/evidence-bundles"),
        &reviewer_id,
        json!({
            "evidence_visibility": "redacted_raw",
            "summary_json": {
                "operator_summary": "internal evidence summary detail"
            },
            "raw_locator_json": {
                "raw_callback_locator": "private-raw-callback-uri"
            },
            "retention_class": "R6"
        }),
    )
    .await;
    assert_eq!(evidence.status, StatusCode::OK);

    let decision = operator_post_json(
        app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "request_more_evidence",
            "user_facing_reason_code": "proof_inconclusive",
            "operator_note_internal": "private operator note must not leak",
            "decision_payload_json": {
                "internal_safety_classification": "must stay internal"
            },
            "decision_idempotency_key": format!("ops-review-decision-{suffix}")
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);

    review_case_id
}

async fn insert_realm_review_trigger(client: &tokio_postgres::Client, suffix: &str) {
    client
        .execute(
            "
            INSERT INTO dao.realm_review_triggers (
                realm_review_trigger_id,
                trigger_kind,
                trigger_state,
                redacted_reason_code,
                context_json,
                trigger_fingerprint
            )
            VALUES (
                $1,
                'corridor_cap_pressure',
                'open',
                'bootstrap_capacity_reached',
                $2,
                $3
            )
            ",
            &[
                &Uuid::new_v4(),
                &json!({
                    "internal_review_note": "realm trigger context must not leak",
                    "raw_overlap": "private overlap detail"
                }),
                &format!("ops-observability-trigger-{suffix}"),
            ],
        )
        .await
        .expect("realm review trigger must insert");
}

async fn force_stale_review_status_projection(
    client: &tokio_postgres::Client,
    review_case_id: &str,
) -> String {
    client
        .query_one(
            "
            UPDATE projection.review_status_views
            SET source_watermark_at = CURRENT_TIMESTAMP - INTERVAL '2 minutes',
                last_projected_at = CURRENT_TIMESTAMP
            WHERE review_case_id::text = $1
            RETURNING last_projected_at::text AS last_projected_at
            ",
            &[&review_case_id],
        )
        .await
        .expect("review status projection must update")
        .get("last_projected_at")
}

async fn force_idle_review_status_projection(
    client: &tokio_postgres::Client,
    review_case_id: &str,
) {
    client
        .execute(
            "
            UPDATE projection.review_status_views
            SET source_watermark_at = CURRENT_TIMESTAMP - INTERVAL '2 hours',
                last_projected_at = CURRENT_TIMESTAMP - INTERVAL '2 hours'
            WHERE review_case_id::text = $1
            ",
            &[&review_case_id],
        )
        .await
        .expect("review status projection must update");
}

async fn review_status_projected_at(
    client: &tokio_postgres::Client,
    review_case_id: &str,
) -> String {
    client
        .query_one(
            "
            SELECT last_projected_at::text AS last_projected_at
            FROM projection.review_status_views
            WHERE review_case_id::text = $1
            ",
            &[&review_case_id],
        )
        .await
        .expect("review status projection must be readable")
        .get("last_projected_at")
}

fn projection_metric<'a>(body: &'a Value, projection_name: &str) -> &'a Value {
    body["projection_lag"]
        .as_array()
        .expect("projection_lag must be an array")
        .iter()
        .find(|metric| metric["projection_name"] == projection_name)
        .expect("projection metric must exist")
}

async fn migration_tracking_count(client: &tokio_postgres::Client) -> i64 {
    client
        .query_one(
            "SELECT COUNT(*)::bigint AS count FROM public.musubi_schema_migrations",
            &[],
        )
        .await
        .expect("migration tracking table must be readable")
        .get("count")
}

#[derive(Debug, PartialEq, Eq)]
struct SideEffectCounts {
    review_cases: i64,
    evidence_bundles: i64,
    operator_decision_facts: i64,
    review_status_views: i64,
    realm_review_triggers: i64,
    outbox_events: i64,
    command_inbox: i64,
}

async fn side_effect_counts(client: &tokio_postgres::Client) -> SideEffectCounts {
    let row = client
        .query_one(
            "
            SELECT
                (SELECT COUNT(*)::bigint FROM dao.review_cases) AS review_cases,
                (SELECT COUNT(*)::bigint FROM dao.evidence_bundles) AS evidence_bundles,
                (SELECT COUNT(*)::bigint FROM dao.operator_decision_facts)
                    AS operator_decision_facts,
                (SELECT COUNT(*)::bigint FROM projection.review_status_views)
                    AS review_status_views,
                (SELECT COUNT(*)::bigint FROM dao.realm_review_triggers)
                    AS realm_review_triggers,
                (SELECT COUNT(*)::bigint FROM outbox.events) AS outbox_events,
                (SELECT COUNT(*)::bigint FROM outbox.command_inbox) AS command_inbox
            ",
            &[],
        )
        .await
        .expect("side-effect counts must be readable");

    SideEffectCounts {
        review_cases: row.get("review_cases"),
        evidence_bundles: row.get("evidence_bundles"),
        operator_decision_facts: row.get("operator_decision_facts"),
        review_status_views: row.get("review_status_views"),
        realm_review_triggers: row.get("realm_review_triggers"),
        outbox_events: row.get("outbox_events"),
        command_inbox: row.get("command_inbox"),
    }
}

struct SignedInUser {
    token: String,
    account_id: String,
}

struct JsonResponse {
    status: StatusCode,
    body: Value,
}

async fn sign_in(app: &Router, pi_uid: &str, username: &str) -> SignedInUser {
    let response = post_json(
        app,
        "/api/auth/pi",
        None,
        json!({
            "pi_uid": pi_uid,
            "username": username,
            "wallet_address": format!("wallet-{pi_uid}"),
            "access_token": format!("access-token-{pi_uid}")
        }),
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);

    SignedInUser {
        token: response.body["token"]
            .as_str()
            .expect("token must exist")
            .to_owned(),
        account_id: response.body["user"]["id"]
            .as_str()
            .expect("user id must exist")
            .to_owned(),
    }
}

async fn insert_operator_account(client: &tokio_postgres::Client, role: &str) -> String {
    let account_id = Uuid::new_v4();
    client
        .execute(
            "
            INSERT INTO core.accounts (account_id, account_class, account_state)
            VALUES ($1, 'Controlled Exceptional Account', 'active')
            ",
            &[&account_id],
        )
        .await
        .expect("operator account must insert");
    client
        .execute(
            "
            INSERT INTO core.operator_role_assignments (
                operator_role_assignment_id,
                operator_account_id,
                operator_role,
                grant_reason
            )
            VALUES ($1, $2, $3, 'ops observability test role')
            ",
            &[&Uuid::new_v4(), &account_id, &role],
        )
        .await
        .expect("operator role assignment must insert");
    account_id.to_string()
}

async fn test_db_client() -> tokio_postgres::Client {
    let database_url = std::env::var("MUSUBI_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("test database url must be present");
    let (client, connection) = tokio_postgres::connect(&database_url, tokio_postgres::NoTls)
        .await
        .expect("test database must be reachable");
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("test database connection error: {error}");
        }
    });
    client
}

async fn operator_post_json(
    app: &Router,
    path: &str,
    operator_id: &str,
    body: Value,
) -> JsonResponse {
    request_json(app, "POST", path, None, Some(operator_id), Some(body)).await
}

async fn post_json(
    app: &Router,
    path: &str,
    bearer_token: Option<&str>,
    body: Value,
) -> JsonResponse {
    request_json(app, "POST", path, bearer_token, None, Some(body)).await
}

async fn get_json(app: &Router, path: &str, bearer_token: Option<&str>) -> JsonResponse {
    request_json(app, "GET", path, bearer_token, None, None).await
}

async fn request_json(
    app: &Router,
    method: &str,
    path: &str,
    bearer_token: Option<&str>,
    operator_id: Option<&str>,
    body: Option<Value>,
) -> JsonResponse {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = bearer_token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    if let Some(operator_id) = operator_id {
        builder = builder.header("x-musubi-operator-id", operator_id);
    }

    let request = builder
        .header("content-type", "application/json")
        .body(match body {
            Some(body) => Body::from(body.to_string()),
            None => Body::empty(),
        })
        .expect("request must build");

    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("app should respond");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), TEST_RESPONSE_BODY_LIMIT)
        .await
        .expect("body must be readable");
    let body = if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            json!({
                "raw_body": String::from_utf8_lossy(&bytes).to_string()
            })
        })
    };

    JsonResponse { status, body }
}
