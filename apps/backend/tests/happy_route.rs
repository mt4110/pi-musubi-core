use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use musubi_backend::{
    build_app, new_test_state,
    services::happy_route::{self, HappyRouteError},
    start_background_outbox_worker,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn happy_route_flows_through_outbox_evidence_ledger_and_projection() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

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
    assert!(callback.body["raw_callback_id"].is_string());
    assert_eq!(callback.body["duplicate_callback"], false);
    assert_eq!(
        callback.body["outbox_event_ids"]
            .as_array()
            .expect("callback should enqueue provider callback ingestion")
            .len(),
        1
    );

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

    assert_writer_truth_tables(&settlement_case_id).await;
}

#[tokio::test]
async fn duplicate_receipt_is_idempotent_and_does_not_double_credit_projection() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
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
    assert_eq!(duplicate_callback.body["duplicate_callback"], false);

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
async fn exact_provider_callback_replay_keeps_the_existing_receipt() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;

    let payload = json!({
        "payment_id": prepared.payment_id,
        "payer_pi_uid": prepared.initiator_pi_uid,
        "amount_minor_units": 10000,
        "currency_code": "PI",
        "txid": "pi-tx-exact-replay",
        "status": "completed"
    });
    let first_callback = post_json(&app, "/api/payment/callback", None, payload.clone()).await;
    assert_eq!(first_callback.status, StatusCode::OK);
    assert_eq!(first_callback.body["duplicate_callback"], false);
    assert!(first_callback.body["raw_callback_id"].is_string());

    let replayed_callback = post_json(&app, "/api/payment/callback", None, payload).await;
    assert_eq!(replayed_callback.status, StatusCode::OK);
    assert_eq!(replayed_callback.body["duplicate_callback"], true);
    assert_ne!(
        replayed_callback.body["raw_callback_id"],
        first_callback.body["raw_callback_id"]
    );

    let drain_after_replay =
        post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain_after_replay.status, StatusCode::OK);
    let callback_messages = drain_after_replay.body["processed_messages"]
        .as_array()
        .expect("processed_messages must be an array")
        .iter()
        .filter(|message| message["event_type"] == "INGEST_PROVIDER_CALLBACK")
        .collect::<Vec<_>>();
    assert_eq!(callback_messages.len(), 2);
    assert!(callback_messages.iter().all(|message| {
        message["provider_submission_id"].as_str() == Some(prepared.payment_id.as_str())
    }));
    assert!(
        callback_messages
            .iter()
            .any(|message| { message["already_processed"].as_bool() == Some(true) })
    );

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
async fn drain_outbox_rejects_mismatched_command_inbox_payloads() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

    let initiator = sign_in(&app, "pi-user-mismatch-a", "mismatch-a").await;
    let counterparty = sign_in(&app, "pi-user-mismatch-b", "mismatch-b").await;

    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "promise-intent-mismatch",
            "realm_id": "realm-mismatch",
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
    let client = test_db_client().await;
    let row = client
        .query_one(
            "
            SELECT event_id, event_type, schema_version
            FROM outbox.events
            WHERE aggregate_id::text = $1
              AND event_type = 'OPEN_HOLD_INTENT'
            ",
            &[&settlement_case_id],
        )
        .await
        .expect("open hold event must exist");
    let event_id: Uuid = row.get("event_id");
    let event_type: String = row.get("event_type");
    let schema_version: i32 = row.get("schema_version");

    client
        .execute(
            "
            INSERT INTO outbox.command_inbox (
                inbox_entry_id,
                consumer_name,
                source_event_id,
                command_id,
                payload_checksum,
                status,
                command_type,
                schema_version,
                received_at,
                available_at
            )
            VALUES ($1, 'settlement-orchestrator', $2, $2, $3, 'processing', $4, $5, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ",
            &[&Uuid::new_v4(), &event_id, &"bogus-checksum", &event_type, &schema_version],
        )
        .await
        .expect("command inbox corruption fixture must insert");

    let error = happy_route::drain_outbox(&test_state.state)
        .await
        .expect_err("corrupt command inbox must fail");
    assert!(matches!(error, HappyRouteError::Internal(_)));
    assert_eq!(
        error.message(),
        "stored command inbox entry did not match the outbox payload"
    );
}

#[tokio::test]
async fn drain_outbox_reclaims_stale_processing_events() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

    let initiator = sign_in(&app, "pi-user-stale-a", "stale-a").await;
    let counterparty = sign_in(&app, "pi-user-stale-b", "stale-b").await;

    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "promise-intent-stale",
            "realm_id": "realm-stale",
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
    let client = test_db_client().await;
    let event_id: Uuid = client
        .query_one(
            "
            SELECT event_id
            FROM outbox.events
            WHERE aggregate_id::text = $1
              AND event_type = 'OPEN_HOLD_INTENT'
            ",
            &[&settlement_case_id],
        )
        .await
        .expect("open hold event must exist")
        .get("event_id");

    client
        .execute(
            "
            UPDATE outbox.events
            SET delivery_status = 'processing',
                claimed_by = 'dead-worker',
                claimed_until = CURRENT_TIMESTAMP - interval '1 minute',
                last_attempt_at = CURRENT_TIMESTAMP - interval '1 minute'
            WHERE event_id = $1
            ",
            &[&event_id],
        )
        .await
        .expect("stale processing fixture must update");

    let drain = post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain.status, StatusCode::OK);
    assert!(
        drain.body["processed_messages"]
            .as_array()
            .expect("processed_messages must be an array")
            .iter()
            .any(|message| {
                message["event_type"] == "OPEN_HOLD_INTENT"
                    && message["provider_submission_id"].is_string()
            })
    );
}

#[tokio::test]
async fn oversized_callback_amount_is_retained_as_raw_evidence() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;

    let callback = post_json(
        &app,
        "/api/payment/callback",
        None,
        json!({
            "payment_id": prepared.payment_id,
            "payer_pi_uid": prepared.initiator_pi_uid,
            "amount_minor_units": 9_223_372_036_854_775_808_i128,
            "currency_code": "PI",
            "txid": "pi-tx-oversized",
            "status": "completed"
        }),
    )
    .await;
    assert_eq!(callback.status, StatusCode::OK);
    let raw_callback_id = callback.body["raw_callback_id"]
        .as_str()
        .expect("raw_callback_id must exist")
        .to_owned();

    let drain_callback =
        post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain_callback.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(drain_callback.body["error"], "provider requires review");

    let client = test_db_client().await;
    let raw_row = client
        .query_one(
            "
            SELECT amount_minor_units, currency_code
            FROM core.raw_provider_callbacks
            WHERE raw_callback_id::text = $1
            ",
            &[&raw_callback_id],
        )
        .await
        .expect("raw callback evidence must exist");
    assert_eq!(raw_row.get::<_, Option<i64>>("amount_minor_units"), None);
    assert_eq!(
        raw_row.get::<_, Option<String>>("currency_code").as_deref(),
        Some("PI")
    );

    let event_row = client
        .query_one(
            "
            SELECT delivery_status
            FROM outbox.events
            WHERE aggregate_id::text = $1
              AND event_type = 'INGEST_PROVIDER_CALLBACK'
            ORDER BY causal_order DESC
            LIMIT 1
            ",
            &[&raw_callback_id],
        )
        .await
        .expect("callback outbox event must exist");
    assert_eq!(
        event_row.get::<_, String>("delivery_status"),
        "manual_review"
    );
}

#[tokio::test]
async fn background_outbox_worker_processes_open_hold_without_http_drain() {
    let test_state = new_test_state().await.expect("test database state");
    let state = test_state.state.clone();
    let worker = start_background_outbox_worker(state.clone());
    let app = build_app(state.clone());

    let initiator = sign_in(&app, "pi-user-worker-a", "worker-a").await;
    let counterparty = sign_in(&app, "pi-user-worker-b", "worker-b").await;

    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "promise-intent-worker",
            "realm_id": "realm-worker",
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
    let mut settlement_view_status = StatusCode::NOT_FOUND;
    for _ in 0..50 {
        let settlement_view = get_json(
            &app,
            &format!("/api/projection/settlement-views/{settlement_case_id}"),
            Some(initiator.token.as_str()),
        )
        .await;
        settlement_view_status = settlement_view.status;
        if settlement_view_status == StatusCode::OK {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    worker.abort();

    assert!(
        settlement_view_status == StatusCode::OK,
        "background outbox worker must build the settlement projection without HTTP drain"
    );
}

#[tokio::test]
async fn payment_callback_without_status_is_accepted_as_raw_evidence() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
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

    assert_eq!(callback.status, StatusCode::OK);
    assert!(callback.body["raw_callback_id"].is_string());
    assert_eq!(callback.body["duplicate_callback"], false);
    assert_eq!(
        callback.body["outbox_event_ids"]
            .as_array()
            .expect("callback should enqueue provider callback ingestion")
            .len(),
        1
    );
}

#[tokio::test]
async fn later_verified_callback_can_fund_after_initial_rejection() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
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
    assert_eq!(rejected_callback.body["duplicate_callback"], false);

    let drain_rejected =
        post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain_rejected.status, StatusCode::OK);

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
    assert_eq!(verified_callback.body["duplicate_callback"], false);

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
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

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
async fn re_authentication_rotates_the_prior_session_token() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let counterparty = sign_in(&app, "pi-user-session-b", "session-b").await;

    let first_sign_in = sign_in_with_access_token_response(
        &app,
        "pi-user-session-a",
        "session-a",
        "access-token-session",
    )
    .await;
    assert_eq!(first_sign_in.status, StatusCode::OK);
    let first_token = first_sign_in.body["token"]
        .as_str()
        .expect("token must exist")
        .to_owned();

    let second_sign_in = sign_in_with_access_token_response(
        &app,
        "pi-user-session-a",
        "session-a",
        "access-token-session",
    )
    .await;
    assert_eq!(second_sign_in.status, StatusCode::OK);
    let second_token = second_sign_in.body["token"]
        .as_str()
        .expect("token must exist")
        .to_owned();
    assert_ne!(first_token, second_token);

    let stale_session_request = post_json(
        &app,
        "/api/promise/intents",
        Some(first_token.as_str()),
        json!({
            "internal_idempotency_key": "stale-session-key",
            "realm_id": "realm-session",
            "counterparty_account_id": counterparty.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(stale_session_request.status, StatusCode::UNAUTHORIZED);

    let active_session_request = post_json(
        &app,
        "/api/promise/intents",
        Some(second_token.as_str()),
        json!({
            "internal_idempotency_key": "active-session-key",
            "realm_id": "realm-session",
            "counterparty_account_id": counterparty.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(active_session_request.status, StatusCode::OK);
}

#[tokio::test]
async fn settlement_projection_requires_authenticated_participant() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
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
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
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
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
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
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
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
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
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
async fn payment_callback_with_non_positive_amount_is_accepted_then_requires_review() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
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

    assert_eq!(callback.status, StatusCode::OK);
    assert!(callback.body["raw_callback_id"].is_string());

    let drain_callback =
        post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain_callback.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(drain_callback.body["error"], "provider requires review");
}

#[tokio::test]
async fn payment_callback_rejects_oversized_body() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let request = Request::builder()
        .method("POST")
        .uri("/api/payment/callback")
        .header("content-type", "application/json")
        .body(Body::from(vec![b'a'; 20_000]))
        .expect("request must build");

    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("app should respond");

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
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

async fn assert_writer_truth_tables(settlement_case_id: &str) {
    let client = test_db_client().await;
    let row = client
        .query_one(
            "
            SELECT
                (SELECT count(*) FROM dao.promise_intent_idempotency_keys) AS idempotency_count,
                (SELECT count(*) FROM dao.settlement_submissions WHERE provider_submission_id IS NOT NULL) AS submission_mapping_count,
                (SELECT count(*) FROM core.raw_provider_callbacks) AS raw_callback_count,
                (SELECT count(*) FROM core.raw_provider_callback_dedupe) AS raw_callback_dedupe_count,
                (SELECT count(*) FROM core.payment_receipts) AS receipt_count,
                (SELECT count(*) FROM ledger.journal_entries WHERE settlement_case_id::text = $1) AS journal_count,
                (SELECT count(*)
                   FROM ledger.account_postings posting
                   JOIN ledger.journal_entries journal
                     ON journal.journal_entry_id = posting.journal_entry_id
                  WHERE journal.settlement_case_id::text = $1) AS posting_count,
                (SELECT count(*) FROM projection.settlement_views WHERE settlement_case_id::text = $1) AS settlement_view_count
            ",
            &[&settlement_case_id],
        )
        .await
        .expect("writer truth counts must be queryable");

    assert_eq!(row.get::<_, i64>("idempotency_count"), 1);
    assert_eq!(row.get::<_, i64>("submission_mapping_count"), 1);
    assert_eq!(row.get::<_, i64>("raw_callback_count"), 1);
    assert_eq!(row.get::<_, i64>("raw_callback_dedupe_count"), 1);
    assert_eq!(row.get::<_, i64>("receipt_count"), 1);
    assert_eq!(row.get::<_, i64>("journal_count"), 1);
    assert_eq!(row.get::<_, i64>("posting_count"), 2);
    assert_eq!(row.get::<_, i64>("settlement_view_count"), 1);
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
