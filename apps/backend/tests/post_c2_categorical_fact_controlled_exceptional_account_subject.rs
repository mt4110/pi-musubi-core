use std::path::PathBuf;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use musubi_backend::{
    build_app, new_test_state,
    services::social_trust_mutation::{
        RecordC2BoundedPromiseReliabilityMutationFactInput, SocialTrustMutationPersistenceError,
        SocialTrustMutationStore,
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
use tokio_postgres::NoTls;
use tower::ServiceExt;
use uuid::Uuid;

const REALM_REFERENCE: &str = "realm-reference-post-c2-controlled-exceptional-account-subject";

#[tokio::test]
async fn active_controlled_exceptional_account_subject_fails_closed_without_side_effects() {
    let (_test_state, app, config, client) = test_context().await;
    let subject_account_id =
        insert_account(&client, "Controlled Exceptional Account", "active").await;
    assert_active_controlled_exceptional_account(&client, &subject_account_id).await;
    let before_coordination = coordination_counts(&client).await;
    let before_runtime = runtime_surface_counts(&client).await;
    let store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("social trust mutation store should connect");

    let error = store
        .record_c2_bounded_promise_reliability_fact(record_input(
            subject_account_id,
            complete_proposal("post-c2-controlled-exceptional-account-subject"),
        ))
        .await
        .expect_err("Controlled Exceptional Account subject must fail closed");

    assert_controlled_exceptional_subject_rejected(error);
    assert_no_writer_projection_or_coordination_side_effects(
        &client,
        &subject_account_id,
        &before_coordination,
    )
    .await;
    assert_eq!(runtime_surface_counts(&client).await, before_runtime);
    assert_public_projection_not_visible(&app, &subject_account_id).await;
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

fn assert_controlled_exceptional_subject_rejected(error: SocialTrustMutationPersistenceError) {
    match error {
        SocialTrustMutationPersistenceError::BadRequest(message) => {
            assert!(
                message.contains("must be an Ordinary Account"),
                "Controlled Exceptional Account subject rejection should name the Ordinary Account boundary; got {message:?}"
            );
        }
        other => panic!("expected Ordinary Account boundary rejection, got {other:?}"),
    }
}

async fn insert_account(
    client: &tokio_postgres::Client,
    account_class: &str,
    account_state: &str,
) -> Uuid {
    let account_id = Uuid::new_v4();
    client
        .execute(
            "
            INSERT INTO core.accounts (account_id, account_class, account_state)
            VALUES ($1, $2, $3)
            ",
            &[&account_id, &account_class, &account_state],
        )
        .await
        .expect("account insert should succeed");
    account_id
}

async fn assert_active_controlled_exceptional_account(
    client: &tokio_postgres::Client,
    account_id: &Uuid,
) {
    let row = client
        .query_one(
            "
            SELECT account_class, account_state
            FROM core.accounts
            WHERE account_id = $1
            ",
            &[account_id],
        )
        .await
        .expect("account classification should load");

    assert_eq!(
        row.get::<_, String>("account_class"),
        "Controlled Exceptional Account"
    );
    assert_eq!(row.get::<_, String>("account_state"), "active");
}

async fn assert_no_writer_projection_or_coordination_side_effects(
    client: &tokio_postgres::Client,
    subject_account_id: &Uuid,
    before_coordination: &CoordinationCounts,
) {
    assert_eq!(
        source_count_for_subject(client, subject_account_id).await,
        0
    );
    assert_eq!(
        mutation_count_for_subject(client, subject_account_id).await,
        0
    );
    assert_projection_absent_for_subject(client, subject_account_id).await;
    assert_eq!(&coordination_counts(client).await, before_coordination);
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
    recovery_runs: i64,
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
                (SELECT COUNT(*)::bigint FROM outbox.recovery_runs) AS recovery_runs,
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
        recovery_runs: row.get("recovery_runs"),
        outbox_event_archive: row.get("outbox_event_archive"),
        outbox_attempt_archive: row.get("outbox_attempt_archive"),
        command_inbox_archive: row.get("command_inbox_archive"),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct RuntimeSurfaceCounts {
    promise_intents: i64,
    promise_intent_idempotency_keys: i64,
    settlement_cases: i64,
    settlement_intents: i64,
    settlement_submissions: i64,
    provider_attempts: i64,
    settlement_observations: i64,
    journal_entries: i64,
    account_postings: i64,
    promise_views: i64,
    settlement_views: i64,
    room_progression_tracks: i64,
    room_progression_facts: i64,
    room_progression_views: i64,
    proposed_mutation_attempts: i64,
    intake_decisions: i64,
}

async fn runtime_surface_counts(client: &tokio_postgres::Client) -> RuntimeSurfaceCounts {
    let row = client
        .query_one(
            "
            SELECT
                (SELECT COUNT(*)::bigint FROM dao.promise_intents) AS promise_intents,
                (SELECT COUNT(*)::bigint FROM dao.promise_intent_idempotency_keys) AS promise_intent_idempotency_keys,
                (SELECT COUNT(*)::bigint FROM dao.settlement_cases) AS settlement_cases,
                (SELECT COUNT(*)::bigint FROM dao.settlement_intents) AS settlement_intents,
                (SELECT COUNT(*)::bigint FROM dao.settlement_submissions) AS settlement_submissions,
                (SELECT COUNT(*)::bigint FROM dao.provider_attempts) AS provider_attempts,
                (SELECT COUNT(*)::bigint FROM dao.settlement_observations) AS settlement_observations,
                (SELECT COUNT(*)::bigint FROM ledger.journal_entries) AS journal_entries,
                (SELECT COUNT(*)::bigint FROM ledger.account_postings) AS account_postings,
                (SELECT COUNT(*)::bigint FROM projection.promise_views) AS promise_views,
                (SELECT COUNT(*)::bigint FROM projection.settlement_views) AS settlement_views,
                (SELECT COUNT(*)::bigint FROM dao.room_progression_tracks) AS room_progression_tracks,
                (SELECT COUNT(*)::bigint FROM dao.room_progression_facts) AS room_progression_facts,
                (SELECT COUNT(*)::bigint FROM projection.room_progression_views) AS room_progression_views,
                (SELECT COUNT(*)::bigint FROM social_trust.proposed_mutation_attempts) AS proposed_mutation_attempts,
                (SELECT COUNT(*)::bigint FROM social_trust.intake_decisions) AS intake_decisions
            ",
            &[],
        )
        .await
        .expect("runtime surface counts should load");

    RuntimeSurfaceCounts {
        promise_intents: row.get("promise_intents"),
        promise_intent_idempotency_keys: row.get("promise_intent_idempotency_keys"),
        settlement_cases: row.get("settlement_cases"),
        settlement_intents: row.get("settlement_intents"),
        settlement_submissions: row.get("settlement_submissions"),
        provider_attempts: row.get("provider_attempts"),
        settlement_observations: row.get("settlement_observations"),
        journal_entries: row.get("journal_entries"),
        account_postings: row.get("account_postings"),
        promise_views: row.get("promise_views"),
        settlement_views: row.get("settlement_views"),
        room_progression_tracks: row.get("room_progression_tracks"),
        room_progression_facts: row.get("room_progression_facts"),
        room_progression_views: row.get("room_progression_views"),
        proposed_mutation_attempts: row.get("proposed_mutation_attempts"),
        intake_decisions: row.get("intake_decisions"),
    }
}

async fn assert_public_projection_not_visible(app: &Router, subject_account_id: &Uuid) {
    let global = get_json(
        app,
        &format!("/api/projection/trust-snapshots/{subject_account_id}"),
        None,
    )
    .await;
    assert_ne!(global.status, StatusCode::OK);
    assert_no_trust_or_lifecycle_exposure_fields(&global.body);

    let realm = get_json(
        app,
        &format!("/api/projection/realm-trust-snapshots/{REALM_REFERENCE}/{subject_account_id}"),
        None,
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
        "C2 Controlled Exceptional Account subject boundary must not expose score/display/projection/Relationship Depth columns: {:?}",
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
            "C2 Controlled Exceptional Account subject boundary must not expose {field} in public API response"
        );
    }
}

struct JsonResponse {
    status: StatusCode,
    body: Value,
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
