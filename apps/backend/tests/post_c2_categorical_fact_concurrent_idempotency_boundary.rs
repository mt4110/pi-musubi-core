use std::{path::PathBuf, time::Duration};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use musubi_backend::{
    build_app, new_test_state,
    services::social_trust_mutation::{
        C2BoundedPromiseReliabilityReplayStatus, C2BoundedPromiseReliabilitySnapshot,
        RecordC2BoundedPromiseReliabilityMutationFactInput, SocialTrustMutationPersistenceError,
        SocialTrustMutationPersistenceOutcome, SocialTrustMutationStore,
    },
};
use musubi_db_runtime::DbConfig;
use musubi_social_trust_domain::{
    AccountLifecycleReference, AgeAssuranceStateReference, AntiAbuseContinuityReference,
    AuditReference, BlockWithdrawalStateReference, C2BoundedPromiseReliabilityBoundaryPosture,
    C2BoundedPromiseReliabilityFactIdempotencyKey, C2BoundedPromiseReliabilityMutationFact,
    C2BoundedPromiseReliabilityMutationFactCandidate, C2BoundedPromiseReliabilitySourceFact,
    C2BoundedPromiseReliabilitySourceFactCandidate, ConsentStateReference,
    CriticalHarmCaseReference, EvidenceLevelReference, EvidencePosture,
    LegalHoldIntersectionReference, PromiseReference, PromiseTermsReference,
    ProposedC2BoundedPromiseReliabilityMutationFact, ReasonFactReference, RetentionClassReference,
    RetentionPosture, ReviewabilityPosture, SafetyCaseReference, SocialTrustAuthorityPosture,
    WriterSourceReference,
};
use serde_json::{Value, json};
use tokio::{task::yield_now, time::sleep};
use tokio_postgres::NoTls;
use tower::ServiceExt;
use uuid::Uuid;

const REALM_REFERENCE: &str = "realm-reference-post-c2-concurrent-idempotency";

#[tokio::test]
async fn concurrent_identical_c2_delivery_creates_one_writer_fact_and_one_replay() {
    let (_test_state, app, config, client) = test_context().await;
    let subject = sign_in(
        &app,
        "pi-user-post-c2-concurrent-identical",
        "post-c2-concurrent-identical",
    )
    .await;
    let subject_account_id = Uuid::parse_str(&subject.account_id).expect("account id is a UUID");
    let before_coordination = coordination_counts(&client).await;
    let first_store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("first social trust mutation store should connect");
    let second_store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("second social trust mutation store should connect");
    let input = record_input(
        subject_account_id,
        complete_proposal("post-c2-concurrent-identical"),
    );

    let (first, second) = tokio::join!(
        first_store.record_c2_bounded_promise_reliability_fact(input.clone()),
        second_store.record_c2_bounded_promise_reliability_fact(input)
    );
    let snapshots = vec![
        recorded(first.expect("first concurrent delivery should resolve")),
        recorded(second.expect("second concurrent delivery should resolve")),
    ];

    assert_eq!(
        snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.replay_status == C2BoundedPromiseReliabilityReplayStatus::Inserted
            })
            .count(),
        1
    );
    assert_eq!(
        snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.replay_status == C2BoundedPromiseReliabilityReplayStatus::ReplayedIdentical
            })
            .count(),
        1
    );
    assert_same_writer_fact(&snapshots[0], &snapshots[1]);
    assert_eq!(
        source_count_for_subject(&client, &subject_account_id).await,
        1
    );
    assert_eq!(
        mutation_count_for_subject(&client, &subject_account_id).await,
        1
    );
    assert_projection_absent_for_subject(&client, &subject_account_id).await;
    assert_eq!(coordination_counts(&client).await, before_coordination);
    assert_public_projection_not_visible(&app, &subject).await;
    assert_no_score_display_or_relationship_depth_columns(&client).await;
}

#[tokio::test]
async fn concurrent_payload_drift_preserves_first_writer_fact_and_later_identical_replay() {
    let (_test_state, app, config, client) = test_context().await;
    let subject = sign_in(
        &app,
        "pi-user-post-c2-concurrent-drift",
        "post-c2-concurrent-drift",
    )
    .await;
    let subject_account_id = Uuid::parse_str(&subject.account_id).expect("account id is a UUID");
    let before_coordination = coordination_counts(&client).await;
    let first_store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("first social trust mutation store should connect");
    let second_store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("second social trust mutation store should connect");
    let replay_store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("replay social trust mutation store should connect");
    let first_input = record_input(
        subject_account_id,
        complete_proposal("post-c2-concurrent-drift"),
    );
    let mut drifted_proposal = complete_proposal("post-c2-concurrent-drift");
    drifted_proposal.audit_reference = Some(AuditReference::new("audit-concurrent-drift"));
    let drifted_input = record_input(subject_account_id, drifted_proposal);

    let (first, drifted) = tokio::join!(
        first_store.record_c2_bounded_promise_reliability_fact(first_input.clone()),
        async {
            yield_now().await;
            sleep(Duration::from_millis(10)).await;
            second_store
                .record_c2_bounded_promise_reliability_fact(drifted_input)
                .await
        }
    );
    let first_snapshot = recorded(first.expect("first concurrent delivery should persist"));
    assert_eq!(
        first_snapshot.replay_status,
        C2BoundedPromiseReliabilityReplayStatus::Inserted
    );
    assert_idempotency_conflict(
        drifted.expect_err("drifted concurrent delivery must fail closed"),
        &first_snapshot.source_reference_id,
    );
    assert_eq!(
        source_count_for_subject(&client, &subject_account_id).await,
        1
    );
    assert_eq!(
        mutation_count_for_subject(&client, &subject_account_id).await,
        1
    );
    assert_projection_absent_for_subject(&client, &subject_account_id).await;
    assert_eq!(coordination_counts(&client).await, before_coordination);

    let replay = recorded(
        replay_store
            .record_c2_bounded_promise_reliability_fact(first_input)
            .await
            .expect("identical replay should remain possible after rejected drift"),
    );
    assert_eq!(
        replay.replay_status,
        C2BoundedPromiseReliabilityReplayStatus::ReplayedIdentical
    );
    assert_same_writer_fact(&first_snapshot, &replay);
    assert_eq!(
        source_count_for_subject(&client, &subject_account_id).await,
        1
    );
    assert_eq!(
        mutation_count_for_subject(&client, &subject_account_id).await,
        1
    );
    assert_projection_absent_for_subject(&client, &subject_account_id).await;
    assert_eq!(coordination_counts(&client).await, before_coordination);
    assert_public_projection_not_visible(&app, &subject).await;
    assert_no_score_display_or_relationship_depth_columns(&client).await;
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

fn complete_proposal(idempotency_key: &str) -> ProposedC2BoundedPromiseReliabilityMutationFact {
    ProposedC2BoundedPromiseReliabilityMutationFact {
        source_fact: C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(
            C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
        ),
        requested_mutation_fact: C2BoundedPromiseReliabilityMutationFactCandidate::Accepted(
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
        ),
        writer_source_reference: Some(WriterSourceReference::new(format!(
            "writer-source-{idempotency_key}"
        ))),
        promise_reference: Some(PromiseReference::new(format!("promise-{idempotency_key}"))),
        promise_terms_reference: Some(PromiseTermsReference::new(format!(
            "promise-terms-{idempotency_key}"
        ))),
        consent_at_formation_reference: Some(ConsentStateReference::new(format!(
            "consent-formation-{idempotency_key}"
        ))),
        consent_at_resolution_reference: Some(ConsentStateReference::new(format!(
            "consent-resolution-{idempotency_key}"
        ))),
        block_withdrawal_state_reference: Some(BlockWithdrawalStateReference::new(format!(
            "block-withdrawal-clear-{idempotency_key}"
        ))),
        age_assurance_state_reference: Some(AgeAssuranceStateReference::new(format!(
            "age-assurance-adult-eligible-{idempotency_key}"
        ))),
        legal_hold_intersection_reference: Some(LegalHoldIntersectionReference::new(format!(
            "legal-hold-clear-{idempotency_key}"
        ))),
        critical_harm_case_reference: Some(CriticalHarmCaseReference::new(format!(
            "critical-harm-clear-{idempotency_key}"
        ))),
        account_lifecycle_reference: Some(AccountLifecycleReference::new(format!(
            "account-lifecycle-active-{idempotency_key}"
        ))),
        anti_abuse_continuity_reference: Some(AntiAbuseContinuityReference::new(format!(
            "anti-abuse-clear-{idempotency_key}"
        ))),
        safety_case_reference: Some(SafetyCaseReference::new(format!(
            "safety-case-clear-{idempotency_key}"
        ))),
        evidence_level_reference: Some(EvidenceLevelReference::new(format!(
            "evidence-level-bounded-{idempotency_key}"
        ))),
        audit_reference: Some(AuditReference::new(format!("audit-{idempotency_key}"))),
        reason_fact: Some(ReasonFactReference::new(format!(
            "reason-fact-{idempotency_key}"
        ))),
        fact_idempotency_key: Some(C2BoundedPromiseReliabilityFactIdempotencyKey::new(
            idempotency_key,
        )),
        evidence_posture: Some(EvidencePosture::Bounded),
        reviewability_posture: Some(ReviewabilityPosture::Reviewable),
        retention_posture: Some(RetentionPosture::Classified(RetentionClassReference::new(
            "R4 Trust / moderation / case",
        ))),
        authority_posture: SocialTrustAuthorityPosture::WriterTruthOnly,
        boundary_posture: C2BoundedPromiseReliabilityBoundaryPosture::Clear,
    }
}

fn record_input(
    subject_account_id: Uuid,
    proposal: ProposedC2BoundedPromiseReliabilityMutationFact,
) -> RecordC2BoundedPromiseReliabilityMutationFactInput {
    RecordC2BoundedPromiseReliabilityMutationFactInput {
        subject_account_id: subject_account_id.to_string(),
        realm_reference: Some(REALM_REFERENCE.to_owned()),
        proposal,
    }
}

fn recorded(outcome: SocialTrustMutationPersistenceOutcome) -> C2BoundedPromiseReliabilitySnapshot {
    match outcome {
        SocialTrustMutationPersistenceOutcome::Recorded(snapshot) => snapshot,
        SocialTrustMutationPersistenceOutcome::RejectedBeforePersistence { decision } => {
            panic!("expected recorded C2 fact, got rejected before persistence: {decision:?}")
        }
    }
}

fn assert_same_writer_fact(
    first: &C2BoundedPromiseReliabilitySnapshot,
    second: &C2BoundedPromiseReliabilitySnapshot,
) {
    assert_eq!(first.source_reference_id, second.source_reference_id);
    assert_eq!(first.mutation_fact_id, second.mutation_fact_id);
    assert_eq!(first.source_fact_label, second.source_fact_label);
    assert_eq!(first.mutation_fact_label, second.mutation_fact_label);
    assert_eq!(first.request_payload_hash, second.request_payload_hash);
    assert_eq!(first.decision_payload_hash, second.decision_payload_hash);
}

fn assert_idempotency_conflict(
    error: SocialTrustMutationPersistenceError,
    expected_source_reference_id: &str,
) {
    match error {
        SocialTrustMutationPersistenceError::IdempotencyConflict {
            existing_source_reference_id,
            ..
        } => assert_eq!(existing_source_reference_id, expected_source_reference_id),
        other => panic!("expected idempotency conflict, got {other:?}"),
    }
}

async fn source_count_for_subject(
    client: &tokio_postgres::Client,
    subject_account_id: &Uuid,
) -> i64 {
    let row = client
        .query_one(
            "
            SELECT COUNT(*)::bigint AS count
            FROM social_trust.categorical_source_references
            WHERE subject_account_id = $1
            ",
            &[subject_account_id],
        )
        .await
        .expect("source reference count should load");
    row.get("count")
}

async fn mutation_count_for_subject(
    client: &tokio_postgres::Client,
    subject_account_id: &Uuid,
) -> i64 {
    let row = client
        .query_one(
            "
            SELECT COUNT(*)::bigint AS count
            FROM social_trust.categorical_mutation_facts
            WHERE subject_account_id = $1
            ",
            &[subject_account_id],
        )
        .await
        .expect("mutation fact count should load");
    row.get("count")
}

async fn assert_projection_absent_for_subject(
    client: &tokio_postgres::Client,
    subject_account_id: &Uuid,
) {
    let row = client
        .query_one(
            "
            SELECT
                (SELECT COUNT(*)::bigint
                 FROM projection.trust_snapshots
                 WHERE account_id = $1) AS trust_snapshot_count,
                (SELECT COUNT(*)::bigint
                 FROM projection.realm_trust_snapshots
                 WHERE account_id = $1) AS realm_trust_snapshot_count
            ",
            &[subject_account_id],
        )
        .await
        .expect("projection counts should load");

    assert_eq!(row.get::<_, i64>("trust_snapshot_count"), 0);
    assert_eq!(row.get::<_, i64>("realm_trust_snapshot_count"), 0);
}

#[derive(Debug, PartialEq, Eq)]
struct CoordinationCounts {
    outbox_events: i64,
    outbox_attempts: i64,
    command_inbox: i64,
    outbox_event_archive: i64,
    outbox_attempt_archive: i64,
    command_inbox_archive: i64,
}

async fn coordination_counts(client: &tokio_postgres::Client) -> CoordinationCounts {
    let row = client
        .query_one(
            "
            SELECT
                (SELECT COUNT(*)::bigint FROM outbox.events) AS outbox_events,
                (SELECT COUNT(*)::bigint FROM outbox.outbox_attempts) AS outbox_attempts,
                (SELECT COUNT(*)::bigint FROM outbox.command_inbox) AS command_inbox,
                (SELECT COUNT(*)::bigint FROM outbox.outbox_event_archive) AS outbox_event_archive,
                (SELECT COUNT(*)::bigint FROM outbox.outbox_attempt_archive) AS outbox_attempt_archive,
                (SELECT COUNT(*)::bigint FROM outbox.command_inbox_archive) AS command_inbox_archive
            ",
            &[],
        )
        .await
        .expect("coordination counts should load");

    CoordinationCounts {
        outbox_events: row.get("outbox_events"),
        outbox_attempts: row.get("outbox_attempts"),
        command_inbox: row.get("command_inbox"),
        outbox_event_archive: row.get("outbox_event_archive"),
        outbox_attempt_archive: row.get("outbox_attempt_archive"),
        command_inbox_archive: row.get("command_inbox_archive"),
    }
}

async fn assert_public_projection_not_visible(app: &Router, subject: &SignedInUser) {
    let global = get_json(
        app,
        &format!("/api/projection/trust-snapshots/{}", subject.account_id),
        Some(subject.token.as_str()),
    )
    .await;
    assert_ne!(global.status, StatusCode::OK);
    assert_no_trust_or_lifecycle_exposure_fields(&global.body);

    let realm = get_json(
        app,
        &format!(
            "/api/projection/realm-trust-snapshots/{}/{}",
            REALM_REFERENCE, subject.account_id
        ),
        Some(subject.token.as_str()),
    )
    .await;
    assert_ne!(realm.status, StatusCode::OK);
    assert_no_trust_or_lifecycle_exposure_fields(&realm.body);
}

async fn assert_no_score_display_or_relationship_depth_columns(client: &tokio_postgres::Client) {
    let rows = client
        .query(
            "
            SELECT table_name, column_name
            FROM information_schema.columns
            WHERE table_schema = 'social_trust'
              AND table_name IN (
                  'categorical_source_references',
                  'categorical_mutation_facts'
              )
              AND (
                  column_name LIKE '%score%'
                  OR column_name LIKE '%weight%'
                  OR column_name LIKE '%rank%'
                  OR column_name LIKE '%display%'
                  OR column_name LIKE '%relationship_depth%'
                  OR column_name LIKE '%projection%'
              )
            ",
            &[],
        )
        .await
        .expect("social trust column metadata should load");

    assert!(
        rows.is_empty(),
        "C2 concurrent idempotency must not expose score/display/projection/Relationship Depth columns: {:?}",
        rows.iter()
            .map(|row| format!(
                "{}.{}",
                row.get::<_, String>("table_name"),
                row.get::<_, String>("column_name")
            ))
            .collect::<Vec<_>>()
    );
}

fn assert_no_trust_or_lifecycle_exposure_fields(body: &Value) {
    for field in [
        "trust_posture",
        "reason_codes",
        "trust_score",
        "score",
        "rank",
        "trust_rank",
        "trust_tier",
        "display_level",
        "public_level",
        "recovery_ceiling",
        "discovery_priority",
        "recommendation_boost",
        "contact_unlock",
        "room_transition",
        "settlement_progression",
        "promise_runtime_outcome",
        "proof_runtime_outcome",
        "relationship_depth",
        "mobile_ui_state",
        "retention_action",
        "pruning_action",
        "archive_action",
        "deletion_action",
        "legal_hold_action",
        "key_lifecycle_action",
        "retry_action",
        "queue_action",
        "outbox_action",
        "inbox_action",
    ] {
        assert!(
            body.get(field).is_none(),
            "C2 concurrent idempotency must not expose {field} in public API response"
        );
    }
}

struct SignedInUser {
    token: String,
    account_id: String,
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
