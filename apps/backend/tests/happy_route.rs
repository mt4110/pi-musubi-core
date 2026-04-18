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
use sha2::{Digest, Sha256};
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
async fn concurrent_payment_receipt_insert_replays_race_winner() {
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
            "txid": "pi-tx-receipt-race",
            "status": "completed"
        }),
    )
    .await;
    assert_eq!(callback.status, StatusCode::OK);
    let raw_callback_id = callback.body["raw_callback_id"]
        .as_str()
        .expect("raw_callback_id must exist")
        .to_owned();

    let client = test_db_client().await;
    let promise_intent_id: Uuid = client
        .query_one(
            "
            SELECT promise_intent_id
            FROM dao.settlement_cases
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("settlement case must expose promise intent id")
        .get("promise_intent_id");
    let raw_callback_uuid =
        Uuid::parse_str(&raw_callback_id).expect("raw callback id must parse as uuid");
    let blocked_receipt_id = Uuid::new_v4();
    let mut blocker_client = test_db_client().await;
    let blocker_tx = blocker_client
        .transaction()
        .await
        .expect("blocker transaction must start");
    blocker_tx
        .execute(
            "
            INSERT INTO core.payment_receipts (
                payment_receipt_id,
                provider_key,
                external_payment_id,
                settlement_case_id,
                promise_intent_id,
                amount_minor_units,
                currency_code,
                amount_scale,
                receipt_status,
                raw_callback_id
            )
            VALUES ($1, 'pi', $2, $3, $4, 10000, 'PI', 3, 'verified', $5)
            ",
            &[
                &blocked_receipt_id,
                &prepared.payment_id,
                &Uuid::parse_str(&prepared.settlement_case_id)
                    .expect("settlement case id must parse as uuid"),
                &promise_intent_id,
                &raw_callback_uuid,
            ],
        )
        .await
        .expect("blocked payment receipt must insert");
    let blocker_pid: i32 = blocker_tx
        .query_one("SELECT pg_backend_pid() AS pid", &[])
        .await
        .expect("blocker backend pid must be queryable")
        .get("pid");

    let app_clone = app.clone();
    let request = tokio::spawn(async move {
        post_json(
            &app_clone,
            "/api/internal/orchestration/drain",
            None,
            json!({}),
        )
        .await
    });
    wait_for_backend_lock_contention(blocker_pid).await;
    blocker_tx
        .commit()
        .await
        .expect("blocker transaction must commit");

    let response = request.await.expect("drain request must join");
    assert_eq!(response.status, StatusCode::OK);
    assert!(
        response.body["processed_messages"]
            .as_array()
            .expect("processed_messages must be an array")
            .iter()
            .any(|message| {
                message["event_type"] == "INGEST_PROVIDER_CALLBACK"
                    && message["provider_submission_id"].as_str()
                        == Some(prepared.payment_id.as_str())
            })
    );

    let receipt_count: i64 = client
        .query_one(
            "
            SELECT count(*) AS count
            FROM core.payment_receipts
            WHERE provider_key = 'pi'
              AND external_payment_id = $1
            ",
            &[&prepared.payment_id],
        )
        .await
        .expect("payment receipt count must be queryable")
        .get("count");
    assert_eq!(receipt_count, 1);
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
    let replayed_raw_callback_id = replayed_callback.body["raw_callback_id"]
        .as_str()
        .expect("replayed raw_callback_id must exist")
        .to_owned();
    let client = test_db_client().await;
    let replay_row = client
        .query_one(
            "
            SELECT event_id, payload_hash, event_type, schema_version
            FROM outbox.events
            WHERE aggregate_id::text = $1
              AND event_type = 'INGEST_PROVIDER_CALLBACK'
            ",
            &[&replayed_raw_callback_id],
        )
        .await
        .expect("replayed callback event must exist");
    let replay_event_id: Uuid = replay_row.get("event_id");
    let replay_payload_hash: String = replay_row.get("payload_hash");
    let replay_event_type: String = replay_row.get("event_type");
    let replay_schema_version: i32 = replay_row.get("schema_version");
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
            VALUES (
                $1,
                'provider-callback-consumer',
                $2,
                $2,
                $3,
                'pending',
                $4,
                $5,
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP
            )
            ",
            &[
                &Uuid::new_v4(),
                &replay_event_id,
                &replay_payload_hash,
                &replay_event_type,
                &replay_schema_version,
            ],
        )
        .await
        .expect("replayed callback command fixture must insert");

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
    let replay_command = client
        .query_one(
            "
            SELECT status, claimed_by, claimed_until
            FROM outbox.command_inbox
            WHERE consumer_name = 'provider-callback-consumer'
              AND command_id = $1
            ",
            &[&replay_event_id],
        )
        .await
        .expect("replayed callback command row must exist");
    assert_eq!(replay_command.get::<_, String>("status"), "completed");
    assert_eq!(replay_command.get::<_, Option<String>>("claimed_by"), None);
    assert_eq!(
        replay_command.get::<_, Option<chrono::DateTime<chrono::Utc>>>("claimed_until"),
        None
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
    let stored = client
        .query_one(
            "
            SELECT
                (SELECT delivery_status FROM outbox.events WHERE event_id = $1) AS delivery_status,
                (SELECT status FROM outbox.command_inbox WHERE consumer_name = 'settlement-orchestrator' AND command_id = $1) AS inbox_status
            ",
            &[&event_id],
        )
        .await
        .expect("terminalized corruption rows must remain queryable");
    assert_eq!(stored.get::<_, String>("delivery_status"), "quarantined");
    assert_eq!(stored.get::<_, String>("inbox_status"), "quarantined");
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
async fn drain_outbox_quarantines_invalid_payloads_and_continues() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let invalid_event_id = Uuid::new_v4();
    let invalid_idempotency_key = Uuid::new_v4();
    let invalid_aggregate_id = Uuid::new_v4();
    let invalid_payload = json!({ "wrong_field": "missing settlement_case_id" });
    let invalid_payload_hash = sha256_hex_test(invalid_payload.to_string().as_bytes());
    let invalid_stream_key = format!("settlement_case:{invalid_aggregate_id}");

    client
        .execute(
            "
            INSERT INTO outbox.events (
                event_id,
                idempotency_key,
                aggregate_type,
                aggregate_id,
                event_type,
                schema_version,
                payload_json,
                payload_hash,
                stream_key,
                delivery_status
            )
            VALUES ($1, $2, 'settlement_case', $3, 'OPEN_HOLD_INTENT', 1, $4, $5, $6, 'pending')
            ",
            &[
                &invalid_event_id,
                &invalid_idempotency_key,
                &invalid_aggregate_id,
                &invalid_payload,
                &invalid_payload_hash,
                &invalid_stream_key,
            ],
        )
        .await
        .expect("invalid outbox payload fixture must insert");

    let initiator = sign_in(&app, "pi-user-invalid-outbox-a", "invalid-outbox-a").await;
    let counterparty = sign_in(&app, "pi-user-invalid-outbox-b", "invalid-outbox-b").await;
    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "promise-intent-invalid-outbox",
            "realm_id": "realm-invalid-outbox",
            "counterparty_account_id": counterparty.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(create_promise.status, StatusCode::OK);

    let outcome = happy_route::drain_outbox(&test_state.state)
        .await
        .expect("invalid outbox payload must quarantine instead of failing");
    assert!(
        outcome
            .processed_messages
            .iter()
            .any(|message| message.event_type == "OPEN_HOLD_INTENT")
    );

    let row = client
        .query_one(
            "
            SELECT
                delivery_status,
                last_error_class,
                last_error_detail,
                claimed_by,
                claimed_until,
                retain_until
            FROM outbox.events
            WHERE event_id = $1
            ",
            &[&invalid_event_id],
        )
        .await
        .expect("invalid outbox row must remain queryable");
    assert_eq!(row.get::<_, String>("delivery_status"), "quarantined");
    assert_eq!(
        row.get::<_, Option<String>>("last_error_class"),
        Some("permanent".to_owned())
    );
    assert_eq!(
        row.get::<_, Option<String>>("last_error_detail"),
        Some("outbox payload for OPEN_HOLD_INTENT is missing settlement_case_id".to_owned())
    );
    assert_eq!(row.get::<_, Option<String>>("claimed_by"), None);
    assert_eq!(
        row.get::<_, Option<chrono::DateTime<chrono::Utc>>>("claimed_until"),
        None
    );
    assert!(
        row.get::<_, Option<chrono::DateTime<chrono::Utc>>>("retain_until")
            .is_some()
    );

    client
        .execute(
            "
            UPDATE outbox.events
            SET retain_until = CURRENT_TIMESTAMP - interval '1 minute'
            WHERE event_id = $1
            ",
            &[&invalid_event_id],
        )
        .await
        .expect("quarantined event must allow prune-fixture update");

    let second_outcome = happy_route::drain_outbox(&test_state.state)
        .await
        .expect("expired quarantined row must prune cleanly");
    assert!(second_outcome.processed_messages.is_empty());

    let pruned_count: i64 = client
        .query_one(
            "SELECT count(*) AS count FROM outbox.events WHERE event_id = $1",
            &[&invalid_event_id],
        )
        .await
        .expect("invalid outbox row count must be queryable")
        .get("count");
    assert_eq!(pruned_count, 0);
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
            "amount_minor_units": 9_223_372_036_854_775_808_u64,
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
            SELECT event_id, delivery_status
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
    let callback_event_id: Uuid = event_row.get("event_id");
    assert_eq!(
        event_row.get::<_, String>("delivery_status"),
        "manual_review"
    );

    let command_row = client
        .query_one(
            "
            SELECT status, claimed_by, claimed_until, retain_until
            FROM outbox.command_inbox
            WHERE consumer_name = 'provider-callback-consumer'
              AND command_id = $1
            ",
            &[&callback_event_id],
        )
        .await
        .expect("callback command inbox row must exist");
    assert_eq!(command_row.get::<_, String>("status"), "quarantined");
    assert_eq!(command_row.get::<_, Option<String>>("claimed_by"), None);
    assert_eq!(
        command_row.get::<_, Option<chrono::DateTime<chrono::Utc>>>("claimed_until"),
        None
    );
    assert!(
        command_row
            .get::<_, Option<chrono::DateTime<chrono::Utc>>>("retain_until")
            .is_some()
    );

    client
        .execute(
            "
            UPDATE outbox.command_inbox
            SET retain_until = CURRENT_TIMESTAMP - interval '1 minute'
            WHERE consumer_name = 'provider-callback-consumer'
              AND command_id = $1
            ",
            &[&callback_event_id],
        )
        .await
        .expect("expired terminal command row must be updateable");

    let prune = happy_route::drain_outbox(&test_state.state)
        .await
        .expect("expired terminal command row must prune cleanly");
    assert!(prune.processed_messages.is_empty());

    let pruned_count: i64 = client
        .query_one(
            "
            SELECT count(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = 'provider-callback-consumer'
              AND command_id = $1
            ",
            &[&callback_event_id],
        )
        .await
        .expect("terminal command row count must be queryable")
        .get("count");
    assert_eq!(pruned_count, 0);
}

#[tokio::test]
async fn drain_outbox_prunes_expired_published_events() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

    let initiator = sign_in(&app, "pi-user-prune-a", "prune-a").await;
    let counterparty = sign_in(&app, "pi-user-prune-b", "prune-b").await;

    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "promise-intent-prune",
            "realm_id": "realm-prune",
            "counterparty_account_id": counterparty.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(create_promise.status, StatusCode::OK);

    let client = test_db_client().await;
    client
        .execute(
            "
            UPDATE outbox.events
            SET delivery_status = 'published',
                retain_until = CURRENT_TIMESTAMP - interval '1 minute'
            ",
            &[],
        )
        .await
        .expect("published prune fixture must update");

    let before = client
        .query_one("SELECT count(*) AS count FROM outbox.events", &[])
        .await
        .expect("outbox count before prune")
        .get::<_, i64>("count");
    assert!(before > 0);

    let outcome = happy_route::drain_outbox(&test_state.state)
        .await
        .expect("drain should prune expired published rows");
    assert!(outcome.processed_messages.is_empty());

    let after = client
        .query_one("SELECT count(*) AS count FROM outbox.events", &[])
        .await
        .expect("outbox count after prune")
        .get::<_, i64>("count");
    assert_eq!(after, 0);
}

#[tokio::test]
async fn completed_command_inbox_rows_clear_claim_metadata() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;

    let row = client
        .query_one(
            "
            SELECT inbox.status, inbox.claimed_by, inbox.claimed_until
            FROM outbox.command_inbox inbox
            JOIN outbox.events events ON events.event_id = inbox.command_id
            WHERE inbox.consumer_name = 'settlement-orchestrator'
              AND events.aggregate_id::text = $1
              AND events.event_type = 'OPEN_HOLD_INTENT'
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("completed open-hold command row must exist");
    assert_eq!(row.get::<_, String>("status"), "completed");
    assert_eq!(row.get::<_, Option<String>>("claimed_by"), None);
    assert_eq!(
        row.get::<_, Option<chrono::DateTime<chrono::Utc>>>("claimed_until"),
        None
    );
}

#[tokio::test]
async fn completed_provider_callback_command_replays_without_new_observations() {
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
            "txid": "pi-tx-completed-replay",
            "status": "completed"
        }),
    )
    .await;
    assert_eq!(callback.status, StatusCode::OK);
    let raw_callback_id = callback.body["raw_callback_id"]
        .as_str()
        .expect("raw_callback_id must exist")
        .to_owned();
    let event_id = callback.body["outbox_event_ids"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|value| value.as_str())
        .expect("callback outbox event id must exist")
        .to_owned();

    let client = test_db_client().await;
    let observation_count_before = client
        .query_one(
            "
            SELECT count(*) AS count
            FROM dao.settlement_observations
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("baseline observation count")
        .get::<_, i64>("count");

    let case_row = client
        .query_one(
            "
            SELECT promise_intent_id
            FROM dao.settlement_cases
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("settlement case must exist");
    let promise_intent_id: Uuid = case_row.get("promise_intent_id");
    let event_row = client
        .query_one(
            "
            SELECT
                event_type,
                schema_version,
                payload_hash
            FROM outbox.events
            WHERE event_id::text = $1
            ",
            &[&event_id],
        )
        .await
        .expect("callback outbox event must exist");
    let event_uuid = Uuid::parse_str(&event_id).expect("event id must be uuid");
    let raw_callback_uuid =
        Uuid::parse_str(&raw_callback_id).expect("raw callback id must be uuid");
    let payment_receipt_id = Uuid::new_v4();
    let payload_checksum: String = event_row.get("payload_hash");
    let event_type: String = event_row.get("event_type");
    let schema_version: i32 = event_row.get("schema_version");

    client
        .execute(
            "
            INSERT INTO core.payment_receipts (
                payment_receipt_id,
                provider_key,
                external_payment_id,
                settlement_case_id,
                promise_intent_id,
                amount_minor_units,
                currency_code,
                amount_scale,
                receipt_status,
                raw_callback_id
            )
            VALUES ($1, 'pi', $2, $3, $4, 10000, 'PI', 3, 'verified', $5)
            ",
            &[
                &payment_receipt_id,
                &prepared.payment_id,
                &Uuid::parse_str(&prepared.settlement_case_id)
                    .expect("settlement case id must be uuid"),
                &promise_intent_id,
                &raw_callback_uuid,
            ],
        )
        .await
        .expect("receipt replay fixture must insert");
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
                available_at,
                processed_at,
                completed_at
            )
            VALUES (
                $1,
                'provider-callback-consumer',
                $2,
                $2,
                $3,
                'completed',
                $4,
                $5,
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP
            )
            ",
            &[
                &Uuid::new_v4(),
                &event_uuid,
                &payload_checksum,
                &event_type,
                &schema_version,
            ],
        )
        .await
        .expect("completed command fixture must insert");

    let drain = post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain.status, StatusCode::OK);
    assert!(
        drain.body["processed_messages"]
            .as_array()
            .expect("processed_messages must be an array")
            .iter()
            .any(|message| {
                message["event_id"].as_str() == Some(event_id.as_str())
                    && message["event_type"] == "INGEST_PROVIDER_CALLBACK"
                    && message["already_processed"].as_bool() == Some(true)
            })
    );

    let observation_count_after = client
        .query_one(
            "
            SELECT count(*) AS count
            FROM dao.settlement_observations
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("observation count after replay")
        .get::<_, i64>("count");
    assert_eq!(observation_count_after, observation_count_before);
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
async fn first_time_sign_in_replays_race_winner_instead_of_500() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let pi_uid = "pi-user-auth-race";
    let username = "auth-race";
    let access_token = "access-token-auth-race";
    let blocked_account_id = Uuid::new_v4();
    let access_token_digest = sha256_hex_test(access_token.as_bytes());
    let mut blocker_client = test_db_client().await;
    let blocker_tx = blocker_client
        .transaction()
        .await
        .expect("blocker transaction must start");

    blocker_tx
        .execute(
            "
            INSERT INTO core.accounts (account_id, account_class, account_state)
            VALUES ($1, 'Ordinary Account', 'active')
            ",
            &[&blocked_account_id],
        )
        .await
        .expect("blocked account must insert");
    blocker_tx
        .execute(
            "
            INSERT INTO core.pi_account_links (
                account_id,
                pi_uid,
                username,
                wallet_address,
                access_token_digest
            )
            VALUES ($1, $2, $3, $4, $5)
            ",
            &[
                &blocked_account_id,
                &pi_uid,
                &username,
                &Some(format!("wallet-{pi_uid}")),
                &access_token_digest,
            ],
        )
        .await
        .expect("blocked pi link must insert");
    let blocker_pid: i32 = blocker_tx
        .query_one("SELECT pg_backend_pid() AS pid", &[])
        .await
        .expect("blocker backend pid must be queryable")
        .get("pid");

    let app_clone = app.clone();
    let request = tokio::spawn(async move {
        sign_in_with_access_token_response(&app_clone, pi_uid, username, access_token).await
    });
    wait_for_backend_lock_contention(blocker_pid).await;
    blocker_tx
        .commit()
        .await
        .expect("blocker transaction must commit");

    let response = request.await.expect("request task must join");
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.body["user"]["id"].as_str(),
        Some(blocked_account_id.to_string().as_str())
    );

    let client = test_db_client().await;
    let row = client
        .query_one(
            "
            SELECT
                (SELECT count(*) FROM core.accounts) AS account_count,
                (SELECT count(*) FROM core.pi_account_links WHERE pi_uid = $1) AS link_count
            ",
            &[&pi_uid],
        )
        .await
        .expect("auth race counts must be queryable");
    assert_eq!(row.get::<_, i64>("account_count"), 1);
    assert_eq!(row.get::<_, i64>("link_count"), 1);
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
async fn settlement_projection_keeps_invalid_ids_as_not_found() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_funded_case(&app).await;

    let invalid_view = get_json(
        &app,
        "/api/projection/settlement-views/not-a-uuid",
        Some(prepared.initiator_token.as_str()),
    )
    .await;

    assert_eq!(invalid_view.status, StatusCode::NOT_FOUND);
    assert_eq!(
        invalid_view.body["error"],
        "settlement projection has not been built for that settlement_case_id"
    );
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
async fn promise_intent_replays_race_winner_instead_of_500() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let initiator = sign_in(&app, "pi-user-promise-race-a", "promise-race-a").await;
    let counterparty = sign_in(&app, "pi-user-promise-race-b", "promise-race-b").await;
    let internal_idempotency_key = "promise-race-key";
    let realm_id = "realm-promise-race";
    let blocked_promise_intent_id = Uuid::new_v4();
    let blocked_settlement_case_id = Uuid::new_v4();
    let request_hash =
        promise_request_hash_test(realm_id, &counterparty.account_id, 10000, "PI", 3);
    let mut blocker_client = test_db_client().await;
    let blocker_tx = blocker_client
        .transaction()
        .await
        .expect("blocker transaction must start");

    blocker_tx
        .execute(
            "
            INSERT INTO dao.promise_intents (
                promise_intent_id,
                realm_id,
                initiator_account_id,
                counterparty_account_id,
                intent_status,
                deposit_amount_minor_units,
                deposit_currency_code,
                deposit_scale
            )
            VALUES ($1, $2, $3, $4, 'proposed', 10000, 'PI', 3)
            ",
            &[
                &blocked_promise_intent_id,
                &realm_id,
                &Uuid::parse_str(&initiator.account_id).expect("initiator account id must be uuid"),
                &Uuid::parse_str(&counterparty.account_id)
                    .expect("counterparty account id must be uuid"),
            ],
        )
        .await
        .expect("blocked promise intent must insert");
    blocker_tx
        .execute(
            "
            INSERT INTO dao.settlement_cases (
                settlement_case_id,
                promise_intent_id,
                realm_id,
                case_status,
                backend_key,
                backend_version
            )
            VALUES ($1, $2, $3, 'pending_funding', 'pi', 'sandbox-2026-04')
            ",
            &[
                &blocked_settlement_case_id,
                &blocked_promise_intent_id,
                &realm_id,
            ],
        )
        .await
        .expect("blocked settlement case must insert");
    blocker_tx
        .execute(
            "
            INSERT INTO dao.promise_intent_idempotency_keys (
                initiator_account_id,
                internal_idempotency_key,
                promise_intent_id,
                request_payload_hash
            )
            VALUES ($1, $2, $3, $4)
            ",
            &[
                &Uuid::parse_str(&initiator.account_id).expect("initiator account id must be uuid"),
                &internal_idempotency_key,
                &blocked_promise_intent_id,
                &request_hash,
            ],
        )
        .await
        .expect("blocked idempotency row must insert");
    let blocker_pid: i32 = blocker_tx
        .query_one("SELECT pg_backend_pid() AS pid", &[])
        .await
        .expect("blocker backend pid must be queryable")
        .get("pid");

    let app_clone = app.clone();
    let initiator_token = initiator.token.clone();
    let counterparty_account_id = counterparty.account_id.clone();
    let request = tokio::spawn(async move {
        post_json(
            &app_clone,
            "/api/promise/intents",
            Some(initiator_token.as_str()),
            json!({
                "internal_idempotency_key": internal_idempotency_key,
                "realm_id": realm_id,
                "counterparty_account_id": counterparty_account_id,
                "deposit_amount_minor_units": 10000,
                "currency_code": "PI"
            }),
        )
        .await
    });
    wait_for_backend_lock_contention(blocker_pid).await;
    blocker_tx
        .commit()
        .await
        .expect("blocker transaction must commit");

    let response = request.await.expect("request task must join");
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["replayed_intent"], true);
    assert_eq!(
        response.body["promise_intent_id"].as_str(),
        Some(blocked_promise_intent_id.to_string().as_str())
    );
    assert_eq!(
        response.body["settlement_case_id"].as_str(),
        Some(blocked_settlement_case_id.to_string().as_str())
    );

    let client = test_db_client().await;
    let row = client
        .query_one(
            "
            SELECT
                (SELECT count(*) FROM dao.promise_intents) AS promise_count,
                (SELECT count(*) FROM dao.settlement_cases) AS settlement_count,
                (SELECT count(*) FROM dao.promise_intent_idempotency_keys) AS idempotency_count
            ",
            &[],
        )
        .await
        .expect("promise race counts must be queryable");
    assert_eq!(row.get::<_, i64>("promise_count"), 1);
    assert_eq!(row.get::<_, i64>("settlement_count"), 1);
    assert_eq!(row.get::<_, i64>("idempotency_count"), 1);
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

async fn wait_for_backend_lock_contention(blocking_pid: i32) {
    let observer = test_db_client().await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let blocked: bool = observer
            .query_one(
                "
                SELECT EXISTS (
                    SELECT 1
                    FROM pg_stat_activity
                    WHERE wait_event_type = 'Lock'
                      AND $1 = ANY(pg_blocking_pids(pid))
                ) AS blocked
                ",
                &[&blocking_pid],
            )
            .await
            .expect("lock contention must be queryable")
            .get("blocked");
        if blocked {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "request never blocked on backend pid {blocking_pid}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

fn sha256_hex_test(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn promise_request_hash_test(
    realm_id: &str,
    counterparty_account_id: &str,
    minor_units: i128,
    currency_code: &str,
    scale: u32,
) -> String {
    let material = [
        realm_id,
        counterparty_account_id,
        &minor_units.to_string(),
        currency_code,
        &scale.to_string(),
    ]
    .join("\u{1f}");
    sha256_hex_test(material.as_bytes())
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
