use std::path::PathBuf;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use musubi_backend::{
    build_app, new_test_state,
    services::promise_completion::{
        PromiseCompletionAuthorityPosture, PromiseCompletionProjectionNonAuthorityPosture,
        PromiseCompletionSourceRouteClass, PromiseCompletionStateClass,
        PromiseCompletionWriterFactFamily, PromiseCompletionWriterFactStore,
        ProposedMutualAcknowledgementAcceptedTransition, ProposedPromiseCompletionWriterFact,
        RecordMutualAcknowledgementAcceptedTransitionInput, RecordPromiseCompletionWriterFactInput,
    },
};
use musubi_db_runtime::DbConfig;
use serde_json::{Value, json};
use tokio_postgres::NoTls;
use tower::ServiceExt;

#[tokio::test]
async fn participant_safe_display_availability_is_not_exposed_by_api_or_ui_surfaces() {
    let (_test_state, app, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("participant-display-api-non-exposure");
    let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;
    let accepted = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(&idempotency_key, &prior.writer_fact_id),
        ))
        .await
        .expect("accepted transition should persist");

    let participant_set_reference = format!("participant-set-{idempotency_key}");
    let acknowledgement_reference = format!("ordinary-acknowledgement-{idempotency_key}");
    let internal_display = store
        .derive_participant_safe_completed_reference_display_availability(
            &accepted.promise_reference,
            &accepted.realm_id,
            &participant_set_reference,
            &acknowledgement_reference,
        )
        .await
        .expect("participant-safe display availability should derive")
        .expect("involved participant set should receive internal availability");
    assert_eq!(internal_display.display_availability, "available");
    assert!(internal_display.completed_reference_available);

    let outsider = sign_in(
        &app,
        "pi-user-promise-completion-api-non-exposure-outsider",
        "promise-completion-api-non-exposure-outsider",
    )
    .await;
    let exposure_context = ExposureContext {
        accepted_writer_fact_id: accepted.writer_fact_id.clone(),
        prior_writer_fact_id: prior.writer_fact_id.clone(),
        promise_reference: accepted.promise_reference.clone(),
        realm_id: accepted.realm_id.clone(),
        promise_terms_reference: format!("promise-terms-{idempotency_key}"),
        participant_set_reference: participant_set_reference.clone(),
        acknowledgement_reference: acknowledgement_reference.clone(),
        idempotency_key: idempotency_key.clone(),
    };

    let proof_surface_side_effects_before = side_effect_counts(&client).await;
    let proof_surface_id = uuid::Uuid::new_v4();
    let proof_challenge_body = json!({
        "venue_id": format!("venue-proof-surface-{proof_surface_id}"),
        "realm_id": format!("realm-proof-surface-{proof_surface_id}"),
        "fallback_mode": "none"
    });
    let unauthenticated_proof_challenge = post_json(
        &app,
        "/api/proof/challenges",
        None,
        proof_challenge_body.clone(),
    )
    .await;
    assert_eq!(
        unauthenticated_proof_challenge.status,
        StatusCode::UNAUTHORIZED
    );
    assert_no_completion_exposure(&unauthenticated_proof_challenge, &exposure_context);

    let proof_challenge = post_json(
        &app,
        "/api/proof/challenges",
        Some(outsider.token.as_str()),
        proof_challenge_body,
    )
    .await;
    assert_eq!(proof_challenge.status, StatusCode::OK);
    assert_no_completion_exposure(&proof_challenge, &exposure_context);
    let proof_challenge_json = proof_challenge
        .body_json
        .as_ref()
        .expect("proof challenge response should be json");

    let proof_submission_body = json!({
        "challenge_id": proof_challenge_json["challenge_id"],
        "venue_id": proof_challenge_json["venue_id"],
        "display_code": "000000",
        "key_version": proof_challenge_json["venue_key_version"],
        "client_nonce": proof_challenge_json["client_nonce"],
        "observed_at_ms": chrono::Utc::now().timestamp_millis(),
        "coarse_location_bucket": "tokyo-shibuya",
        "device_session_id": format!("proof-device-{proof_surface_id}"),
        "fallback_mode": "none"
    });
    let unauthenticated_proof_submission = post_json(
        &app,
        "/api/proof/submissions",
        None,
        proof_submission_body.clone(),
    )
    .await;
    assert_eq!(
        unauthenticated_proof_submission.status,
        StatusCode::UNAUTHORIZED
    );
    assert_no_completion_exposure(&unauthenticated_proof_submission, &exposure_context);

    let proof_submission = post_json(
        &app,
        "/api/proof/submissions",
        Some(outsider.token.as_str()),
        proof_submission_body,
    )
    .await;
    assert_eq!(proof_submission.status, StatusCode::OK);
    assert_no_completion_exposure(&proof_submission, &exposure_context);
    assert_eq!(
        side_effect_counts(&client).await,
        proof_surface_side_effects_before
    );

    let writer_fact_count_before =
        writer_fact_count_for_promise(&client, &accepted.promise_reference).await;
    let side_effects_before = side_effect_counts(&client).await;

    for path in candidate_api_paths(&exposure_context) {
        for token in [None, Some(outsider.token.as_str())] {
            let response = get_http(&app, &path, token).await;
            assert_eq!(
                response.status,
                StatusCode::NOT_FOUND,
                "participant-safe display API candidate route must not exist: {path}"
            );
            assert_no_completion_exposure(&response, &exposure_context);
        }
    }

    for (path, expected_without_token, expected_with_outsider) in [
        (
            format!(
                "/api/projection/promise-views/{}",
                accepted.promise_reference
            ),
            StatusCode::UNAUTHORIZED,
            StatusCode::NOT_FOUND,
        ),
        (
            format!("/api/projection/trust-snapshots/{}", outsider.account_id),
            StatusCode::UNAUTHORIZED,
            StatusCode::NOT_FOUND,
        ),
        (
            format!(
                "/api/projection/realm-trust-snapshots/{}/{}",
                accepted.realm_id, outsider.account_id
            ),
            StatusCode::UNAUTHORIZED,
            StatusCode::NOT_FOUND,
        ),
    ] {
        let unauthenticated = get_http(&app, &path, None).await;
        assert_eq!(
            unauthenticated.status, expected_without_token,
            "unauthenticated existing projection route must not expose completion display data: {path}"
        );
        assert_no_completion_exposure(&unauthenticated, &exposure_context);

        let outsider_response = get_http(&app, &path, Some(outsider.token.as_str())).await;
        assert_eq!(
            outsider_response.status, expected_with_outsider,
            "non-involved Ordinary Account must not observe completion display data: {path}"
        );
        assert_no_completion_exposure(&outsider_response, &exposure_context);
    }

    for path in [
        "/api/internal/ops/health",
        "/api/internal/ops/readiness",
        "/api/internal/ops/observability/snapshot",
        "/api/internal/ops/observability/slo",
    ] {
        let internal_response = get_http(&app, path, None).await;
        assert_eq!(
            internal_response.status,
            StatusCode::OK,
            "internal ops observability route must remain readable without exposing completion display data: {path}"
        );
        assert_no_completion_display_exposure(&internal_response, &exposure_context);

        let participant_response = get_http(&app, path, Some(outsider.token.as_str())).await;
        assert_eq!(
            participant_response.status,
            StatusCode::UNAUTHORIZED,
            "participant bearer token must not unlock ops observability data: {path}"
        );
        assert_no_completion_display_exposure(&participant_response, &exposure_context);
    }

    assert_eq!(
        writer_fact_count_for_promise(&client, &accepted.promise_reference).await,
        writer_fact_count_before
    );
    assert_eq!(side_effect_counts(&client).await, side_effects_before);

    let projected = prepare_successful_projection_case(&app, &exposure_context).await;
    let successful_read_side_effects_before = side_effect_counts(&client).await;
    for token in [
        projected.initiator_token.as_str(),
        projected.counterparty_token.as_str(),
    ] {
        for path in [
            format!(
                "/api/projection/promise-views/{}",
                projected.promise_intent_id
            ),
            format!(
                "/api/projection/settlement-views/{}",
                projected.settlement_case_id
            ),
            format!(
                "/api/projection/settlement-views/{}/expanded",
                projected.settlement_case_id
            ),
            format!(
                "/api/projection/room-progression-views/{}",
                projected.room_progression_id
            ),
        ] {
            let response = get_http(&app, &path, Some(token)).await;
            assert_eq!(
                response.status,
                StatusCode::OK,
                "existing projection route should remain successful for legitimate participant requests: {path}"
            );
            assert_no_completion_exposure(&response, &exposure_context);
        }
    }

    for (token, account_id) in [
        (
            projected.initiator_token.as_str(),
            projected.initiator_account_id.as_str(),
        ),
        (
            projected.counterparty_token.as_str(),
            projected.counterparty_account_id.as_str(),
        ),
    ] {
        for path in [
            format!("/api/projection/trust-snapshots/{account_id}"),
            format!(
                "/api/projection/realm-trust-snapshots/{}/{}",
                projected.realm_id, account_id
            ),
        ] {
            let response = get_http(&app, &path, Some(token)).await;
            assert_eq!(
                response.status,
                StatusCode::OK,
                "existing trust projection route should remain successful for the represented participant: {path}"
            );
            assert_no_completion_exposure(&response, &exposure_context);
        }
    }

    for path in [
        format!(
            "/api/projection/promise-views/{}",
            projected.promise_intent_id
        ),
        format!(
            "/api/projection/trust-snapshots/{}",
            projected.initiator_account_id
        ),
        format!(
            "/api/projection/realm-trust-snapshots/{}/{}",
            projected.realm_id, projected.initiator_account_id
        ),
        format!(
            "/api/projection/trust-snapshots/{}",
            projected.counterparty_account_id
        ),
        format!(
            "/api/projection/realm-trust-snapshots/{}/{}",
            projected.realm_id, projected.counterparty_account_id
        ),
    ] {
        let unauthenticated = get_http(&app, &path, None).await;
        assert_eq!(
            unauthenticated.status,
            StatusCode::UNAUTHORIZED,
            "unauthenticated seeded projection route must not expose completion display data: {path}"
        );
        assert_no_completion_exposure(&unauthenticated, &exposure_context);

        let outsider_response = get_http(&app, &path, Some(outsider.token.as_str())).await;
        assert_eq!(
            outsider_response.status,
            StatusCode::NOT_FOUND,
            "non-involved Ordinary Account must not observe seeded projection data: {path}"
        );
        assert_no_completion_exposure(&outsider_response, &exposure_context);
    }

    let room_progression_path = format!(
        "/api/projection/room-progression-views/{}",
        projected.room_progression_id
    );
    let unauthenticated_room = get_http(&app, &room_progression_path, None).await;
    assert_eq!(
        unauthenticated_room.status,
        StatusCode::UNAUTHORIZED,
        "unauthenticated room projection route must not expose completion display data: {room_progression_path}"
    );
    assert_no_completion_exposure(&unauthenticated_room, &exposure_context);

    let outsider_room = get_http(&app, &room_progression_path, Some(outsider.token.as_str())).await;
    assert_eq!(
        outsider_room.status,
        StatusCode::NOT_FOUND,
        "non-involved Ordinary Account must not observe room projection data: {room_progression_path}"
    );
    assert_no_completion_exposure(&outsider_room, &exposure_context);

    for path in [
        format!(
            "/api/projection/settlement-views/{}",
            projected.settlement_case_id
        ),
        format!(
            "/api/projection/settlement-views/{}/expanded",
            projected.settlement_case_id
        ),
    ] {
        let unauthenticated = get_http(&app, &path, None).await;
        assert_eq!(
            unauthenticated.status,
            StatusCode::UNAUTHORIZED,
            "unauthenticated settlement projection route must not expose completion display data: {path}"
        );
        assert_no_completion_exposure(&unauthenticated, &exposure_context);

        let outsider_response = get_http(&app, &path, Some(outsider.token.as_str())).await;
        assert_eq!(
            outsider_response.status,
            StatusCode::NOT_FOUND,
            "non-involved Ordinary Account must not observe settlement projection data: {path}"
        );
        assert_no_completion_exposure(&outsider_response, &exposure_context);
    }

    assert_eq!(
        side_effect_counts(&client).await,
        successful_read_side_effects_before
    );
}

async fn test_context() -> (
    musubi_backend::TestState,
    Router,
    DbConfig,
    tokio_postgres::Client,
) {
    let test_state = new_test_state()
        .await
        .expect("test database state should initialize");
    let app = build_app(test_state.state.clone());
    let database_url = std::env::var("MUSUBI_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("integration tests require MUSUBI_TEST_DATABASE_URL or DATABASE_URL to be set");
    let migrations_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .canonicalize()
        .expect("migrations directory should resolve");
    let migrations_dir = migrations_dir
        .to_str()
        .expect("migrations directory should be utf-8")
        .to_owned();
    let config = DbConfig::from_lookup(|name| match name {
        "APP_ENV" => Some("test".to_owned()),
        "DATABASE_URL" => Some(database_url.clone()),
        "MIGRATIONS_DIR" => Some(migrations_dir.clone()),
        "REQUIRE_LATEST_SCHEMA" => Some("true".to_owned()),
        _ => None,
    })
    .expect("test db config should parse");

    let (client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to test database");
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("test database connection error: {error}");
        }
    });

    (test_state, app, config, client)
}

fn unique_idempotency_key(label: &str) -> String {
    format!("{label}-{}", uuid::Uuid::new_v4())
}

async fn record_writer_fact(
    store: &PromiseCompletionWriterFactStore,
    fact: ProposedPromiseCompletionWriterFact,
) -> musubi_backend::services::promise_completion::PromiseCompletionWriterFactSnapshot {
    store
        .record_writer_fact(RecordPromiseCompletionWriterFactInput { fact })
        .await
        .expect("writer fact should persist")
}

async fn record_prior_pending_mutual_acknowledgement(
    store: &PromiseCompletionWriterFactStore,
    idempotency_key: &str,
) -> musubi_backend::services::promise_completion::PromiseCompletionWriterFactSnapshot {
    let mut fact = base_fact(idempotency_key);
    fact.fact_family = PromiseCompletionWriterFactFamily::SourceRouteCandidate;
    fact.previous_completion_state_class = None;
    fact.completion_state_class =
        PromiseCompletionStateClass::CompletionPendingMutualAcknowledgement;
    fact.completed_reference_eligible = false;
    fact.fact_idempotency_key = Some(format!("prior-{idempotency_key}"));
    fact.reason_code_class = Some(format!("completion-pending-mutual-ack-{idempotency_key}"));
    record_writer_fact(store, fact).await
}

fn accepted_transition_fact(
    idempotency_key: &str,
    prior_writer_fact_id: &str,
) -> ProposedPromiseCompletionWriterFact {
    let mut fact = base_fact(idempotency_key);
    fact.fact_family = PromiseCompletionWriterFactFamily::CompletionStateTransition;
    fact.previous_completion_state_class =
        Some(PromiseCompletionStateClass::CompletionPendingMutualAcknowledgement);
    fact.completion_state_class = PromiseCompletionStateClass::CompletionAccepted;
    fact.completed_reference_eligible = true;
    fact.prior_writer_fact_id = Some(prior_writer_fact_id.to_owned());
    fact.fact_idempotency_key = Some(format!(
        "completion-accepted-from-prior-{prior_writer_fact_id}"
    ));
    fact.reason_code_class = Some(format!("completion-accepted-{idempotency_key}"));
    fact
}

fn base_fact(idempotency_key: &str) -> ProposedPromiseCompletionWriterFact {
    ProposedPromiseCompletionWriterFact {
        promise_reference: Some(format!("promise-completion-{idempotency_key}")),
        realm_id: Some(format!("realm-completion-{idempotency_key}")),
        fact_family: PromiseCompletionWriterFactFamily::CompletionStateTransition,
        source_route_class:
            PromiseCompletionSourceRouteClass::MutualAccountableCompletionAcknowledgement,
        previous_completion_state_class: Some(
            PromiseCompletionStateClass::CompletionPendingMutualAcknowledgement,
        ),
        completion_state_class: PromiseCompletionStateClass::CompletionAccepted,
        completed_reference_eligible: true,
        promise_terms_reference: Some(format!("promise-terms-{idempotency_key}")),
        participant_set_reference: Some(format!("participant-set-{idempotency_key}")),
        ordinary_participant_acknowledgement_reference: Some(format!(
            "ordinary-acknowledgement-{idempotency_key}"
        )),
        governed_review_reference: None,
        review_authority_reference: None,
        proof_eligibility_reference: None,
        proof_evidence_writer_fact_reference: None,
        consent_at_formation_reference: Some(format!("consent-formation-{idempotency_key}")),
        consent_at_resolution_reference: Some(format!("consent-resolution-{idempotency_key}")),
        block_withdrawal_state_reference: Some(format!("block-withdrawal-clear-{idempotency_key}")),
        age_assurance_state_reference: Some(format!(
            "age-assurance-adult-eligible-{idempotency_key}"
        )),
        legal_hold_intersection_reference: Some(format!("legal-hold-clear-{idempotency_key}")),
        critical_harm_case_reference: Some(format!("critical-harm-clear-{idempotency_key}")),
        account_lifecycle_reference: Some(format!("account-lifecycle-active-{idempotency_key}")),
        anti_abuse_continuity_reference: Some(format!("anti-abuse-clear-{idempotency_key}")),
        safety_case_reference: Some(format!("safety-case-clear-{idempotency_key}")),
        reason_code_class: Some(format!("completion-accepted-{idempotency_key}")),
        evidence_level_reference: Some(format!("evidence-level-bounded-{idempotency_key}")),
        correction_or_supersession_reference: None,
        prior_writer_fact_id: None,
        policy_version: Some(1),
        fact_idempotency_key: Some(format!("accepted-transition-{idempotency_key}")),
        retention_class_reference: Some("R4 Trust / moderation / case".to_owned()),
        access_audit_reference: Some(format!("access-audit-{idempotency_key}")),
        projection_non_authority_posture: Some(
            PromiseCompletionProjectionNonAuthorityPosture::ProjectionNonAuthoritative,
        ),
        authority_posture: Some(PromiseCompletionAuthorityPosture::WriterTruthOnly),
    }
}

fn transition_input(
    fact: ProposedPromiseCompletionWriterFact,
) -> RecordMutualAcknowledgementAcceptedTransitionInput {
    RecordMutualAcknowledgementAcceptedTransitionInput {
        transition: ProposedMutualAcknowledgementAcceptedTransition { fact },
    }
}

fn candidate_api_paths(context: &ExposureContext) -> Vec<String> {
    vec![
        format!(
            "/api/promise-completion/display-availability/{}?realm_id={}&participant_set_reference={}&ordinary_participant_acknowledgement_reference={}",
            context.promise_reference,
            context.realm_id,
            context.participant_set_reference,
            context.acknowledgement_reference
        ),
        format!(
            "/api/projection/promise-completion-display/{}?realm_id={}",
            context.promise_reference, context.realm_id
        ),
        format!(
            "/api/projection/promise-completions/{}",
            context.promise_reference
        ),
        format!(
            "/api/projection/completed-references/{}",
            context.promise_reference
        ),
        format!(
            "/api/promise-completions/{}?realm_id={}",
            context.promise_reference, context.realm_id
        ),
        format!(
            "/api/promise-completion/display-availability/wrong-promise-{}?realm_id=wrong-realm-{}&participant_set_reference=wrong-participant-set-{}&ordinary_participant_acknowledgement_reference=wrong-acknowledgement-{}",
            context.idempotency_key,
            context.idempotency_key,
            context.idempotency_key,
            context.idempotency_key
        ),
    ]
}

struct SuccessfulProjectionCase {
    promise_intent_id: String,
    settlement_case_id: String,
    realm_id: String,
    initiator_account_id: String,
    initiator_token: String,
    counterparty_account_id: String,
    counterparty_token: String,
    room_progression_id: String,
}

async fn prepare_successful_projection_case(
    app: &Router,
    exposure_context: &ExposureContext,
) -> SuccessfulProjectionCase {
    let suffix = format!("completion-api-non-exposure-{}", uuid::Uuid::new_v4());
    let initiator = sign_in(
        app,
        &format!("pi-user-{suffix}-initiator"),
        &format!("{suffix}-initiator"),
    )
    .await;
    let counterparty = sign_in(
        app,
        &format!("pi-user-{suffix}-counterparty"),
        &format!("{suffix}-counterparty"),
    )
    .await;
    let realm_id = format!("realm-{suffix}");

    let create_promise = post_json(
        app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": format!("promise-intent-{suffix}"),
            "realm_id": realm_id,
            "counterparty_account_id": counterparty.account_id.clone(),
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(create_promise.status, StatusCode::OK);
    assert_no_completion_display_exposure(&create_promise, exposure_context);
    let create_body = create_promise
        .body_json
        .as_ref()
        .expect("create promise response should be json");
    let promise_intent_id = create_body["promise_intent_id"]
        .as_str()
        .expect("promise_intent_id must exist")
        .to_owned();
    let settlement_case_id = create_body["settlement_case_id"]
        .as_str()
        .expect("settlement_case_id must exist")
        .to_owned();

    let drain_open_hold =
        post_json(app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain_open_hold.status, StatusCode::OK);
    assert_no_completion_exposure(&drain_open_hold, exposure_context);
    let payment_id = drain_open_hold
        .body_json
        .as_ref()
        .expect("drain response should be json")["processed_messages"]
        .as_array()
        .expect("processed_messages must be an array")
        .iter()
        .find(|message| message["event_type"] == "OPEN_HOLD_INTENT")
        .and_then(|message| message["provider_submission_id"].as_str())
        .expect("OPEN_HOLD_INTENT should yield a provider_submission_id")
        .to_owned();

    let callback = post_json(
        app,
        "/api/payment/callback",
        None,
        json!({
            "payment_id": payment_id,
            "payer_pi_uid": initiator.pi_uid,
            "amount_minor_units": 10000,
            "currency_code": "PI",
            "txid": format!("pi-tx-{suffix}"),
            "status": "completed"
        }),
    )
    .await;
    assert_eq!(callback.status, StatusCode::OK);
    assert_no_completion_display_exposure(&callback, exposure_context);

    let drain_projection =
        post_json(app, "/api/internal/orchestration/drain", None, json!({})).await;
    assert_eq!(drain_projection.status, StatusCode::OK);
    assert_no_completion_exposure(&drain_projection, exposure_context);

    let create_room = post_json(
        app,
        "/api/internal/room-progressions",
        None,
        json!({
            "realm_id": format!("realm-{suffix}"),
            "participant_account_ids": [
                initiator.account_id.clone(),
                counterparty.account_id.clone()
            ],
            "user_facing_reason_code": "room_created",
            "source_fact_kind": "intent_room_request",
            "source_fact_id": format!("room-progression-source-{suffix}"),
            "source_snapshot_json": {
                "completed_reference_available": true,
                "display_availability": "available",
                "accepted_writer_fact_id": format!("room-source-accepted-{suffix}")
            },
            "request_idempotency_key": format!("room-progression-create-{suffix}")
        }),
    )
    .await;
    assert_eq!(create_room.status, StatusCode::OK);
    assert_no_completion_display_exposure(&create_room, exposure_context);
    let room_progression_id = create_room
        .body_json
        .as_ref()
        .expect("create room response should be json")["room_progression_id"]
        .as_str()
        .expect("room_progression_id must exist")
        .to_owned();

    SuccessfulProjectionCase {
        promise_intent_id,
        settlement_case_id,
        realm_id: format!("realm-{suffix}"),
        initiator_account_id: initiator.account_id,
        initiator_token: initiator.token,
        counterparty_account_id: counterparty.account_id,
        counterparty_token: counterparty.token,
        room_progression_id,
    }
}

async fn writer_fact_count_for_promise(
    client: &tokio_postgres::Client,
    promise_reference: &str,
) -> i64 {
    let row = client
        .query_one(
            "
            SELECT COUNT(*)::bigint AS count
            FROM promise_completion.writer_fact_records
            WHERE promise_reference = $1
            ",
            &[&promise_reference],
        )
        .await
        .expect("writer fact count should load");
    row.get("count")
}

#[derive(Debug, PartialEq, Eq)]
struct SideEffectCounts {
    promise_completion_writer_facts: i64,
    projection_promise_views: i64,
    projection_settlement_views: i64,
    projection_trust_snapshots: i64,
    projection_realm_trust_snapshots: i64,
    projection_room_progression_views: i64,
    social_trust_intake_attempts: i64,
    social_trust_categorical_sources: i64,
    social_trust_categorical_mutations: i64,
    room_progression_tracks: i64,
    room_progression_facts: i64,
    settlement_cases: i64,
    settlement_intents: i64,
    settlement_submissions: i64,
    settlement_observations: i64,
    provider_attempts: i64,
    outbox_events: i64,
    outbox_attempts: i64,
    command_inbox: i64,
}

async fn side_effect_counts(client: &tokio_postgres::Client) -> SideEffectCounts {
    let row = client
        .query_one(
            "
            SELECT
                (SELECT COUNT(*)::bigint FROM promise_completion.writer_fact_records) AS promise_completion_writer_facts,
                (SELECT COUNT(*)::bigint FROM projection.promise_views) AS projection_promise_views,
                (SELECT COUNT(*)::bigint FROM projection.settlement_views) AS projection_settlement_views,
                (SELECT COUNT(*)::bigint FROM projection.trust_snapshots) AS projection_trust_snapshots,
                (SELECT COUNT(*)::bigint FROM projection.realm_trust_snapshots) AS projection_realm_trust_snapshots,
                (SELECT COUNT(*)::bigint FROM projection.room_progression_views) AS projection_room_progression_views,
                (SELECT COUNT(*)::bigint FROM social_trust.proposed_mutation_attempts) AS social_trust_intake_attempts,
                (SELECT COUNT(*)::bigint FROM social_trust.categorical_source_references) AS social_trust_categorical_sources,
                (SELECT COUNT(*)::bigint FROM social_trust.categorical_mutation_facts) AS social_trust_categorical_mutations,
                (SELECT COUNT(*)::bigint FROM dao.room_progression_tracks) AS room_progression_tracks,
                (SELECT COUNT(*)::bigint FROM dao.room_progression_facts) AS room_progression_facts,
                (SELECT COUNT(*)::bigint FROM dao.settlement_cases) AS settlement_cases,
                (SELECT COUNT(*)::bigint FROM dao.settlement_intents) AS settlement_intents,
                (SELECT COUNT(*)::bigint FROM dao.settlement_submissions) AS settlement_submissions,
                (SELECT COUNT(*)::bigint FROM dao.settlement_observations) AS settlement_observations,
                (SELECT COUNT(*)::bigint FROM dao.provider_attempts) AS provider_attempts,
                (SELECT COUNT(*)::bigint FROM outbox.events) AS outbox_events,
                (SELECT COUNT(*)::bigint FROM outbox.outbox_attempts) AS outbox_attempts,
                (SELECT COUNT(*)::bigint FROM outbox.command_inbox) AS command_inbox
            ",
            &[],
        )
        .await
        .expect("side-effect counts should load");

    SideEffectCounts {
        promise_completion_writer_facts: row.get("promise_completion_writer_facts"),
        projection_promise_views: row.get("projection_promise_views"),
        projection_settlement_views: row.get("projection_settlement_views"),
        projection_trust_snapshots: row.get("projection_trust_snapshots"),
        projection_realm_trust_snapshots: row.get("projection_realm_trust_snapshots"),
        projection_room_progression_views: row.get("projection_room_progression_views"),
        social_trust_intake_attempts: row.get("social_trust_intake_attempts"),
        social_trust_categorical_sources: row.get("social_trust_categorical_sources"),
        social_trust_categorical_mutations: row.get("social_trust_categorical_mutations"),
        room_progression_tracks: row.get("room_progression_tracks"),
        room_progression_facts: row.get("room_progression_facts"),
        settlement_cases: row.get("settlement_cases"),
        settlement_intents: row.get("settlement_intents"),
        settlement_submissions: row.get("settlement_submissions"),
        settlement_observations: row.get("settlement_observations"),
        provider_attempts: row.get("provider_attempts"),
        outbox_events: row.get("outbox_events"),
        outbox_attempts: row.get("outbox_attempts"),
        command_inbox: row.get("command_inbox"),
    }
}

#[derive(Debug)]
struct ExposureContext {
    accepted_writer_fact_id: String,
    prior_writer_fact_id: String,
    promise_reference: String,
    realm_id: String,
    promise_terms_reference: String,
    participant_set_reference: String,
    acknowledgement_reference: String,
    idempotency_key: String,
}

fn assert_no_completion_exposure(response: &HttpResponse, context: &ExposureContext) {
    assert_no_completion_display_exposure(response, context);

    for field in [
        "raw_personal_data",
        "raw_evidence",
        "provider_payload",
        "proof_payload",
        "operator_note",
        "review_narrative",
        "legal_hold",
        "critical_harm",
        "child_safety",
        "sensitive_trait",
        "abuse_marker",
        "internal_safety_classification",
        "support_status",
        "payment_state",
        "shame_label",
        "accusation_label",
        "public_trust_label",
        "social_trust",
        "relationship_depth",
        "settlement_progression",
        "room_transition",
        "contact_unlock",
        "discovery_priority",
        "recommendation_boost",
        "mobile_ui_state",
    ] {
        assert!(
            !response.body_text.contains(field),
            "API response must not expose Promise completion display field `{field}`: {:?}",
            response
        );
    }
}

fn assert_no_completion_display_exposure(response: &HttpResponse, context: &ExposureContext) {
    for sensitive_value in [
        context.accepted_writer_fact_id.as_str(),
        context.prior_writer_fact_id.as_str(),
        context.promise_reference.as_str(),
        context.realm_id.as_str(),
        context.promise_terms_reference.as_str(),
        context.participant_set_reference.as_str(),
        context.acknowledgement_reference.as_str(),
        context.idempotency_key.as_str(),
        "mutual_accountable_completion_acknowledgement",
        "completion_accepted",
        "promise_completed_reference",
    ] {
        assert!(
            !response.body_text.contains(sensitive_value),
            "API response must not expose Promise completion display sensitive value `{sensitive_value}`: {:?}",
            response
        );
    }

    for field in [
        "completed_reference_available",
        "display_availability",
        "display_audience",
        "display_meaning",
        "display_class",
        "accepted_writer_fact_id",
        "prior_writer_fact_id",
        "participant_set_reference",
        "ordinary_participant_acknowledgement_reference",
        "fact_idempotency_key",
        "source_route_class",
        "completion_state_class",
    ] {
        assert!(
            !response.body_text.contains(field),
            "API response must not expose Promise completion display field `{field}`: {:?}",
            response
        );
    }
}

#[derive(Debug)]
struct SignedInUser {
    token: String,
    account_id: String,
    pi_uid: String,
}

async fn sign_in(app: &Router, pi_uid: &str, username: &str) -> SignedInUser {
    let response = request_http(
        app,
        "POST",
        "/api/auth/pi",
        None,
        Some(json!({
            "pi_uid": pi_uid,
            "username": username,
            "wallet_address": format!("wallet-{pi_uid}"),
            "access_token": format!("access-token-{pi_uid}")
        })),
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    let body = response.body_json.expect("sign-in response should be json");

    SignedInUser {
        token: body["token"].as_str().expect("token must exist").to_owned(),
        account_id: body["user"]["id"]
            .as_str()
            .expect("user id must exist")
            .to_owned(),
        pi_uid: body["user"]["pi_uid"]
            .as_str()
            .expect("pi_uid must exist")
            .to_owned(),
    }
}

#[derive(Debug)]
struct HttpResponse {
    status: StatusCode,
    body_text: String,
    body_json: Option<Value>,
}

async fn get_http(app: &Router, path: &str, bearer_token: Option<&str>) -> HttpResponse {
    request_http(app, "GET", path, bearer_token, None).await
}

async fn post_json(
    app: &Router,
    path: &str,
    bearer_token: Option<&str>,
    body: Value,
) -> HttpResponse {
    request_http(app, "POST", path, bearer_token, Some(body)).await
}

async fn request_http(
    app: &Router,
    method: &str,
    path: &str,
    bearer_token: Option<&str>,
    body: Option<Value>,
) -> HttpResponse {
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
    let body_text = String::from_utf8(bytes.to_vec()).expect("response body should be utf-8");
    let body_json = if body_text.trim().is_empty() {
        None
    } else {
        serde_json::from_str(&body_text).ok()
    };

    HttpResponse {
        status,
        body_text,
        body_json,
    }
}
