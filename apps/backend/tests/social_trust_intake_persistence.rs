use std::path::PathBuf;

use musubi_backend::{
    new_test_state,
    services::social_trust_intake::{
        RecordSocialTrustIntakeAttemptInput, SocialTrustIntakePersistenceError,
        SocialTrustIntakePersistenceOutcome, SocialTrustIntakeReplayStatus, SocialTrustIntakeStore,
    },
};
use musubi_db_runtime::DbConfig;
use musubi_social_trust_domain::{
    DurableIdempotencyPosture, EvidencePosture, ForbiddenSocialTrustSourceCategory,
    ProposedSocialTrustMutationAttempt, ReasonFactReference, RetentionClassReference,
    RetentionPosture, ReviewabilityPosture, SocialTrustAuthorityPosture, SocialTrustIntakeDecision,
    SocialTrustIntakeRejection, SocialTrustMutationAttemptIdempotencyKey,
    SocialTrustSourceCategory, WriterSourceReference,
};
use tokio_postgres::NoTls;
use uuid::Uuid;

fn lookup(database_url: &str, migrations_dir: &str, name: &'static str) -> Option<String> {
    match name {
        "APP_ENV" => Some("test".to_owned()),
        "DATABASE_URL" => Some(database_url.to_owned()),
        "MIGRATIONS_DIR" => Some(migrations_dir.to_owned()),
        "REQUIRE_LATEST_SCHEMA" => Some("true".to_owned()),
        _ => None,
    }
}

#[tokio::test]
async fn candidate_attempt_is_persisted_and_replayed_by_database_identity() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustIntakeStore::connect(&config)
        .await
        .expect("store should connect");
    let input = record_input(subject_account_id, complete_attempt("dedupe-candidate"));

    let first = recorded(
        store
            .record_attempt(input.clone())
            .await
            .expect("first intake should persist"),
    );
    let replay = recorded(
        store
            .record_attempt(input)
            .await
            .expect("identical duplicate should replay"),
    );

    assert_eq!(first.decision_kind, "candidate_for_writer_persistence");
    assert_eq!(first.rejection_reason_code, None);
    assert_eq!(first.replay_status, SocialTrustIntakeReplayStatus::Inserted);
    assert_eq!(
        replay.replay_status,
        SocialTrustIntakeReplayStatus::ReplayedIdentical
    );
    assert_eq!(first.attempt_id, replay.attempt_id);
    assert_eq!(first.intake_decision_id, replay.intake_decision_id);
    assert_eq!(first.request_payload_hash, replay.request_payload_hash);
    assert_eq!(
        attempt_count_for_subject(&client, &subject_account_id).await,
        1
    );
}

#[tokio::test]
async fn duplicate_delivery_with_payload_drift_fails_closed() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustIntakeStore::connect(&config)
        .await
        .expect("store should connect");
    let first = record_input(subject_account_id, complete_attempt("dedupe-drift"));
    let mut drifted_attempt = complete_attempt("dedupe-drift");
    drifted_attempt.retention_posture = Some(RetentionPosture::Classified(
        RetentionClassReference::new("social-trust-intake-review-retention"),
    ));
    let drifted = record_input(subject_account_id, drifted_attempt);

    let _ = recorded(
        store
            .record_attempt(first)
            .await
            .expect("first intake should persist"),
    );
    let error = store
        .record_attempt(drifted)
        .await
        .expect_err("payload drift must fail closed");

    match error {
        SocialTrustIntakePersistenceError::IdempotencyConflict { .. } => {}
        other => panic!("expected idempotency conflict, got {other:?}"),
    }
    assert_eq!(
        attempt_count_for_subject(&client, &subject_account_id).await,
        1
    );
}

#[tokio::test]
async fn identical_replay_survives_subject_account_suspension() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustIntakeStore::connect(&config)
        .await
        .expect("store should connect");
    let input = record_input(
        subject_account_id,
        complete_attempt("dedupe-after-suspension"),
    );

    let first = recorded(
        store
            .record_attempt(input.clone())
            .await
            .expect("first intake should persist"),
    );
    set_account_state(&client, &subject_account_id, "suspended").await;
    let replay = recorded(
        store
            .record_attempt(input)
            .await
            .expect("duplicate intake should replay despite later account suspension"),
    );

    assert_eq!(first.attempt_id, replay.attempt_id);
    assert_eq!(
        replay.replay_status,
        SocialTrustIntakeReplayStatus::ReplayedIdentical
    );
}

#[tokio::test]
async fn new_attempt_requires_active_ordinary_account() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "suspended").await;
    let store = SocialTrustIntakeStore::connect(&config)
        .await
        .expect("store should connect");

    let error = store
        .record_attempt(record_input(
            subject_account_id,
            complete_attempt("dedupe-suspended-subject"),
        ))
        .await
        .expect_err("new intake should reject inactive subjects");

    assert!(matches!(
        error,
        SocialTrustIntakePersistenceError::BadRequest(_)
    ));
    assert_eq!(
        attempt_count_for_subject(&client, &subject_account_id).await,
        0
    );
}

#[tokio::test]
async fn forbidden_source_persists_rejected_decision_without_projection_refresh() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustIntakeStore::connect(&config)
        .await
        .expect("store should connect");
    let mut attempt = complete_attempt("dedupe-forbidden-payment");
    attempt.source_category =
        SocialTrustSourceCategory::Forbidden(ForbiddenSocialTrustSourceCategory::PaymentAmount);

    let snapshot = recorded(
        store
            .record_attempt(record_input(subject_account_id, attempt))
            .await
            .expect("forbidden source should persist rejected intake decision"),
    );

    assert_eq!(snapshot.source_category, "payment_amount");
    assert_eq!(snapshot.decision_kind, "rejected");
    assert_eq!(
        snapshot.rejection_reason_code.as_deref(),
        Some("forbidden_source")
    );
    assert_eq!(
        projection_trust_snapshot_count(&client, &subject_account_id).await,
        0
    );
}

#[tokio::test]
async fn projection_only_attempt_persists_rejection_without_projection_refresh() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustIntakeStore::connect(&config)
        .await
        .expect("store should connect");
    let mut attempt = complete_attempt("dedupe-projection-only");
    attempt.authority_posture = SocialTrustAuthorityPosture::ProjectionOnly;

    let snapshot = recorded(
        store
            .record_attempt(record_input(subject_account_id, attempt))
            .await
            .expect("projection-only posture should persist rejected intake decision"),
    );

    assert_eq!(snapshot.decision_kind, "rejected");
    assert_eq!(
        snapshot.rejection_reason_code.as_deref(),
        Some("projection_only_authority")
    );
    assert_eq!(
        projection_trust_snapshot_count(&client, &subject_account_id).await,
        0
    );
}

#[tokio::test]
async fn missing_idempotency_fails_before_persistence() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustIntakeStore::connect(&config)
        .await
        .expect("store should connect");
    let mut attempt = complete_attempt("ignored-missing-idempotency");
    attempt.idempotency_posture = None;

    let outcome = store
        .record_attempt(record_input(subject_account_id, attempt))
        .await
        .expect("missing idempotency should fail closed without db write");

    assert_rejected_before_persistence(
        outcome,
        SocialTrustIntakeRejection::MissingIdempotencyPosture,
    );
    assert_eq!(
        attempt_count_for_subject(&client, &subject_account_id).await,
        0
    );
}

#[tokio::test]
async fn missing_retention_fails_before_persistence() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustIntakeStore::connect(&config)
        .await
        .expect("store should connect");
    let mut attempt = complete_attempt("ignored-missing-retention");
    attempt.retention_posture = None;

    let outcome = store
        .record_attempt(record_input(subject_account_id, attempt))
        .await
        .expect("missing retention should fail closed without db write");

    assert_rejected_before_persistence(
        outcome,
        SocialTrustIntakeRejection::MissingRetentionPosture,
    );
    assert_eq!(
        attempt_count_for_subject(&client, &subject_account_id).await,
        0
    );
}

#[tokio::test]
async fn social_trust_intake_schema_does_not_create_scores_or_depth_tables() {
    let (_test_state, _config, client) = test_context().await;

    let row = client
        .query_one(
            "
            SELECT COUNT(*)::bigint AS count
            FROM information_schema.columns
            WHERE table_schema = 'social_trust'
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
        .expect("schema guard query should run");
    let forbidden_column_count: i64 = row.get("count");

    assert_eq!(forbidden_column_count, 0);
}

async fn test_context() -> (musubi_backend::TestState, DbConfig, tokio_postgres::Client) {
    let test_state = new_test_state()
        .await
        .expect("test database state should initialize");
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
    let config = DbConfig::from_lookup(|name| lookup(&database_url, &migrations_dir, name))
        .expect("test db config should parse");

    let (client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to test database");
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("test database connection error: {error}");
        }
    });

    (test_state, config, client)
}

fn complete_attempt(idempotency_key: &str) -> ProposedSocialTrustMutationAttempt {
    ProposedSocialTrustMutationAttempt {
        source_category: SocialTrustSourceCategory::WriterSourceCandidate,
        writer_source_reference: Some(WriterSourceReference::new("source-fact-1")),
        reason_fact: Some(ReasonFactReference::new("reason-fact-1")),
        idempotency_posture: Some(DurableIdempotencyPosture::DurableDedupeKey(
            SocialTrustMutationAttemptIdempotencyKey::new(idempotency_key),
        )),
        evidence_posture: Some(EvidencePosture::Bounded),
        reviewability_posture: Some(ReviewabilityPosture::Reviewable),
        retention_posture: Some(RetentionPosture::Classified(RetentionClassReference::new(
            "social-trust-intake-retention",
        ))),
        authority_posture: SocialTrustAuthorityPosture::WriterTruthOnly,
    }
}

fn record_input(
    subject_account_id: Uuid,
    attempt: ProposedSocialTrustMutationAttempt,
) -> RecordSocialTrustIntakeAttemptInput {
    RecordSocialTrustIntakeAttemptInput {
        subject_account_id: subject_account_id.to_string(),
        attempt,
    }
}

fn recorded(
    outcome: SocialTrustIntakePersistenceOutcome,
) -> musubi_backend::services::social_trust_intake::SocialTrustIntakeSnapshot {
    match outcome {
        SocialTrustIntakePersistenceOutcome::Recorded(snapshot) => snapshot,
        SocialTrustIntakePersistenceOutcome::RejectedBeforePersistence { decision } => {
            panic!("expected recorded intake, got rejected before persistence: {decision:?}")
        }
    }
}

fn assert_rejected_before_persistence(
    outcome: SocialTrustIntakePersistenceOutcome,
    expected: SocialTrustIntakeRejection,
) {
    match outcome {
        SocialTrustIntakePersistenceOutcome::RejectedBeforePersistence {
            decision: SocialTrustIntakeDecision::Reject(actual),
        } => assert_eq!(actual, expected),
        other => panic!("expected rejected before persistence, got {other:?}"),
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

async fn set_account_state(
    client: &tokio_postgres::Client,
    account_id: &Uuid,
    account_state: &str,
) {
    client
        .execute(
            "
            UPDATE core.accounts
            SET account_state = $2
            WHERE account_id = $1
            ",
            &[account_id, &account_state],
        )
        .await
        .expect("account state update should succeed");
}

async fn attempt_count_for_subject(
    client: &tokio_postgres::Client,
    subject_account_id: &Uuid,
) -> i64 {
    let row = client
        .query_one(
            "
            SELECT COUNT(*)::bigint AS count
            FROM social_trust.proposed_mutation_attempts
            WHERE subject_account_id = $1
            ",
            &[subject_account_id],
        )
        .await
        .expect("attempt count should load");
    row.get("count")
}

async fn projection_trust_snapshot_count(
    client: &tokio_postgres::Client,
    subject_account_id: &Uuid,
) -> i64 {
    let row = client
        .query_one(
            "
            SELECT COUNT(*)::bigint AS count
            FROM projection.trust_snapshots
            WHERE account_id = $1
            ",
            &[subject_account_id],
        )
        .await
        .expect("projection trust snapshot count should load");
    row.get("count")
}
