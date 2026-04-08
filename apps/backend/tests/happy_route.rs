use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use musubi_backend::{build_app, new_state};
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn happy_route_flows_through_outbox_evidence_ledger_and_projection() {
    let app = build_app(new_state());

    let initiator = sign_in(&app, "pi-user-a", "musubi-a").await;
    let counterparty = sign_in(&app, "pi-user-b", "musubi-b").await;

    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "promise-intent-001",
            "realm_id": "realm-001",
            "counterparty_account_id": counterparty.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(create_promise.status, StatusCode::OK);
    assert_eq!(create_promise.body["case_status"], "pending_funding");
    assert_eq!(create_promise.body["replayed_intent"], false);

    let settlement_case_id = create_promise.body["settlement_case_id"]
        .as_str()
        .expect("settlement_case_id must exist")
        .to_owned();

    let drain_outbox = post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain_outbox.status, StatusCode::OK);

    let payment_id = drain_outbox.body["processed_messages"]
        .as_array()
        .expect("processed_messages must be an array")
        .iter()
        .find(|message| message["event_type"] == "OPEN_HOLD_INTENT")
        .and_then(|message| message["provider_submission_id"].as_str())
        .expect("OPEN_HOLD_INTENT should yield a provider_submission_id")
        .to_owned();

    let pending_view = get_json(
        &app,
        &format!("/api/projection/settlement-views/{settlement_case_id}"),
        Some(initiator.token.as_str()),
    )
    .await;
    assert_eq!(pending_view.status, StatusCode::OK);
    assert_eq!(
        pending_view.body["current_settlement_status"],
        "pending_funding"
    );
    assert_eq!(pending_view.body["total_funded_minor_units"], 0);

    let callback = post_json(
        &app,
        "/api/payment/callback",
        None,
        json!({
            "payment_id": payment_id,
            "payer_pi_uid": initiator.pi_uid,
            "amount_minor_units": 10000,
            "currency_code": "PI",
            "txid": "pi-tx-001",
            "status": "completed"
        }),
    )
    .await;
    assert_eq!(callback.status, StatusCode::OK);
    assert_eq!(callback.body["receipt_status"], "verified");
    assert_eq!(callback.body["duplicate_receipt"], false);
    assert!(callback.body["ledger_journal_id"].is_string());
    assert_eq!(callback.body["case_status"], "funded");

    let second_drain = post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(second_drain.status, StatusCode::OK);

    let funded_view = get_json(
        &app,
        &format!("/api/projection/settlement-views/{settlement_case_id}"),
        Some(initiator.token.as_str()),
    )
    .await;
    assert_eq!(funded_view.status, StatusCode::OK);
    assert_eq!(funded_view.body["current_settlement_status"], "funded");
    assert_eq!(funded_view.body["total_funded_minor_units"], 10000);
    assert_eq!(funded_view.body["currency_code"], "PI");
    assert!(funded_view.body["latest_journal_entry_id"].is_string());
}

#[tokio::test]
async fn duplicate_callback_is_idempotent_and_does_not_double_credit_projection() {
    let app = build_app(new_state());
    let prepared = prepare_funded_case(&app).await;

    let duplicate_callback = post_json(
        &app,
        "/api/payment/callback",
        None,
        json!({
            "payment_id": prepared.payment_id,
            "payer_pi_uid": prepared.initiator_pi_uid,
            "amount_minor_units": 10000,
            "currency_code": "PI",
            "txid": "pi-tx-duplicate",
            "status": "completed"
        }),
    )
    .await;
    assert_eq!(duplicate_callback.status, StatusCode::OK);
    assert_eq!(duplicate_callback.body["duplicate_receipt"], true);

    let drain_after_duplicate =
        post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain_after_duplicate.status, StatusCode::OK);

    let settlement_view = get_json(
        &app,
        &format!(
            "/api/projection/settlement-views/{}",
            prepared.settlement_case_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(settlement_view.status, StatusCode::OK);
    assert_eq!(settlement_view.body["current_settlement_status"], "funded");
    assert_eq!(settlement_view.body["total_funded_minor_units"], 10000);
}

#[tokio::test]
async fn payment_callback_requires_explicit_status() {
    let app = build_app(new_state());
    let prepared = prepare_pending_case(&app).await;

    let callback = post_json(
        &app,
        "/api/payment/callback",
        None,
        json!({
            "payment_id": prepared.payment_id,
            "payer_pi_uid": prepared.initiator_pi_uid,
            "amount_minor_units": 10000,
            "currency_code": "PI",
            "txid": "pi-tx-missing-status"
        }),
    )
    .await;

    assert_eq!(callback.status, StatusCode::BAD_REQUEST);
    assert_eq!(callback.body["error"], "status is required");
}

#[tokio::test]
async fn later_verified_callback_can_fund_after_initial_rejection() {
    let app = build_app(new_state());
    let prepared = prepare_pending_case(&app).await;

    let rejected_callback = post_json(
        &app,
        "/api/payment/callback",
        None,
        json!({
            "payment_id": prepared.payment_id,
            "payer_pi_uid": prepared.initiator_pi_uid,
            "amount_minor_units": 10000,
            "currency_code": "PI",
            "txid": "pi-tx-rejected",
            "status": "failed"
        }),
    )
    .await;
    assert_eq!(rejected_callback.status, StatusCode::OK);
    assert_eq!(rejected_callback.body["receipt_status"], "rejected");
    assert_eq!(rejected_callback.body["duplicate_receipt"], false);
    assert_eq!(rejected_callback.body["case_status"], "pending_funding");

    let verified_callback = post_json(
        &app,
        "/api/payment/callback",
        None,
        json!({
            "payment_id": prepared.payment_id,
            "payer_pi_uid": prepared.initiator_pi_uid,
            "amount_minor_units": 10000,
            "currency_code": "PI",
            "txid": "pi-tx-verified",
            "status": "completed"
        }),
    )
    .await;
    assert_eq!(verified_callback.status, StatusCode::OK);
    assert_eq!(verified_callback.body["receipt_status"], "verified");
    assert_eq!(verified_callback.body["duplicate_receipt"], false);
    assert_eq!(verified_callback.body["case_status"], "funded");
    assert!(verified_callback.body["ledger_journal_id"].is_string());

    let drain_projection =
        post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain_projection.status, StatusCode::OK);

    let settlement_view = get_json(
        &app,
        &format!(
            "/api/projection/settlement-views/{}",
            prepared.settlement_case_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(settlement_view.status, StatusCode::OK);
    assert_eq!(settlement_view.body["current_settlement_status"], "funded");
    assert_eq!(settlement_view.body["total_funded_minor_units"], 10000);
}

#[tokio::test]
async fn authenticate_pi_requires_matching_access_token_for_existing_account() {
    let app = build_app(new_state());

    let first_sign_in = sign_in_with_access_token_response(
        &app,
        "pi-user-auth-reuse",
        "auth-reuse",
        "access-token-1",
    )
    .await;
    assert_eq!(first_sign_in.status, StatusCode::OK);

    let second_sign_in = sign_in_with_access_token_response(
        &app,
        "pi-user-auth-reuse",
        "auth-reuse",
        "access-token-2",
    )
    .await;
    assert_eq!(second_sign_in.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        second_sign_in.body["error"],
        "pi identity proof did not match the existing account"
    );
}

#[tokio::test]
async fn settlement_projection_requires_authenticated_participant() {
    let app = build_app(new_state());
    let prepared = prepare_funded_case(&app).await;
    let outsider = sign_in(&app, "pi-user-outsider", "outsider").await;

    let anonymous_view = get_json(
        &app,
        &format!(
            "/api/projection/settlement-views/{}",
            prepared.settlement_case_id
        ),
        None,
    )
    .await;
    assert_eq!(anonymous_view.status, StatusCode::UNAUTHORIZED);

    let outsider_view = get_json(
        &app,
        &format!(
            "/api/projection/settlement-views/{}",
            prepared.settlement_case_id
        ),
        Some(outsider.token.as_str()),
    )
    .await;
    assert_eq!(outsider_view.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn settlement_projection_accepts_lowercase_bearer_scheme() {
    let app = build_app(new_state());
    let prepared = prepare_funded_case(&app).await;

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/projection/settlement-views/{}",
            prepared.settlement_case_id
        ))
        .header(
            "authorization",
            format!("bearer {}", prepared.initiator_token),
        )
        .body(Body::empty())
        .expect("request must build");

    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("app should respond");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn promise_intent_rejects_blank_internal_idempotency_key() {
    let app = build_app(new_state());
    let initiator = sign_in(&app, "pi-user-empty-key-a", "empty-key-a").await;
    let counterparty = sign_in(&app, "pi-user-empty-key-b", "empty-key-b").await;

    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "   ",
            "realm_id": "realm-empty-key",
            "counterparty_account_id": counterparty.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;

    assert_eq!(create_promise.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        create_promise.body["error"],
        "internal_idempotency_key is required"
    );
}

#[tokio::test]
async fn promise_intent_idempotency_is_scoped_per_initiator() {
    let app = build_app(new_state());
    let initiator_a = sign_in(&app, "pi-user-scope-a", "scope-a").await;
    let initiator_b = sign_in(&app, "pi-user-scope-b", "scope-b").await;
    let counterparty = sign_in(&app, "pi-user-scope-c", "scope-c").await;

    let create_for_a = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator_a.token.as_str()),
        json!({
            "internal_idempotency_key": "shared-client-key",
            "realm_id": "realm-scope",
            "counterparty_account_id": counterparty.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(create_for_a.status, StatusCode::OK);
    assert_eq!(create_for_a.body["replayed_intent"], false);

    let create_for_b = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator_b.token.as_str()),
        json!({
            "internal_idempotency_key": "shared-client-key",
            "realm_id": "realm-scope",
            "counterparty_account_id": counterparty.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(create_for_b.status, StatusCode::OK);
    assert_eq!(create_for_b.body["replayed_intent"], false);
    assert_ne!(
        create_for_a.body["promise_intent_id"],
        create_for_b.body["promise_intent_id"]
    );
    assert_ne!(
        create_for_a.body["settlement_case_id"],
        create_for_b.body["settlement_case_id"]
    );
}

#[tokio::test]
async fn promise_intent_rejects_payload_drift_for_same_initiator_and_key() {
    let app = build_app(new_state());
    let initiator = sign_in(&app, "pi-user-drift-a", "drift-a").await;
    let counterparty_a = sign_in(&app, "pi-user-drift-b", "drift-b").await;
    let counterparty_b = sign_in(&app, "pi-user-drift-c", "drift-c").await;

    let first_create = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "drift-key",
            "realm_id": "realm-drift",
            "counterparty_account_id": counterparty_a.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(first_create.status, StatusCode::OK);

    let drifted_create = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "drift-key",
            "realm_id": "realm-drift",
            "counterparty_account_id": counterparty_b.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(drifted_create.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        drifted_create.body["error"],
        "internal_idempotency_key was already used with a different Promise payload"
    );
}

#[tokio::test]
async fn payment_callback_rejects_non_positive_amount_with_generic_message() {
    let app = build_app(new_state());
    let prepared = prepare_pending_case(&app).await;

    let callback = post_json(
        &app,
        "/api/payment/callback",
        None,
        json!({
            "payment_id": prepared.payment_id,
            "payer_pi_uid": prepared.initiator_pi_uid,
            "amount_minor_units": 0,
            "currency_code": "PI",
            "txid": "pi-tx-zero-amount",
            "status": "completed"
        }),
    )
    .await;

    assert_eq!(callback.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        callback.body["error"],
        "minor_units must be greater than zero"
    );
}

struct SignedInUser {
    token: String,
    account_id: String,
    pi_uid: String,
}

struct PreparedCase {
    settlement_case_id: String,
    payment_id: String,
    initiator_pi_uid: String,
    initiator_token: String,
}

async fn prepare_pending_case(app: &Router) -> PreparedCase {
    let initiator = sign_in(app, "pi-user-prepare-a", "prepare-a").await;
    let counterparty = sign_in(app, "pi-user-prepare-b", "prepare-b").await;

    let create_promise = post_json(
        app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "promise-intent-prepare",
            "realm_id": "realm-prepare",
            "counterparty_account_id": counterparty.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(create_promise.status, StatusCode::OK);

    let settlement_case_id = create_promise.body["settlement_case_id"]
        .as_str()
        .expect("settlement_case_id must exist")
        .to_owned();

    let drain_outbox = post_json(app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain_outbox.status, StatusCode::OK);

    let payment_id = drain_outbox.body["processed_messages"]
        .as_array()
        .expect("processed_messages must be an array")
        .iter()
        .find(|message| message["event_type"] == "OPEN_HOLD_INTENT")
        .and_then(|message| message["provider_submission_id"].as_str())
        .expect("OPEN_HOLD_INTENT should yield a provider_submission_id")
        .to_owned();

    PreparedCase {
        settlement_case_id,
        payment_id,
        initiator_pi_uid: initiator.pi_uid,
        initiator_token: initiator.token,
    }
}

async fn prepare_funded_case(app: &Router) -> PreparedCase {
    let prepared = prepare_pending_case(app).await;

    let callback = post_json(
        app,
        "/api/payment/callback",
        None,
        json!({
            "payment_id": prepared.payment_id,
            "payer_pi_uid": prepared.initiator_pi_uid,
            "amount_minor_units": 10000,
            "currency_code": "PI",
            "txid": "pi-tx-prepare",
            "status": "completed"
        }),
    )
    .await;
    assert_eq!(callback.status, StatusCode::OK);

    let drain_projection =
        post_json(app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain_projection.status, StatusCode::OK);

    prepared
}

async fn sign_in(app: &Router, pi_uid: &str, username: &str) -> SignedInUser {
    let response = sign_in_with_access_token_response(
        app,
        pi_uid,
        username,
        &format!("access-token-{pi_uid}"),
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
        pi_uid: response.body["user"]["pi_uid"]
            .as_str()
            .expect("pi_uid must exist")
            .to_owned(),
    }
}

async fn sign_in_with_access_token_response(
    app: &Router,
    pi_uid: &str,
    username: &str,
    access_token: &str,
) -> JsonResponse {
    post_json(
        app,
        "/api/auth/pi",
        None,
        json!({
            "pi_uid": pi_uid,
            "username": username,
            "wallet_address": format!("wallet-{pi_uid}"),
            "access_token": access_token
        }),
    )
    .await
}

struct JsonResponse {
    status: StatusCode,
    body: Value,
}

async fn post_json(
    app: &Router,
    path: &str,
    bearer_token: Option<&str>,
    body: Value,
) -> JsonResponse {
    request_json(app, "POST", path, bearer_token, Some(body)).await
}

async fn get_json(app: &Router, path: &str, bearer_token: Option<&str>) -> JsonResponse {
    request_json(app, "GET", path, bearer_token, None).await
}

async fn request_json(
    app: &Router,
    method: &str,
    path: &str,
    bearer_token: Option<&str>,
    body: Option<Value>,
) -> JsonResponse {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = bearer_token {
        builder = builder.header("authorization", format!("Bearer {token}"));
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
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body must be readable");
    let body = if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&bytes).expect("response body must be valid json")
    };

    JsonResponse { status, body }
}
