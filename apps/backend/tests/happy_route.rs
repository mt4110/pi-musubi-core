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
async fn projection_read_models_expose_freshness_and_bounded_trust() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_funded_case(&app).await;

    let promise_view = get_json(
        &app,
        &format!(
            "/api/projection/promise-views/{}",
            prepared.promise_intent_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(promise_view.status, StatusCode::OK);
    assert_eq!(
        promise_view.body["promise_intent_id"],
        prepared.promise_intent_id
    );
    assert_eq!(promise_view.body["realm_id"], prepared.realm_id);
    assert_eq!(
        promise_view.body["counterparty_account_id"],
        prepared.counterparty_account_id
    );
    assert_eq!(promise_view.body["current_intent_status"], "proposed");
    assert_eq!(promise_view.body["deposit_amount_minor_units"], 10000);
    assert_eq!(promise_view.body["latest_settlement_status"], "funded");
    assert!(
        promise_view.body["provenance"]["source_fact_count"]
            .as_i64()
            .expect("promise source fact count must be numeric")
            >= 2
    );

    let expanded_settlement = get_json(
        &app,
        &format!(
            "/api/projection/settlement-views/{}/expanded",
            prepared.settlement_case_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(expanded_settlement.status, StatusCode::OK);
    assert_eq!(
        expanded_settlement.body["settlement_case_id"],
        prepared.settlement_case_id
    );
    assert_eq!(
        expanded_settlement.body["current_settlement_status"],
        "funded"
    );
    assert_eq!(expanded_settlement.body["proof_status"], "unavailable");
    assert_eq!(expanded_settlement.body["proof_signal_count"], 0);
    assert!(
        expanded_settlement.body["provenance"]["source_fact_count"]
            .as_i64()
            .expect("settlement source fact count must be numeric")
            >= 5
    );

    let trust = get_json(
        &app,
        &format!(
            "/api/projection/trust-snapshots/{}",
            prepared.initiator_account_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(trust.status, StatusCode::OK);
    assert_eq!(trust.body["realm_id"], Value::Null);
    assert_eq!(trust.body["trust_posture"], "bounded_reliability_observed");
    assert_eq!(trust.body["funded_settlement_count_90d"], 1);
    assert_eq!(trust.body["proof_status"], "unavailable");
    assert!(trust.body.get("trust_score").is_none());
    assert!(trust.body.get("rank").is_none());
    let reason_codes = trust.body["reason_codes"]
        .as_array()
        .expect("reason_codes must be an array");
    assert!(
        reason_codes
            .iter()
            .any(|code| { code.as_str() == Some("deposit_backed_promise_funded") })
    );
    assert!(
        reason_codes
            .iter()
            .any(|code| code.as_str() == Some("proof_unavailable"))
    );

    let counterparty_trust = get_json(
        &app,
        &format!(
            "/api/projection/trust-snapshots/{}",
            prepared.counterparty_account_id
        ),
        Some(prepared.counterparty_token.as_str()),
    )
    .await;
    assert_eq!(counterparty_trust.status, StatusCode::OK);
    assert_eq!(counterparty_trust.body["funded_settlement_count_90d"], 1);
    assert!(
        counterparty_trust.body["reason_codes"]
            .as_array()
            .expect("counterparty reason codes must be an array")
            .iter()
            .any(|code| code.as_str() == Some("deposit_backed_promise_funded"))
    );
}

#[tokio::test]
async fn projection_rebuild_restores_read_models_from_writer_truth() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_funded_case(&app).await;
    let client = test_db_client().await;

    client
        .batch_execute(
            "
            DELETE FROM projection.projection_meta;
            DELETE FROM projection.realm_trust_snapshots;
            DELETE FROM projection.trust_snapshots;
            DELETE FROM projection.settlement_views;
            DELETE FROM projection.promise_views;
            ",
        )
        .await
        .expect("projection rows must be deletable");

    let missing_view = get_json(
        &app,
        &format!(
            "/api/projection/settlement-views/{}",
            prepared.settlement_case_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(missing_view.status, StatusCode::NOT_FOUND);

    let rebuild = post_json(&app, "/api/internal/projection/rebuild", None, json!({})).await;
    assert_eq!(rebuild.status, StatusCode::OK);
    let rebuild_generation = rebuild.body["rebuild_generation"]
        .as_str()
        .expect("rebuild generation must exist")
        .to_owned();
    let rebuilt = rebuild.body["rebuilt"]
        .as_array()
        .expect("rebuilt items must be an array");
    let matching_rebuilt_count = |projection_name: &str, projection_row_count: i64| {
        rebuilt
            .iter()
            .filter(|item| {
                item["projection_name"] == projection_name
                    && item["projection_row_count"].as_i64() == Some(projection_row_count)
            })
            .count()
    };
    assert_eq!(matching_rebuilt_count("promise_views", 1), 1);
    assert_eq!(matching_rebuilt_count("settlement_views", 1), 1);
    assert_eq!(matching_rebuilt_count("trust_snapshots", 2), 1);
    assert_eq!(matching_rebuilt_count("realm_trust_snapshots", 2), 1);

    let rebuilt_promise = get_json(
        &app,
        &format!(
            "/api/projection/promise-views/{}",
            prepared.promise_intent_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(rebuilt_promise.status, StatusCode::OK);
    assert_eq!(
        rebuilt_promise.body["provenance"]["rebuild_generation"],
        rebuild_generation
    );

    let rebuilt_trust = get_json(
        &app,
        &format!(
            "/api/projection/trust-snapshots/{}",
            prepared.initiator_account_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(rebuilt_trust.status, StatusCode::OK);
    assert_eq!(
        rebuilt_trust.body["provenance"]["rebuild_generation"],
        rebuild_generation
    );
}

#[tokio::test]
async fn projection_rebuild_reports_source_watermark_and_positive_lag_for_stale_facts() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_funded_case(&app).await;
    let client = test_db_client().await;

    client
        .execute(
            "
            UPDATE dao.promise_intents
            SET updated_at = CURRENT_TIMESTAMP - interval '2 hours'
            WHERE promise_intent_id::text = $1
            ",
            &[&prepared.promise_intent_id],
        )
        .await
        .expect("promise timestamp must be adjustable for lag test");
    client
        .execute(
            "
            UPDATE dao.settlement_cases
            SET updated_at = CURRENT_TIMESTAMP - interval '2 hours'
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("settlement timestamp must be adjustable for lag test");
    client
        .execute(
            "
            UPDATE core.payment_receipts
            SET updated_at = CURRENT_TIMESTAMP - interval '2 hours'
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("receipt timestamp must be adjustable for lag test");
    client
        .execute(
            "
            UPDATE ledger.journal_entries
            SET
                effective_at = CURRENT_TIMESTAMP - interval '2 hours',
                created_at = CURRENT_TIMESTAMP - interval '2 hours'
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("journal timestamp must be adjustable for lag test");
    client
        .execute(
            "
            UPDATE dao.settlement_observations
            SET observed_at = CURRENT_TIMESTAMP - interval '2 hours'
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("observation timestamp must be adjustable for lag test");

    let rebuild = post_json(&app, "/api/internal/projection/rebuild", None, json!({})).await;
    assert_eq!(rebuild.status, StatusCode::OK);
    for item in rebuild.body["rebuilt"]
        .as_array()
        .expect("rebuilt items must be an array")
    {
        let projection_lag_ms = item["projection_lag_ms"]
            .as_i64()
            .expect("projection_lag_ms must be numeric");
        assert!(
            projection_lag_ms >= 60 * 60 * 1000,
            "projection meta lag should reflect stale writer facts: {projection_lag_ms}"
        );
        assert!(item["source_watermark_at"].as_str().is_some());
        assert!(
            item["source_fact_count"]
                .as_i64()
                .expect("source fact count must be numeric")
                > 0
        );
    }

    let promise_view = get_json(
        &app,
        &format!(
            "/api/projection/promise-views/{}",
            prepared.promise_intent_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(promise_view.status, StatusCode::OK);
    assert_stale_provenance(&promise_view.body["provenance"]);

    let expanded_settlement = get_json(
        &app,
        &format!(
            "/api/projection/settlement-views/{}/expanded",
            prepared.settlement_case_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(expanded_settlement.status, StatusCode::OK);
    assert_stale_provenance(&expanded_settlement.body["provenance"]);

    let trust = get_json(
        &app,
        &format!(
            "/api/projection/trust-snapshots/{}",
            prepared.initiator_account_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(trust.status, StatusCode::OK);
    assert_stale_provenance(&trust.body["provenance"]);
}

#[tokio::test]
async fn duplicate_projector_input_is_safe_for_projection_rows() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_funded_case(&app).await;
    let client = test_db_client().await;

    let event_row = client
        .query_one(
            "
            SELECT event_id
            FROM outbox.events
            WHERE aggregate_id::text = $1
              AND event_type = 'REFRESH_SETTLEMENT_VIEW'
            ORDER BY created_at DESC
            LIMIT 1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("settlement projection event must exist");
    let event_id: Uuid = event_row.get("event_id");
    let before = client
        .query_one(
            "
            SELECT last_projected_at
            FROM projection.settlement_views
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("settlement view must exist before duplicate input");
    let before_projected_at: chrono::DateTime<chrono::Utc> = before.get("last_projected_at");

    client
        .execute(
            "
            UPDATE outbox.events
            SET delivery_status = 'pending',
                published_at = NULL,
                retain_until = NULL,
                available_at = CURRENT_TIMESTAMP
            WHERE event_id = $1
            ",
            &[&event_id],
        )
        .await
        .expect("projection event must be requeueable for duplicate input test");

    let drain = post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain.status, StatusCode::OK);
    let event_id_text = event_id.to_string();
    assert!(
        drain.body["processed_messages"]
            .as_array()
            .expect("processed messages must be an array")
            .iter()
            .any(|message| {
                message["event_id"].as_str() == Some(event_id_text.as_str())
                    && message["event_type"] == "REFRESH_SETTLEMENT_VIEW"
                    && message["already_processed"].as_bool() == Some(true)
            })
    );

    let after = client
        .query_one(
            "
            SELECT
                last_projected_at,
                (SELECT count(*) FROM projection.settlement_views) AS settlement_view_count,
                (SELECT count(*) FROM projection.trust_snapshots) AS trust_snapshot_count
            FROM projection.settlement_views
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("settlement view must remain after duplicate input");
    let after_projected_at: chrono::DateTime<chrono::Utc> = after.get("last_projected_at");
    assert_eq!(after_projected_at, before_projected_at);
    assert_eq!(after.get::<_, i64>("settlement_view_count"), 1);
    assert_eq!(after.get::<_, i64>("trust_snapshot_count"), 2);
}

#[tokio::test]
async fn trust_visibility_is_self_scoped_and_realm_local_separated() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_funded_case(&app).await;

    let global = get_json(
        &app,
        &format!(
            "/api/projection/trust-snapshots/{}",
            prepared.initiator_account_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(global.status, StatusCode::OK);
    assert_eq!(global.body["realm_id"], Value::Null);
    assert!(
        global.body["reason_codes"]
            .as_array()
            .expect("global reason codes must be an array")
            .iter()
            .all(|code| code.as_str() != Some("realm_scoped"))
    );

    let realm = get_json(
        &app,
        &format!(
            "/api/projection/realm-trust-snapshots/{}/{}",
            prepared.realm_id, prepared.initiator_account_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(realm.status, StatusCode::OK);
    assert_eq!(realm.body["realm_id"], prepared.realm_id);
    assert!(
        realm.body["reason_codes"]
            .as_array()
            .expect("realm reason codes must be an array")
            .iter()
            .any(|code| code.as_str() == Some("realm_scoped"))
    );

    let cross_account = get_json(
        &app,
        &format!(
            "/api/projection/trust-snapshots/{}",
            prepared.initiator_account_id
        ),
        Some(prepared.counterparty_token.as_str()),
    )
    .await;
    assert_eq!(cross_account.status, StatusCode::NOT_FOUND);

    let wrong_realm = get_json(
        &app,
        &format!(
            "/api/projection/realm-trust-snapshots/other-realm/{}",
            prepared.initiator_account_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(wrong_realm.status, StatusCode::NOT_FOUND);
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
async fn orchestration_repair_resets_stale_outbox_and_inbox_claims() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

    let initiator = sign_in(&app, "pi-user-repair-stale-a", "repair-stale-a").await;
    let counterparty = sign_in(&app, "pi-user-repair-stale-b", "repair-stale-b").await;

    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "promise-intent-repair-stale",
            "realm_id": "realm-repair-stale",
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
    let event_row = client
        .query_one(
            "
            SELECT event_id, event_type, schema_version, payload_hash
            FROM outbox.events
            WHERE aggregate_id::text = $1
              AND event_type = 'OPEN_HOLD_INTENT'
            ",
            &[&settlement_case_id],
        )
        .await
        .expect("open hold event must exist");
    let event_id: Uuid = event_row.get("event_id");
    let event_type: String = event_row.get("event_type");
    let schema_version: i32 = event_row.get("schema_version");
    let payload_hash: String = event_row.get("payload_hash");

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
        .expect("stale outbox fixture must update");
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
                attempt_count,
                claimed_by,
                claimed_until
            )
            VALUES (
                $1,
                'settlement-orchestrator',
                $2,
                $2,
                $3,
                'processing',
                $4,
                $5,
                CURRENT_TIMESTAMP - interval '10 minutes',
                CURRENT_TIMESTAMP,
                1,
                'dead-worker',
                CURRENT_TIMESTAMP - interval '1 minute'
            )
            ",
            &[
                &Uuid::new_v4(),
                &event_id,
                &payload_hash,
                &event_type,
                &schema_version,
            ],
        )
        .await
        .expect("stale inbox fixture must insert");

    let repair = post_repair(&app, repair_request(false)).await;
    assert_eq!(repair.status, StatusCode::OK);
    assert!(repair.body["recovery_run_id"].as_str().is_some());
    assert_eq!(repair.body["stale_outbox_reclaimed_count"], 1);
    assert_eq!(repair.body["stale_inbox_reclaimed_count"], 1);
    assert_eq!(repair.body["producer_cleanup_repaired_count"], 0);

    let repaired = client
        .query_one(
            "
            SELECT
                events.delivery_status,
                events.claimed_by AS event_claimed_by,
                events.claimed_until AS event_claimed_until,
                inbox.status AS inbox_status,
                inbox.claimed_by AS inbox_claimed_by,
                inbox.claimed_until AS inbox_claimed_until,
                (SELECT count(*) FROM outbox.recovery_runs WHERE completed_at IS NOT NULL) AS recovery_count
            FROM outbox.events events
            JOIN outbox.command_inbox inbox
                ON inbox.command_id = events.event_id
            WHERE events.event_id = $1
            ",
            &[&event_id],
        )
        .await
        .expect("repaired rows must remain queryable");
    assert_eq!(repaired.get::<_, String>("delivery_status"), "pending");
    assert_eq!(repaired.get::<_, Option<String>>("event_claimed_by"), None);
    assert_eq!(
        repaired.get::<_, Option<chrono::DateTime<chrono::Utc>>>("event_claimed_until"),
        None
    );
    assert_eq!(repaired.get::<_, String>("inbox_status"), "pending");
    assert_eq!(repaired.get::<_, Option<String>>("inbox_claimed_by"), None);
    assert_eq!(
        repaired.get::<_, Option<chrono::DateTime<chrono::Utc>>>("inbox_claimed_until"),
        None
    );
    assert_eq!(repaired.get::<_, i64>("recovery_count"), 1);

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
async fn orchestration_repair_marks_event_published_after_completed_command() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

    let initiator = sign_in(&app, "pi-user-repair-completed-a", "repair-completed-a").await;
    let counterparty = sign_in(&app, "pi-user-repair-completed-b", "repair-completed-b").await;

    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "promise-intent-repair-completed",
            "realm_id": "realm-repair-completed",
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
    let event_row = client
        .query_one(
            "
            SELECT event_id, event_type, schema_version, payload_hash
            FROM outbox.events
            WHERE aggregate_id::text = $1
              AND event_type = 'OPEN_HOLD_INTENT'
            ",
            &[&settlement_case_id],
        )
        .await
        .expect("open hold event must exist");
    let event_id: Uuid = event_row.get("event_id");
    let event_type: String = event_row.get("event_type");
    let schema_version: i32 = event_row.get("schema_version");
    let payload_hash: String = event_row.get("payload_hash");

    client
        .execute(
            "
            UPDATE outbox.events
            SET delivery_status = 'processing',
                claimed_by = 'writer-before-crash',
                claimed_until = CURRENT_TIMESTAMP + interval '1 minute',
                last_attempt_at = CURRENT_TIMESTAMP
            WHERE event_id = $1
            ",
            &[&event_id],
        )
        .await
        .expect("outbox producer cleanup fixture must update");
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
                'settlement-orchestrator',
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
                &event_id,
                &payload_hash,
                &event_type,
                &schema_version,
            ],
        )
        .await
        .expect("completed inbox fixture must insert");

    let repair = post_repair(&app, repair_request(false)).await;
    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["producer_cleanup_repaired_count"], 1);
    assert_eq!(repair.body["stale_outbox_reclaimed_count"], 0);

    let repaired = client
        .query_one(
            "
            SELECT delivery_status, claimed_by, claimed_until, retain_until
            FROM outbox.events
            WHERE event_id = $1
            ",
            &[&event_id],
        )
        .await
        .expect("repaired event must remain queryable");
    assert_eq!(repaired.get::<_, String>("delivery_status"), "published");
    assert_eq!(repaired.get::<_, Option<String>>("claimed_by"), None);
    assert_eq!(
        repaired.get::<_, Option<chrono::DateTime<chrono::Utc>>>("claimed_until"),
        None
    );
    assert!(
        repaired
            .get::<_, Option<chrono::DateTime<chrono::Utc>>>("retain_until")
            .is_some()
    );

    let second_repair = post_repair(&app, repair_request(false)).await;
    assert_eq!(second_repair.status, StatusCode::OK);
    assert_eq!(second_repair.body["producer_cleanup_repaired_count"], 0);
}

#[tokio::test]
async fn orchestration_repair_reenqueues_raw_callback_evidence_after_pitr_gap() {
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
            "txid": "pi-tx-repair-pitr",
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
    let deleted = client
        .execute(
            "
            DELETE FROM outbox.events
            WHERE aggregate_id::text = $1
              AND event_type = 'INGEST_PROVIDER_CALLBACK'
            ",
            &[&raw_callback_id],
        )
        .await
        .expect("callback outbox event must be deletable for PITR fixture");
    assert_eq!(deleted, 1);

    let repair = post_repair(&app, repair_request(false)).await;
    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["callback_ingest_enqueued_count"], 1);

    let drain = post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain.status, StatusCode::OK);
    assert!(
        drain.body["processed_messages"]
            .as_array()
            .expect("processed_messages must be an array")
            .iter()
            .any(|message| {
                message["event_type"] == "INGEST_PROVIDER_CALLBACK"
                    && message["provider_submission_id"].as_str()
                        == Some(prepared.payment_id.as_str())
            })
    );

    let repaired = client
        .query_one(
            "
            SELECT
                (SELECT case_status FROM dao.settlement_cases WHERE settlement_case_id::text = $1) AS case_status,
                (SELECT count(*) FROM core.payment_receipts WHERE settlement_case_id::text = $1) AS receipt_count,
                (SELECT count(*) FROM ledger.journal_entries WHERE settlement_case_id::text = $1) AS journal_count
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("PITR repair result must be queryable");
    assert_eq!(repaired.get::<_, String>("case_status"), "funded");
    assert_eq!(repaired.get::<_, i64>("receipt_count"), 1);
    assert_eq!(repaired.get::<_, i64>("journal_count"), 1);

    let second_repair = post_repair(&app, repair_request(false)).await;
    assert_eq!(second_repair.status, StatusCode::OK);
    assert_eq!(second_repair.body["callback_ingest_enqueued_count"], 0);
    assert_eq!(second_repair.body["verified_receipt_repaired_count"], 0);
}

#[tokio::test]
async fn orchestration_repair_ignores_projection_without_writer_receipt() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;

    client
        .execute(
            "
            UPDATE projection.settlement_views
            SET current_settlement_status = 'funded',
                total_funded_minor_units = 10000,
                currency_code = 'PI',
                last_projected_at = CURRENT_TIMESTAMP
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("projection corruption fixture must update");

    let repair = post_repair(&app, repair_request(false)).await;
    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["verified_receipt_repaired_count"], 0);
    assert_eq!(repair.body["callback_ingest_enqueued_count"], 0);

    let writer = client
        .query_one(
            "
            SELECT
                (SELECT case_status FROM dao.settlement_cases WHERE settlement_case_id::text = $1) AS case_status,
                (SELECT count(*) FROM core.payment_receipts WHERE settlement_case_id::text = $1) AS receipt_count,
                (SELECT count(*) FROM ledger.journal_entries WHERE settlement_case_id::text = $1) AS journal_count
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("writer state must remain queryable");
    assert_eq!(writer.get::<_, String>("case_status"), "pending_funding");
    assert_eq!(writer.get::<_, i64>("receipt_count"), 0);
    assert_eq!(writer.get::<_, i64>("journal_count"), 0);
}

#[tokio::test]
async fn orchestration_repair_applies_verified_receipt_side_effects_forward_once() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;

    let raw_callback_id = Uuid::new_v4();
    let raw_body = json!({
        "payment_id": prepared.payment_id,
        "payer_pi_uid": prepared.initiator_pi_uid,
        "amount_minor_units": 10000,
        "currency_code": "PI",
        "txid": "pi-tx-repair-verified",
        "status": "completed"
    })
    .to_string();
    let raw_body_bytes = raw_body.as_bytes().to_vec();
    client
        .execute(
            "
            INSERT INTO core.raw_provider_callbacks (
                raw_callback_id,
                provider_name,
                dedupe_key,
                replay_of_raw_callback_id,
                raw_body_bytes,
                raw_body,
                redacted_headers,
                signature_valid,
                provider_submission_id,
                provider_ref,
                payer_pi_uid,
                amount_minor_units,
                currency_code,
                amount_scale,
                txid,
                callback_status,
                received_at
            )
            VALUES (
                $1,
                'pi',
                $2,
                NULL,
                $3,
                $4,
                '{}'::jsonb,
                NULL,
                $5,
                NULL,
                $6,
                10000,
                'PI',
                3,
                'pi-tx-repair-verified',
                'completed',
                CURRENT_TIMESTAMP
            )
            ",
            &[
                &raw_callback_id,
                &format!("repair-verified-{raw_callback_id}"),
                &raw_body_bytes,
                &raw_body,
                &prepared.payment_id,
                &prepared.initiator_pi_uid,
            ],
        )
        .await
        .expect("raw callback fixture must insert");
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
                &Uuid::new_v4(),
                &prepared.payment_id,
                &Uuid::parse_str(&prepared.settlement_case_id)
                    .expect("settlement_case_id must be uuid"),
                &Uuid::parse_str(&prepared.promise_intent_id)
                    .expect("promise_intent_id must be uuid"),
                &raw_callback_id,
            ],
        )
        .await
        .expect("verified receipt fixture must insert");

    let repair = post_repair(&app, repair_request(false)).await;
    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["verified_receipt_repaired_count"], 1);
    assert_eq!(repair.body["callback_ingest_enqueued_count"], 0);

    let writer = client
        .query_one(
            "
            SELECT
                (SELECT case_status FROM dao.settlement_cases WHERE settlement_case_id::text = $1) AS case_status,
                (SELECT count(*) FROM ledger.journal_entries WHERE settlement_case_id::text = $1) AS journal_count,
                (SELECT count(*)
                   FROM ledger.account_postings posting
                   JOIN ledger.journal_entries journal
                     ON journal.journal_entry_id = posting.journal_entry_id
                  WHERE journal.settlement_case_id::text = $1) AS posting_count
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("writer repair result must be queryable");
    assert_eq!(writer.get::<_, String>("case_status"), "funded");
    assert_eq!(writer.get::<_, i64>("journal_count"), 1);
    assert_eq!(writer.get::<_, i64>("posting_count"), 2);

    let second_repair = post_repair(&app, repair_request(false)).await;
    assert_eq!(second_repair.status, StatusCode::OK);
    assert_eq!(second_repair.body["verified_receipt_repaired_count"], 0);

    let journal_count: i64 = client
        .query_one(
            "
            SELECT count(*) AS count
            FROM ledger.journal_entries
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("journal count after replay must be queryable")
        .get("count");
    assert_eq!(journal_count, 1);
}

#[tokio::test]
async fn orchestration_repair_rejects_empty_body() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let request = Request::builder()
        .method("POST")
        .uri("/api/internal/orchestration/repair")
        .header("content-type", "application/json")
        .body(Body::empty())
        .expect("request must build");

    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("app should respond");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn orchestration_repair_requires_non_empty_reason() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let mut body = repair_request(false);
    body["reason"] = json!("   ");

    let repair = post_repair(&app, body).await;

    assert_eq!(repair.status, StatusCode::BAD_REQUEST);
    assert_eq!(repair.body["error"], "reason is required");
}

#[tokio::test]
async fn orchestration_repair_requires_operator_id() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

    let repair = post_json(
        &app,
        "/api/internal/orchestration/repair",
        None,
        repair_request(false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        repair.body["error"],
        "x-musubi-operator-id header is required"
    );
}

#[tokio::test]
async fn orchestration_repair_rejects_overlong_reason() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let mut body = repair_request(false);
    body["reason"] = json!("x".repeat(1001));

    let repair = post_repair(&app, body).await;

    assert_eq!(repair.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        repair.body["error"],
        "reason must be at most 1000 characters"
    );
}

#[tokio::test]
async fn orchestration_repair_rejects_overlong_operator_id() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let operator_id = "x".repeat(201);

    let repair = post_repair_with_operator(&app, repair_request(false), &operator_id).await;

    assert_eq!(repair.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        repair.body["error"],
        "x-musubi-operator-id must be at most 200 characters"
    );
}

#[tokio::test]
async fn orchestration_repair_rejects_oversized_body() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let request = Request::builder()
        .method("POST")
        .uri("/api/internal/orchestration/repair")
        .header("content-type", "application/json")
        .header("x-musubi-operator-id", "operator-issue-16")
        .body(Body::from(vec![b'a'; 20_000]))
        .expect("request must build");

    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("app should respond");

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn orchestration_repair_rejects_empty_scope() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, false, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        repair.body["error"],
        "at least one repair category must be included"
    );
}

#[tokio::test]
async fn orchestration_repair_dry_run_does_not_mutate_domain_rows() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let fixture = create_undrained_open_hold(&app, "repair-dry-run").await;
    let client = test_db_client().await;
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
            &[&fixture.event_id],
        )
        .await
        .expect("stale outbox fixture must update");

    let repair = post_repair(
        &app,
        repair_request_with_scope(true, 100, true, false, false, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["dry_run"], true);
    assert_eq!(repair.body["stale_outbox_reclaimed_count"], 1);
    let stored = client
        .query_one(
            "
            SELECT delivery_status, claimed_by
            FROM outbox.events
            WHERE event_id = $1
            ",
            &[&fixture.event_id],
        )
        .await
        .expect("dry-run event row must remain queryable");
    assert_eq!(stored.get::<_, String>("delivery_status"), "processing");
    assert_eq!(
        stored.get::<_, Option<String>>("claimed_by"),
        Some("dead-worker".to_owned())
    );
}

#[tokio::test]
async fn orchestration_repair_respects_max_rows_per_category() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let first = create_undrained_open_hold(&app, "repair-limit-a").await;
    let second = create_undrained_open_hold(&app, "repair-limit-b").await;
    let event_ids = vec![first.event_id, second.event_id];
    let client = test_db_client().await;
    client
        .execute(
            "
            UPDATE outbox.events
            SET delivery_status = 'processing',
                claimed_by = 'dead-worker',
                claimed_until = CURRENT_TIMESTAMP - interval '1 minute',
                last_attempt_at = CURRENT_TIMESTAMP - interval '1 minute'
            WHERE event_id = ANY($1::uuid[])
            ",
            &[&event_ids],
        )
        .await
        .expect("stale outbox fixtures must update");

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 1, true, false, false, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["stale_outbox_reclaimed_count"], 1);
    assert_eq!(repair.body["limited"], true);
    let stored = client
        .query_one(
            "
            SELECT
                count(*) FILTER (WHERE delivery_status = 'pending') AS pending_count,
                count(*) FILTER (WHERE delivery_status = 'processing') AS processing_count
            FROM outbox.events
            WHERE event_id = ANY($1::uuid[])
            ",
            &[&event_ids],
        )
        .await
        .expect("limited repair rows must remain queryable");
    assert_eq!(stored.get::<_, i64>("pending_count"), 1);
    assert_eq!(stored.get::<_, i64>("processing_count"), 1);
}

#[tokio::test]
async fn orchestration_repair_does_not_reset_unknown_outbox_event_type() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let event_id = Uuid::new_v4();
    let aggregate_id = Uuid::new_v4();
    let payload = json!({ "unknown": "repair-event" });
    let payload_hash = sha256_hex_test(payload.to_string().as_bytes());
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
                delivery_status,
                claimed_by,
                claimed_until,
                last_attempt_at
            )
            VALUES (
                $1,
                $2,
                'unknown_repair',
                $3,
                'UNKNOWN_REPAIR_EVENT',
                1,
                $4,
                $5,
                $6,
                'processing',
                'dead-worker',
                CURRENT_TIMESTAMP - interval '1 minute',
                CURRENT_TIMESTAMP - interval '1 minute'
            )
            ",
            &[
                &event_id,
                &Uuid::new_v4(),
                &aggregate_id,
                &payload,
                &payload_hash,
                &format!("unknown_repair:{aggregate_id}"),
            ],
        )
        .await
        .expect("unknown outbox event fixture must insert");

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, true, false, false, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["stale_outbox_reclaimed_count"], 0);
    let status: String = client
        .query_one(
            "SELECT delivery_status FROM outbox.events WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("unknown event status must be queryable")
        .get("delivery_status");
    assert_eq!(status, "processing");
}

#[tokio::test]
async fn orchestration_repair_does_not_reset_unknown_command_consumer() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let fixture = create_undrained_open_hold(&app, "repair-unknown-consumer").await;
    let client = test_db_client().await;
    let inbox_entry_id = Uuid::new_v4();
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
                attempt_count,
                claimed_by,
                claimed_until
            )
            VALUES (
                $1,
                'unknown-consumer',
                $2,
                $2,
                $3,
                'processing',
                $4,
                $5,
                CURRENT_TIMESTAMP - interval '10 minutes',
                CURRENT_TIMESTAMP,
                1,
                'dead-worker',
                CURRENT_TIMESTAMP - interval '1 minute'
            )
            ",
            &[
                &inbox_entry_id,
                &fixture.event_id,
                &fixture.payload_hash,
                &fixture.event_type,
                &fixture.schema_version,
            ],
        )
        .await
        .expect("unknown consumer inbox fixture must insert");

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, true, false, false, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["stale_inbox_reclaimed_count"], 0);
    let status: String = client
        .query_one(
            "SELECT status FROM outbox.command_inbox WHERE inbox_entry_id = $1",
            &[&inbox_entry_id],
        )
        .await
        .expect("unknown consumer inbox status must be queryable")
        .get("status");
    assert_eq!(status, "processing");
}

#[tokio::test]
async fn orchestration_repair_does_not_reset_outbox_row_that_is_no_longer_processing() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let fixture = create_undrained_open_hold(&app, "repair-outbox-not-processing").await;
    let client = test_db_client().await;
    client
        .execute(
            "
            UPDATE outbox.events
            SET delivery_status = 'pending',
                claimed_by = 'fresh-worker',
                claimed_until = CURRENT_TIMESTAMP - interval '1 minute',
                last_attempt_at = CURRENT_TIMESTAMP - interval '1 minute'
            WHERE event_id = $1
            ",
            &[&fixture.event_id],
        )
        .await
        .expect("non-processing outbox fixture must update");

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, true, false, false, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["stale_outbox_reclaimed_count"], 0);
    let status: String = client
        .query_one(
            "SELECT delivery_status FROM outbox.events WHERE event_id = $1",
            &[&fixture.event_id],
        )
        .await
        .expect("event status must be queryable")
        .get("delivery_status");
    assert_eq!(status, "pending");
}

#[tokio::test]
async fn orchestration_repair_does_not_reset_outbox_row_with_fresh_claim() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let fixture = create_undrained_open_hold(&app, "repair-outbox-fresh-claim").await;
    let client = test_db_client().await;
    client
        .execute(
            "
            UPDATE outbox.events
            SET delivery_status = 'processing',
                claimed_by = 'active-worker',
                claimed_until = CURRENT_TIMESTAMP + interval '5 minutes',
                last_attempt_at = CURRENT_TIMESTAMP
            WHERE event_id = $1
            ",
            &[&fixture.event_id],
        )
        .await
        .expect("fresh outbox fixture must update");

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, true, false, false, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["stale_outbox_reclaimed_count"], 0);
    let row = client
        .query_one(
            "
            SELECT delivery_status, claimed_by
            FROM outbox.events
            WHERE event_id = $1
            ",
            &[&fixture.event_id],
        )
        .await
        .expect("fresh outbox row must be queryable");
    assert_eq!(row.get::<_, String>("delivery_status"), "processing");
    assert_eq!(
        row.get::<_, Option<String>>("claimed_by"),
        Some("active-worker".to_owned())
    );
}

#[tokio::test]
async fn orchestration_repair_does_not_reset_completed_command_inbox_row() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let fixture = create_undrained_open_hold(&app, "repair-inbox-completed").await;
    let client = test_db_client().await;
    let inbox_entry_id = Uuid::new_v4();
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
                completed_at,
                claimed_by,
                claimed_until
            )
            VALUES (
                $1,
                'settlement-orchestrator',
                $2,
                $2,
                $3,
                'completed',
                $4,
                $5,
                CURRENT_TIMESTAMP - interval '10 minutes',
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP,
                'old-worker',
                CURRENT_TIMESTAMP - interval '1 minute'
            )
            ",
            &[
                &inbox_entry_id,
                &fixture.event_id,
                &fixture.payload_hash,
                &fixture.event_type,
                &fixture.schema_version,
            ],
        )
        .await
        .expect("completed command inbox fixture must insert");

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, true, false, false, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["stale_inbox_reclaimed_count"], 0);
    let status: String = client
        .query_one(
            "SELECT status FROM outbox.command_inbox WHERE inbox_entry_id = $1",
            &[&inbox_entry_id],
        )
        .await
        .expect("completed command status must be queryable")
        .get("status");
    assert_eq!(status, "completed");
}

#[tokio::test]
async fn orchestration_repair_does_not_reset_command_inbox_row_with_fresh_claim() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let fixture = create_undrained_open_hold(&app, "repair-inbox-fresh-claim").await;
    let client = test_db_client().await;
    let inbox_entry_id = Uuid::new_v4();
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
                claimed_by,
                claimed_until
            )
            VALUES (
                $1,
                'settlement-orchestrator',
                $2,
                $2,
                $3,
                'processing',
                $4,
                $5,
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP,
                'active-worker',
                CURRENT_TIMESTAMP + interval '5 minutes'
            )
            ",
            &[
                &inbox_entry_id,
                &fixture.event_id,
                &fixture.payload_hash,
                &fixture.event_type,
                &fixture.schema_version,
            ],
        )
        .await
        .expect("fresh command inbox fixture must insert");

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, true, false, false, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["stale_inbox_reclaimed_count"], 0);
    let row = client
        .query_one(
            "
            SELECT status, claimed_by
            FROM outbox.command_inbox
            WHERE inbox_entry_id = $1
            ",
            &[&inbox_entry_id],
        )
        .await
        .expect("fresh command inbox row must be queryable");
    assert_eq!(row.get::<_, String>("status"), "processing");
    assert_eq!(
        row.get::<_, Option<String>>("claimed_by"),
        Some("active-worker".to_owned())
    );
}

#[tokio::test]
async fn orchestration_repair_does_not_reset_unknown_command_type() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let fixture = create_undrained_open_hold(&app, "repair-unknown-command-type").await;
    let client = test_db_client().await;
    let inbox_entry_id = Uuid::new_v4();
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
                claimed_by,
                claimed_until
            )
            VALUES (
                $1,
                'settlement-orchestrator',
                $2,
                $2,
                $3,
                'processing',
                'UNKNOWN_COMMAND',
                $4,
                CURRENT_TIMESTAMP - interval '10 minutes',
                CURRENT_TIMESTAMP,
                'dead-worker',
                CURRENT_TIMESTAMP - interval '1 minute'
            )
            ",
            &[
                &inbox_entry_id,
                &fixture.event_id,
                &fixture.payload_hash,
                &fixture.schema_version,
            ],
        )
        .await
        .expect("unknown command type fixture must insert");

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, true, false, false, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["stale_inbox_reclaimed_count"], 0);
    let status: String = client
        .query_one(
            "SELECT status FROM outbox.command_inbox WHERE inbox_entry_id = $1",
            &[&inbox_entry_id],
        )
        .await
        .expect("unknown command type status must be queryable")
        .get("status");
    assert_eq!(status, "processing");
}

#[tokio::test]
async fn orchestration_repair_does_not_publish_completed_command_with_checksum_mismatch() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let fixture = create_undrained_open_hold(&app, "repair-producer-mismatch").await;
    let client = test_db_client().await;
    client
        .execute(
            "
            UPDATE outbox.events
            SET delivery_status = 'processing',
                claimed_by = 'writer-before-crash',
                claimed_until = CURRENT_TIMESTAMP + interval '1 minute',
                last_attempt_at = CURRENT_TIMESTAMP
            WHERE event_id = $1
            ",
            &[&fixture.event_id],
        )
        .await
        .expect("producer cleanup fixture must update");
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
                'settlement-orchestrator',
                $2,
                $2,
                'bogus-checksum',
                'completed',
                $3,
                $4,
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP
            )
            ",
            &[
                &Uuid::new_v4(),
                &fixture.event_id,
                &fixture.event_type,
                &fixture.schema_version,
            ],
        )
        .await
        .expect("checksum mismatch command fixture must insert");

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, true, false, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["producer_cleanup_repaired_count"], 0);
    let status: String = client
        .query_one(
            "SELECT delivery_status FROM outbox.events WHERE event_id = $1",
            &[&fixture.event_id],
        )
        .await
        .expect("producer mismatch event must be queryable")
        .get("delivery_status");
    assert_eq!(status, "processing");
}

#[tokio::test]
async fn orchestration_repair_producer_cleanup_rechecks_event_status_before_publishing() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let fixture = create_undrained_open_hold(&app, "repair-producer-status-recheck").await;
    let client = test_db_client().await;
    client
        .execute(
            "
            UPDATE outbox.events
            SET delivery_status = 'published',
                published_at = CURRENT_TIMESTAMP
            WHERE event_id = $1
            ",
            &[&fixture.event_id],
        )
        .await
        .expect("published producer fixture must update");
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
                'settlement-orchestrator',
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
                &fixture.event_id,
                &fixture.payload_hash,
                &fixture.event_type,
                &fixture.schema_version,
            ],
        )
        .await
        .expect("completed producer command fixture must insert");

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, true, false, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["producer_cleanup_repaired_count"], 0);
    let status: String = client
        .query_one(
            "SELECT delivery_status FROM outbox.events WHERE event_id = $1",
            &[&fixture.event_id],
        )
        .await
        .expect("producer event status must be queryable")
        .get("delivery_status");
    assert_eq!(status, "published");
}

#[tokio::test]
async fn orchestration_repair_ignores_raw_callback_without_provider_submission_id() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let raw_callback_id = Uuid::new_v4();
    let raw_body = json!({ "callback": "missing-provider-submission" }).to_string();
    let raw_body_bytes = raw_body.as_bytes().to_vec();
    client
        .execute(
            "
            INSERT INTO core.raw_provider_callbacks (
                raw_callback_id,
                provider_name,
                dedupe_key,
                replay_of_raw_callback_id,
                raw_body_bytes,
                raw_body,
                redacted_headers,
                signature_valid,
                provider_submission_id,
                received_at
            )
            VALUES ($1, 'pi', $2, NULL, $3, $4, '{}'::jsonb, NULL, NULL, CURRENT_TIMESTAMP)
            ",
            &[
                &raw_callback_id,
                &format!("missing-provider-submission-{raw_callback_id}"),
                &raw_body_bytes,
                &raw_body,
            ],
        )
        .await
        .expect("raw callback without provider submission must insert");

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, true, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["callback_ingest_enqueued_count"], 0);
    let event_count: i64 = client
        .query_one(
            "
            SELECT count(*) AS count
            FROM outbox.events
            WHERE aggregate_id = $1
              AND event_type = 'INGEST_PROVIDER_CALLBACK'
            ",
            &[&raw_callback_id],
        )
        .await
        .expect("callback repair event count must be queryable")
        .get("count");
    assert_eq!(event_count, 0);
}

#[tokio::test]
async fn orchestration_repair_reenqueues_callback_only_when_raw_evidence_matches_writer_truth() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;
    let raw_callback_id = insert_raw_callback_repair_fixture(
        &client,
        &prepared,
        "completed",
        10000,
        "PI",
        &prepared.initiator_pi_uid,
        &prepared.payment_id,
    )
    .await;

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, true, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["callback_ingest_enqueued_count"], 1);
    let event_count: i64 = client
        .query_one(
            "
            SELECT count(*) AS count
            FROM outbox.events
            WHERE aggregate_id = $1
              AND event_type = 'INGEST_PROVIDER_CALLBACK'
            ",
            &[&raw_callback_id],
        )
        .await
        .expect("callback event count must be queryable")
        .get("count");
    assert_eq!(event_count, 1);
}

#[tokio::test]
async fn orchestration_repair_does_not_reenqueue_failed_cancelled_rejected_or_unknown_callbacks() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;
    for status in ["failed", "cancelled", "rejected", "provider-new-state"] {
        insert_raw_callback_repair_fixture(
            &client,
            &prepared,
            status,
            10000,
            "PI",
            &prepared.initiator_pi_uid,
            &prepared.payment_id,
        )
        .await;
    }

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, true, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["callback_ingest_enqueued_count"], 0);
}

#[tokio::test]
async fn orchestration_repair_does_not_reenqueue_callback_with_amount_currency_or_payer_mismatch() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;
    insert_raw_callback_repair_fixture(
        &client,
        &prepared,
        "completed",
        9999,
        "PI",
        &prepared.initiator_pi_uid,
        &prepared.payment_id,
    )
    .await;
    insert_raw_callback_repair_fixture(
        &client,
        &prepared,
        "completed",
        10000,
        "USD",
        &prepared.initiator_pi_uid,
        &prepared.payment_id,
    )
    .await;
    insert_raw_callback_repair_fixture(
        &client,
        &prepared,
        "completed",
        10000,
        "PI",
        "wrong-payer-pi-uid",
        &prepared.payment_id,
    )
    .await;

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, true, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["callback_ingest_enqueued_count"], 0);
}

#[tokio::test]
async fn orchestration_repair_does_not_reenqueue_callback_for_non_accepted_submission() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;
    client
        .execute(
            "
            UPDATE dao.settlement_submissions
            SET submission_status = 'pending',
                updated_at = CURRENT_TIMESTAMP
            WHERE provider_submission_id = $1
            ",
            &[&prepared.payment_id],
        )
        .await
        .expect("submission status fixture must update");
    insert_raw_callback_repair_fixture(
        &client,
        &prepared,
        "completed",
        10000,
        "PI",
        &prepared.initiator_pi_uid,
        &prepared.payment_id,
    )
    .await;

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, true, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["callback_ingest_enqueued_count"], 0);
}

#[tokio::test]
async fn orchestration_repair_does_not_reenqueue_callback_when_verified_receipt_exists() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;
    insert_verified_receipt_fixture(&client, &prepared, "pi-tx-existing-receipt").await;
    let raw_callback_id = insert_raw_callback_repair_fixture(
        &client,
        &prepared,
        "completed",
        10000,
        "PI",
        &prepared.initiator_pi_uid,
        &prepared.payment_id,
    )
    .await;

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, true, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["callback_ingest_enqueued_count"], 0);
    let event_count: i64 = client
        .query_one(
            "
            SELECT count(*) AS count
            FROM outbox.events
            WHERE aggregate_id = $1
              AND event_type = 'INGEST_PROVIDER_CALLBACK'
            ",
            &[&raw_callback_id],
        )
        .await
        .expect("callback event count must be queryable")
        .get("count");
    assert_eq!(event_count, 0);
}

#[tokio::test]
async fn orchestration_repair_dry_run_callback_repair_does_not_enqueue() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;
    let raw_callback_id = insert_raw_callback_repair_fixture(
        &client,
        &prepared,
        "completed",
        10000,
        "PI",
        &prepared.initiator_pi_uid,
        &prepared.payment_id,
    )
    .await;

    let repair = post_repair(
        &app,
        repair_request_with_scope(true, 100, false, false, true, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["callback_ingest_enqueued_count"], 1);
    let event_count: i64 = client
        .query_one(
            "
            SELECT count(*) AS count
            FROM outbox.events
            WHERE aggregate_id = $1
              AND event_type = 'INGEST_PROVIDER_CALLBACK'
            ",
            &[&raw_callback_id],
        )
        .await
        .expect("callback event count must be queryable")
        .get("count");
    assert_eq!(event_count, 0);
}

#[tokio::test]
async fn orchestration_repair_does_not_reenqueue_callback_when_ingest_event_already_exists() {
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
            "txid": "pi-tx-existing-ingest-event",
            "status": "completed"
        }),
    )
    .await;
    assert_eq!(callback.status, StatusCode::OK);

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, true, false),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["callback_ingest_enqueued_count"], 0);
}

#[tokio::test]
async fn orchestration_repair_does_not_duplicate_callback_reenqueue() {
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
            "txid": "pi-tx-repair-duplicate-callback",
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
    client
        .execute(
            "
            DELETE FROM outbox.events
            WHERE aggregate_id::text = $1
              AND event_type = 'INGEST_PROVIDER_CALLBACK'
            ",
            &[&raw_callback_id],
        )
        .await
        .expect("callback outbox event must be deletable for duplicate repair fixture");

    let first_repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, true, false),
    )
    .await;
    let second_repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, true, false),
    )
    .await;

    assert_eq!(first_repair.status, StatusCode::OK);
    assert_eq!(second_repair.status, StatusCode::OK);
    assert_eq!(first_repair.body["callback_ingest_enqueued_count"], 1);
    assert_eq!(second_repair.body["callback_ingest_enqueued_count"], 0);
    let event_count: i64 = client
        .query_one(
            "
            SELECT count(*) AS count
            FROM outbox.events
            WHERE aggregate_id::text = $1
              AND event_type = 'INGEST_PROVIDER_CALLBACK'
            ",
            &[&raw_callback_id],
        )
        .await
        .expect("callback repair event count must be queryable")
        .get("count");
    assert_eq!(event_count, 1);
}

#[tokio::test]
async fn orchestration_repair_does_not_mark_funded_from_existing_receipt_journal_without_postings()
{
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;
    insert_verified_receipt_fixture(&client, &prepared, "pi-tx-existing-journal").await;
    insert_receipt_journal_header(&client, &prepared).await;

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, false, true),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["verified_receipt_repaired_count"], 0);
    let writer = client
        .query_one(
            "
            SELECT
                (SELECT case_status FROM dao.settlement_cases WHERE settlement_case_id::text = $1) AS case_status,
                (SELECT count(*) FROM ledger.journal_entries WHERE settlement_case_id::text = $1) AS journal_count,
                (SELECT count(*)
                   FROM ledger.account_postings posting
                   JOIN ledger.journal_entries journal
                     ON journal.journal_entry_id = posting.journal_entry_id
                  WHERE journal.settlement_case_id::text = $1) AS posting_count
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("existing receipt journal repair result must be queryable");
    assert_eq!(writer.get::<_, String>("case_status"), "pending_funding");
    assert_eq!(writer.get::<_, i64>("journal_count"), 1);
    assert_eq!(writer.get::<_, i64>("posting_count"), 0);
}

#[tokio::test]
async fn orchestration_repair_does_not_mark_funded_from_existing_receipt_journal_with_partial_postings()
 {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;
    insert_verified_receipt_fixture(&client, &prepared, "pi-tx-partial-journal").await;
    let journal_entry_id = insert_receipt_journal_header(&client, &prepared).await;
    insert_receipt_postings_fixture(
        &client,
        &prepared,
        &journal_entry_id,
        10000,
        "PI",
        true,
        false,
    )
    .await;

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, false, true),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["verified_receipt_repaired_count"], 0);
    let writer = client
        .query_one(
            "
            SELECT
                (SELECT case_status FROM dao.settlement_cases WHERE settlement_case_id::text = $1) AS case_status,
                (SELECT count(*)
                   FROM ledger.account_postings posting
                   JOIN ledger.journal_entries journal
                     ON journal.journal_entry_id = posting.journal_entry_id
                  WHERE journal.settlement_case_id::text = $1) AS posting_count
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("partial journal repair result must be queryable");
    assert_eq!(writer.get::<_, String>("case_status"), "pending_funding");
    assert_eq!(writer.get::<_, i64>("posting_count"), 1);
}

#[tokio::test]
async fn orchestration_repair_does_not_mark_funded_from_existing_receipt_journal_with_wrong_amount()
{
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;
    insert_verified_receipt_fixture(&client, &prepared, "pi-tx-wrong-amount-journal").await;
    let journal_entry_id = insert_receipt_journal_header(&client, &prepared).await;
    insert_receipt_postings_fixture(
        &client,
        &prepared,
        &journal_entry_id,
        9999,
        "PI",
        true,
        true,
    )
    .await;

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, false, true),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["verified_receipt_repaired_count"], 0);
    let case_status: String = client
        .query_one(
            "
            SELECT case_status
            FROM dao.settlement_cases
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("case status must be queryable")
        .get("case_status");
    assert_eq!(case_status, "pending_funding");
}

#[tokio::test]
async fn orchestration_repair_does_not_mark_funded_from_existing_receipt_journal_with_wrong_currency()
 {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;
    insert_verified_receipt_fixture(&client, &prepared, "pi-tx-wrong-currency-journal").await;
    let journal_entry_id = insert_receipt_journal_header(&client, &prepared).await;
    insert_receipt_postings_fixture(
        &client,
        &prepared,
        &journal_entry_id,
        10000,
        "USD",
        true,
        true,
    )
    .await;

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, false, true),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["verified_receipt_repaired_count"], 0);
    let case_status: String = client
        .query_one(
            "
            SELECT case_status
            FROM dao.settlement_cases
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("case status must be queryable")
        .get("case_status");
    assert_eq!(case_status, "pending_funding");
}

#[tokio::test]
async fn orchestration_repair_marks_funded_from_existing_receipt_journal_with_complete_postings() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let client = test_db_client().await;
    insert_verified_receipt_fixture(&client, &prepared, "pi-tx-complete-journal").await;
    let journal_entry_id = insert_receipt_journal_header(&client, &prepared).await;
    insert_receipt_postings_fixture(
        &client,
        &prepared,
        &journal_entry_id,
        10000,
        "PI",
        true,
        true,
    )
    .await;

    let repair = post_repair(
        &app,
        repair_request_with_scope(false, 100, false, false, false, true),
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    assert_eq!(repair.body["verified_receipt_repaired_count"], 1);
    let writer = client
        .query_one(
            "
            SELECT
                (SELECT case_status FROM dao.settlement_cases WHERE settlement_case_id::text = $1) AS case_status,
                (SELECT count(*) FROM ledger.journal_entries WHERE settlement_case_id::text = $1) AS journal_count,
                (SELECT count(*)
                   FROM ledger.account_postings posting
                   JOIN ledger.journal_entries journal
                     ON journal.journal_entry_id = posting.journal_entry_id
                  WHERE journal.settlement_case_id::text = $1) AS posting_count
            ",
            &[&prepared.settlement_case_id],
        )
        .await
        .expect("complete journal repair result must be queryable");
    assert_eq!(writer.get::<_, String>("case_status"), "funded");
    assert_eq!(writer.get::<_, i64>("journal_count"), 1);
    assert_eq!(writer.get::<_, i64>("posting_count"), 2);
}

#[tokio::test]
async fn orchestration_repair_audit_records_reason_scope_and_operator() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let body = json!({
        "dry_run": true,
        "reason": "operator requested scoped repair audit",
        "max_rows_per_category": 7,
        "include_stale_claims": true,
        "include_producer_cleanup": false,
        "include_callback_ingest": false,
        "include_verified_receipt_side_effects": false
    });

    let repair = post_json_with_headers(
        &app,
        "/api/internal/orchestration/repair",
        None,
        body,
        &[("x-musubi-operator-id", "operator-issue-16")],
    )
    .await;

    assert_eq!(repair.status, StatusCode::OK);
    let recovery_run_id = repair.body["recovery_run_id"]
        .as_str()
        .expect("recovery_run_id must exist");
    let client = test_db_client().await;
    let audit = client
        .query_one(
            "
            SELECT
                dry_run,
                request_reason,
                requested_by,
                max_rows_per_category,
                include_stale_claims,
                include_producer_cleanup,
                include_callback_ingest,
                include_verified_receipt_side_effects,
                limited
            FROM outbox.recovery_runs
            WHERE recovery_run_id::text = $1
            ",
            &[&recovery_run_id],
        )
        .await
        .expect("recovery audit row must exist");
    assert_eq!(audit.get::<_, bool>("dry_run"), true);
    assert_eq!(
        audit.get::<_, String>("request_reason"),
        "operator requested scoped repair audit"
    );
    assert_eq!(audit.get::<_, String>("requested_by"), "operator-issue-16");
    assert_eq!(audit.get::<_, i32>("max_rows_per_category"), 7);
    assert_eq!(audit.get::<_, bool>("include_stale_claims"), true);
    assert_eq!(audit.get::<_, bool>("include_producer_cleanup"), false);
    assert_eq!(audit.get::<_, bool>("include_callback_ingest"), false);
    assert_eq!(
        audit.get::<_, bool>("include_verified_receipt_side_effects"),
        false
    );
    assert_eq!(audit.get::<_, bool>("limited"), false);
}

#[tokio::test]
async fn orchestration_repair_conflicts_when_lock_is_held() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let mut lock_client = test_db_client().await;
    let lock_tx = lock_client
        .transaction()
        .await
        .expect("lock transaction must start");
    lock_tx
        .execute(
            "SELECT pg_advisory_xact_lock($1, $2)",
            &[&2_026_041_i32, &16_i32],
        )
        .await
        .expect("repair advisory lock must be held");

    let repair = post_repair(&app, repair_request(false)).await;

    assert_eq!(repair.status, StatusCode::CONFLICT);
    assert_eq!(
        repair.body["error"],
        "orchestration repair is already running"
    );
    lock_tx
        .rollback()
        .await
        .expect("lock transaction must roll back");
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
async fn ambiguous_callback_refreshes_trust_snapshots_for_manual_review() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;

    let before_trust = get_json(
        &app,
        &format!(
            "/api/projection/trust-snapshots/{}",
            prepared.initiator_account_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(before_trust.status, StatusCode::OK);
    assert_eq!(
        before_trust.body["trust_posture"],
        "insufficient_authoritative_facts"
    );

    let callback = post_json(
        &app,
        "/api/payment/callback",
        None,
        json!({
            "payment_id": prepared.payment_id,
            "payer_pi_uid": prepared.initiator_pi_uid,
            "amount_minor_units": 10000,
            "currency_code": "PI",
            "txid": "pi-tx-ambiguous-review",
            "status": "provider-new-state"
        }),
    )
    .await;
    assert_eq!(callback.status, StatusCode::OK);

    let drain = post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain.status, StatusCode::OK);
    let processed_messages = drain.body["processed_messages"]
        .as_array()
        .expect("processed_messages must be an array");
    assert!(processed_messages.iter().any(|message| {
        message["event_type"] == "INGEST_PROVIDER_CALLBACK"
            && message["provider_submission_id"].as_str() == Some(prepared.payment_id.as_str())
    }));
    assert!(processed_messages.iter().any(|message| {
        message["event_type"] == "REFRESH_SETTLEMENT_VIEW"
            && message["aggregate_id"].as_str() == Some(prepared.settlement_case_id.as_str())
    }));

    let initiator_trust = get_json(
        &app,
        &format!(
            "/api/projection/trust-snapshots/{}",
            prepared.initiator_account_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(initiator_trust.status, StatusCode::OK);
    assert_eq!(
        initiator_trust.body["trust_posture"],
        "review_attention_needed"
    );
    assert_eq!(initiator_trust.body["manual_review_case_bucket"], "some");
    assert!(
        initiator_trust.body["reason_codes"]
            .as_array()
            .expect("reason codes must be an array")
            .iter()
            .any(|code| code.as_str() == Some("manual_review_bucket_nonzero"))
    );

    let counterparty_trust = get_json(
        &app,
        &format!(
            "/api/projection/trust-snapshots/{}",
            prepared.counterparty_account_id
        ),
        Some(prepared.counterparty_token.as_str()),
    )
    .await;
    assert_eq!(counterparty_trust.status, StatusCode::OK);
    assert_eq!(
        counterparty_trust.body["trust_posture"],
        "review_attention_needed"
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
async fn later_verified_callback_refreshes_trust_after_manual_review_upgrade() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_funded_case(&app).await;
    let client = test_db_client().await;

    client
        .execute(
            "
            UPDATE core.payment_receipts
            SET receipt_status = 'manual_review',
                updated_at = CURRENT_TIMESTAMP
            WHERE provider_key = 'pi'
              AND external_payment_id = $1
            ",
            &[&prepared.payment_id],
        )
        .await
        .expect("receipt must be adjustable to manual_review");

    let rebuild = post_json(&app, "/api/internal/projection/rebuild", None, json!({})).await;
    assert_eq!(rebuild.status, StatusCode::OK);

    let manual_review_trust = get_json(
        &app,
        &format!(
            "/api/projection/trust-snapshots/{}",
            prepared.initiator_account_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(manual_review_trust.status, StatusCode::OK);
    assert_eq!(
        manual_review_trust.body["trust_posture"],
        "review_attention_needed"
    );
    assert_eq!(
        manual_review_trust.body["manual_review_case_bucket"],
        "some"
    );

    let verified_callback = post_json(
        &app,
        "/api/payment/callback",
        None,
        json!({
            "payment_id": prepared.payment_id,
            "payer_pi_uid": prepared.initiator_pi_uid,
            "amount_minor_units": 10000,
            "currency_code": "PI",
            "txid": "pi-tx-late-verified-refresh",
            "status": "completed"
        }),
    )
    .await;
    assert_eq!(verified_callback.status, StatusCode::OK);
    assert_eq!(verified_callback.body["duplicate_callback"], false);

    let drain = post_json(&app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain.status, StatusCode::OK);
    let processed_messages = drain.body["processed_messages"]
        .as_array()
        .expect("processed_messages must be an array");
    assert!(processed_messages.iter().any(|message| {
        message["event_type"] == "INGEST_PROVIDER_CALLBACK"
            && message["provider_submission_id"].as_str() == Some(prepared.payment_id.as_str())
    }));
    assert!(processed_messages.iter().any(|message| {
        message["event_type"] == "REFRESH_SETTLEMENT_VIEW"
            && message["aggregate_id"].as_str() == Some(prepared.settlement_case_id.as_str())
    }));

    let restored_trust = get_json(
        &app,
        &format!(
            "/api/projection/trust-snapshots/{}",
            prepared.initiator_account_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(restored_trust.status, StatusCode::OK);
    assert_eq!(
        restored_trust.body["trust_posture"],
        "bounded_reliability_observed"
    );
    assert_eq!(restored_trust.body["manual_review_case_bucket"], "none");
    assert!(
        !restored_trust.body["reason_codes"]
            .as_array()
            .expect("reason codes must be an array")
            .iter()
            .any(|code| code.as_str() == Some("manual_review_bucket_nonzero"))
    );
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
async fn projection_reads_require_authenticated_participant() {
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

    let anonymous_promise = get_json(
        &app,
        &format!(
            "/api/projection/promise-views/{}",
            prepared.promise_intent_id
        ),
        None,
    )
    .await;
    assert_eq!(anonymous_promise.status, StatusCode::UNAUTHORIZED);

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

    let outsider_promise = get_json(
        &app,
        &format!(
            "/api/projection/promise-views/{}",
            prepared.promise_intent_id
        ),
        Some(outsider.token.as_str()),
    )
    .await;
    assert_eq!(outsider_promise.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn promise_projection_response_uses_writer_owned_participant_ids() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_pending_case(&app).await;
    let outsider = sign_in(&app, "pi-user-projection-outsider", "projection-outsider").await;
    let outsider_account_id = Uuid::parse_str(&outsider.account_id).expect("outsider account uuid");
    let client = test_db_client().await;

    client
        .execute(
            "
            UPDATE projection.promise_views
            SET initiator_account_id = $2::uuid,
                counterparty_account_id = $2::uuid
            WHERE promise_intent_id::text = $1
            ",
            &[&prepared.promise_intent_id, &outsider_account_id],
        )
        .await
        .expect("projection participant corruption fixture must update");

    let promise = get_json(
        &app,
        &format!(
            "/api/projection/promise-views/{}",
            prepared.promise_intent_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;

    assert_eq!(promise.status, StatusCode::OK);
    assert_eq!(
        promise.body["initiator_account_id"],
        prepared.initiator_account_id
    );
    assert_eq!(
        promise.body["counterparty_account_id"],
        prepared.counterparty_account_id
    );
}

#[tokio::test]
async fn settlement_projection_reads_use_writer_owned_linkage() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let prepared = prepare_funded_case(&app).await;
    let outsider = sign_in(&app, "pi-user-settlement-outsider", "settlement-outsider").await;
    let unrelated_counterparty =
        sign_in(&app, "pi-user-settlement-unrelated", "settlement-unrelated").await;
    let client = test_db_client().await;

    let unrelated_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(outsider.token.as_str()),
        json!({
            "internal_idempotency_key": "promise-intent-settlement-linkage",
            "realm_id": "realm-settlement-linkage",
            "counterparty_account_id": unrelated_counterparty.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(unrelated_promise.status, StatusCode::OK);
    let unrelated_promise_intent_id = Uuid::parse_str(
        unrelated_promise.body["promise_intent_id"]
            .as_str()
            .expect("unrelated promise_intent_id must exist"),
    )
    .expect("unrelated promise_intent_id must parse");

    client
        .execute(
            "
            UPDATE projection.settlement_views
            SET promise_intent_id = $2,
                realm_id = 'realm-corrupted-linkage'
            WHERE settlement_case_id::text = $1
            ",
            &[&prepared.settlement_case_id, &unrelated_promise_intent_id],
        )
        .await
        .expect("projection settlement linkage corruption fixture must update");

    let settlement = get_json(
        &app,
        &format!(
            "/api/projection/settlement-views/{}",
            prepared.settlement_case_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(settlement.status, StatusCode::OK);
    assert_eq!(
        settlement.body["promise_intent_id"],
        prepared.promise_intent_id
    );
    assert_eq!(settlement.body["realm_id"], prepared.realm_id);

    let expanded_settlement = get_json(
        &app,
        &format!(
            "/api/projection/settlement-views/{}/expanded",
            prepared.settlement_case_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;
    assert_eq!(expanded_settlement.status, StatusCode::OK);
    assert_eq!(
        expanded_settlement.body["promise_intent_id"],
        prepared.promise_intent_id
    );
    assert_eq!(expanded_settlement.body["realm_id"], prepared.realm_id);

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

    let outsider_expanded_view = get_json(
        &app,
        &format!(
            "/api/projection/settlement-views/{}/expanded",
            prepared.settlement_case_id
        ),
        Some(outsider.token.as_str()),
    )
    .await;
    assert_eq!(outsider_expanded_view.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn projection_reads_keep_invalid_ids_as_not_found() {
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

    let invalid_promise = get_json(
        &app,
        "/api/projection/promise-views/not-a-uuid",
        Some(prepared.initiator_token.as_str()),
    )
    .await;

    assert_eq!(invalid_promise.status, StatusCode::NOT_FOUND);
    assert_eq!(
        invalid_promise.body["error"],
        "promise projection has not been built for that promise_intent_id"
    );

    let invalid_trust = get_json(
        &app,
        "/api/projection/trust-snapshots/not-a-uuid",
        Some(prepared.initiator_token.as_str()),
    )
    .await;

    assert_eq!(invalid_trust.status, StatusCode::NOT_FOUND);
    assert_eq!(
        invalid_trust.body["error"],
        "trust projection is not visible for that account"
    );

    let invalid_realm_trust = get_json(
        &app,
        &format!(
            "/api/projection/realm-trust-snapshots/{}/not-a-uuid",
            prepared.realm_id
        ),
        Some(prepared.initiator_token.as_str()),
    )
    .await;

    assert_eq!(invalid_realm_trust.status, StatusCode::NOT_FOUND);
    assert_eq!(
        invalid_realm_trust.body["error"],
        "trust projection is not visible for that account"
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
    promise_intent_id: String,
    settlement_case_id: String,
    realm_id: String,
    payment_id: String,
    initiator_account_id: String,
    initiator_pi_uid: String,
    initiator_token: String,
    counterparty_account_id: String,
    counterparty_token: String,
}

struct OpenHoldFixture {
    event_id: Uuid,
    event_type: String,
    schema_version: i32,
    payload_hash: String,
}

async fn create_undrained_open_hold(app: &Router, suffix: &str) -> OpenHoldFixture {
    let initiator_pi_uid = format!("pi-user-{suffix}-a");
    let initiator_username = format!("{suffix}-a");
    let counterparty_pi_uid = format!("pi-user-{suffix}-b");
    let counterparty_username = format!("{suffix}-b");
    let initiator = sign_in(app, &initiator_pi_uid, &initiator_username).await;
    let counterparty = sign_in(app, &counterparty_pi_uid, &counterparty_username).await;

    let create_promise = post_json(
        app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": format!("promise-intent-{suffix}"),
            "realm_id": format!("realm-{suffix}"),
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
    let event_row = client
        .query_one(
            "
            SELECT event_id, event_type, schema_version, payload_hash
            FROM outbox.events
            WHERE aggregate_id::text = $1
              AND event_type = 'OPEN_HOLD_INTENT'
            ",
            &[&settlement_case_id],
        )
        .await
        .expect("open hold event must exist");

    OpenHoldFixture {
        event_id: event_row.get("event_id"),
        event_type: event_row.get("event_type"),
        schema_version: event_row.get("schema_version"),
        payload_hash: event_row.get("payload_hash"),
    }
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
    let promise_intent_id = create_promise.body["promise_intent_id"]
        .as_str()
        .expect("promise_intent_id must exist")
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
        promise_intent_id,
        settlement_case_id,
        realm_id: "realm-prepare".to_owned(),
        payment_id,
        initiator_account_id: initiator.account_id,
        initiator_pi_uid: initiator.pi_uid,
        initiator_token: initiator.token,
        counterparty_account_id: counterparty.account_id,
        counterparty_token: counterparty.token,
    }
}

async fn insert_verified_receipt_fixture(
    client: &tokio_postgres::Client,
    prepared: &PreparedCase,
    txid: &str,
) -> Uuid {
    let raw_callback_id = Uuid::new_v4();
    let raw_body = json!({
        "payment_id": prepared.payment_id,
        "payer_pi_uid": prepared.initiator_pi_uid,
        "amount_minor_units": 10000,
        "currency_code": "PI",
        "txid": txid,
        "status": "completed"
    })
    .to_string();
    let raw_body_bytes = raw_body.as_bytes().to_vec();
    client
        .execute(
            "
            INSERT INTO core.raw_provider_callbacks (
                raw_callback_id,
                provider_name,
                dedupe_key,
                replay_of_raw_callback_id,
                raw_body_bytes,
                raw_body,
                redacted_headers,
                signature_valid,
                provider_submission_id,
                provider_ref,
                payer_pi_uid,
                amount_minor_units,
                currency_code,
                amount_scale,
                txid,
                callback_status,
                received_at
            )
            VALUES (
                $1,
                'pi',
                $2,
                NULL,
                $3,
                $4,
                '{}'::jsonb,
                NULL,
                $5,
                NULL,
                $6,
                10000,
                'PI',
                3,
                $7,
                'completed',
                CURRENT_TIMESTAMP
            )
            ",
            &[
                &raw_callback_id,
                &format!("verified-receipt-{raw_callback_id}"),
                &raw_body_bytes,
                &raw_body,
                &prepared.payment_id,
                &prepared.initiator_pi_uid,
                &txid,
            ],
        )
        .await
        .expect("raw callback fixture must insert");
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
                &Uuid::new_v4(),
                &prepared.payment_id,
                &Uuid::parse_str(&prepared.settlement_case_id)
                    .expect("settlement_case_id must be uuid"),
                &Uuid::parse_str(&prepared.promise_intent_id)
                    .expect("promise_intent_id must be uuid"),
                &raw_callback_id,
            ],
        )
        .await
        .expect("verified receipt fixture must insert");
    raw_callback_id
}

async fn insert_raw_callback_repair_fixture(
    client: &tokio_postgres::Client,
    prepared: &PreparedCase,
    status: &str,
    amount_minor_units: i64,
    currency_code: &str,
    payer_pi_uid: &str,
    provider_submission_id: &str,
) -> Uuid {
    let raw_callback_id = Uuid::new_v4();
    let raw_body = json!({
        "payment_id": provider_submission_id,
        "payer_pi_uid": payer_pi_uid,
        "amount_minor_units": amount_minor_units,
        "currency_code": currency_code,
        "txid": format!("pi-tx-repair-{raw_callback_id}"),
        "status": status
    })
    .to_string();
    let raw_body_bytes = raw_body.as_bytes().to_vec();
    client
        .execute(
            "
            INSERT INTO core.raw_provider_callbacks (
                raw_callback_id,
                provider_name,
                dedupe_key,
                replay_of_raw_callback_id,
                raw_body_bytes,
                raw_body,
                redacted_headers,
                signature_valid,
                provider_submission_id,
                provider_ref,
                payer_pi_uid,
                amount_minor_units,
                currency_code,
                amount_scale,
                txid,
                callback_status,
                received_at
            )
            VALUES (
                $1,
                'pi',
                $2,
                NULL,
                $3,
                $4,
                '{}'::jsonb,
                NULL,
                $5,
                NULL,
                $6,
                $7,
                $8,
                3,
                $9,
                $10,
                CURRENT_TIMESTAMP
            )
            ",
            &[
                &raw_callback_id,
                &format!("repair-callback-{raw_callback_id}"),
                &raw_body_bytes,
                &raw_body,
                &provider_submission_id,
                &payer_pi_uid,
                &amount_minor_units,
                &currency_code,
                &format!("pi-tx-repair-{raw_callback_id}"),
                &status,
            ],
        )
        .await
        .expect("raw callback repair fixture must insert");
    assert_eq!(
        provider_submission_id, prepared.payment_id,
        "repair fixture normally targets the prepared accepted submission"
    );
    raw_callback_id
}

async fn insert_receipt_journal_header(
    client: &tokio_postgres::Client,
    prepared: &PreparedCase,
) -> Uuid {
    let journal_entry_id = Uuid::new_v4();
    client
        .execute(
            "
            INSERT INTO ledger.journal_entries (
                journal_entry_id,
                settlement_case_id,
                promise_intent_id,
                realm_id,
                entry_kind,
                effective_at
            )
            VALUES ($1, $2, $3, $4, 'receipt_recognized', CURRENT_TIMESTAMP)
            ",
            &[
                &journal_entry_id,
                &Uuid::parse_str(&prepared.settlement_case_id)
                    .expect("settlement_case_id must be uuid"),
                &Uuid::parse_str(&prepared.promise_intent_id)
                    .expect("promise_intent_id must be uuid"),
                &prepared.realm_id,
            ],
        )
        .await
        .expect("receipt journal header fixture must insert");
    journal_entry_id
}

async fn insert_receipt_postings_fixture(
    client: &tokio_postgres::Client,
    prepared: &PreparedCase,
    journal_entry_id: &Uuid,
    amount_minor_units: i64,
    currency_code: &str,
    include_debit: bool,
    include_credit: bool,
) {
    if include_debit {
        client
            .execute(
                "
                INSERT INTO ledger.account_postings (
                    posting_id,
                    journal_entry_id,
                    posting_order,
                    ledger_account_code,
                    account_id,
                    direction,
                    amount_minor_units,
                    currency_code
                )
                VALUES ($1, $2, 1, 'provider_clearing_inbound', NULL, 'debit', $3, $4)
                ",
                &[
                    &Uuid::new_v4(),
                    journal_entry_id,
                    &amount_minor_units,
                    &currency_code,
                ],
            )
            .await
            .expect("debit posting fixture must insert");
    }
    if include_credit {
        client
            .execute(
                "
                INSERT INTO ledger.account_postings (
                    posting_id,
                    journal_entry_id,
                    posting_order,
                    ledger_account_code,
                    account_id,
                    direction,
                    amount_minor_units,
                    currency_code
                )
                VALUES ($1, $2, 2, 'user_secured_funds_liability', $3, 'credit', $4, $5)
                ",
                &[
                    &Uuid::new_v4(),
                    journal_entry_id,
                    &Uuid::parse_str(&prepared.initiator_account_id)
                        .expect("initiator_account_id must be uuid"),
                    &amount_minor_units,
                    &currency_code,
                ],
            )
            .await
            .expect("credit posting fixture must insert");
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

fn repair_request(dry_run: bool) -> Value {
    repair_request_with_scope(dry_run, 100, true, true, true, true)
}

fn repair_request_with_scope(
    dry_run: bool,
    max_rows_per_category: i64,
    include_stale_claims: bool,
    include_producer_cleanup: bool,
    include_callback_ingest: bool,
    include_verified_receipt_side_effects: bool,
) -> Value {
    json!({
        "dry_run": dry_run,
        "reason": "test repair",
        "max_rows_per_category": max_rows_per_category,
        "include_stale_claims": include_stale_claims,
        "include_producer_cleanup": include_producer_cleanup,
        "include_callback_ingest": include_callback_ingest,
        "include_verified_receipt_side_effects": include_verified_receipt_side_effects
    })
}

fn assert_stale_provenance(provenance: &Value) {
    let source_watermark = parse_datetime_field(provenance, "source_watermark_at");
    let freshness_checked = parse_datetime_field(provenance, "freshness_checked_at");
    let last_projected = parse_datetime_field(provenance, "last_projected_at");
    let projection_lag_ms = provenance["projection_lag_ms"]
        .as_i64()
        .expect("projection_lag_ms must be numeric");
    let source_fact_count = provenance["source_fact_count"]
        .as_i64()
        .expect("source_fact_count must be numeric");

    assert!(freshness_checked >= source_watermark);
    assert!(last_projected >= source_watermark);
    assert!(source_fact_count > 0);
    assert!(
        projection_lag_ms >= 60 * 60 * 1000,
        "projection lag should reflect stale writer facts: {projection_lag_ms}"
    );
    assert!(provenance["rebuild_generation"].as_str().is_some());
}

fn parse_datetime_field(value: &Value, field_name: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(
        value[field_name]
            .as_str()
            .unwrap_or_else(|| panic!("{field_name} must be an RFC3339 string")),
    )
    .unwrap_or_else(|error| panic!("{field_name} must parse as RFC3339: {error}"))
    .with_timezone(&chrono::Utc)
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

async fn post_json_with_headers(
    app: &Router,
    path: &str,
    bearer_token: Option<&str>,
    body: Value,
    headers: &[(&str, &str)],
) -> JsonResponse {
    request_json_with_headers(app, "POST", path, bearer_token, Some(body), headers).await
}

async fn post_repair(app: &Router, body: Value) -> JsonResponse {
    post_repair_with_operator(app, body, "operator-issue-16").await
}

async fn post_repair_with_operator(app: &Router, body: Value, operator_id: &str) -> JsonResponse {
    post_json_with_headers(
        app,
        "/api/internal/orchestration/repair",
        None,
        body,
        &[("x-musubi-operator-id", operator_id)],
    )
    .await
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
    request_json_with_headers(app, method, path, bearer_token, body, &[]).await
}

async fn request_json_with_headers(
    app: &Router,
    method: &str,
    path: &str,
    bearer_token: Option<&str>,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> JsonResponse {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = bearer_token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    for (name, value) in headers {
        builder = builder.header(*name, *value);
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
