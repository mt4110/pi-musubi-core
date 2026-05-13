use std::path::PathBuf;

use musubi_backend::{
    new_test_state,
    services::social_trust_mutation::{
        C2BoundedPromiseReliabilityReplayStatus,
        RecordC2BoundedPromiseReliabilityMutationFactInput, SocialTrustMutationPersistenceError,
        SocialTrustMutationPersistenceOutcome, SocialTrustMutationStore,
    },
};
use musubi_db_runtime::DbConfig;
use musubi_social_trust_domain::{
    AccountLifecycleReference, AgeAssuranceStateReference, AntiAbuseContinuityReference,
    AuditReference, BlockWithdrawalStateReference, C2BoundedPromiseReliabilityBoundaryIntersection,
    C2BoundedPromiseReliabilityBoundaryPosture, C2BoundedPromiseReliabilityFactIdempotencyKey,
    C2BoundedPromiseReliabilityMutationFact, C2BoundedPromiseReliabilityMutationFactCandidate,
    C2BoundedPromiseReliabilityRejection, C2BoundedPromiseReliabilitySourceFact,
    C2BoundedPromiseReliabilitySourceFactCandidate, ConsentStateReference,
    CriticalHarmCaseReference, EvidenceLevelReference, EvidencePosture,
    LegalHoldIntersectionReference, PromiseReference, PromiseTermsReference,
    ProposedC2BoundedPromiseReliabilityMutationFact, ReasonFactReference,
    RejectedC2BoundedPromiseReliabilitySourceFact, RetentionClassReference, RetentionPosture,
    ReviewabilityPosture, SafetyCaseReference, SocialTrustAuthorityPosture, WriterSourceReference,
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
async fn accepted_positive_fact_is_persisted_and_replayed_without_projection_refresh() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("store should connect");
    let input = record_input(
        subject_account_id,
        complete_proposal(
            "dedupe-positive",
            C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
        ),
    );

    let first = recorded(
        store
            .record_c2_bounded_promise_reliability_fact(input.clone())
            .await
            .expect("first categorical fact should persist"),
    );
    let replay = recorded(
        store
            .record_c2_bounded_promise_reliability_fact(input)
            .await
            .expect("identical duplicate should replay"),
    );

    assert_eq!(
        first.source_fact_label,
        "promise_reliability_outcome.completed_as_agreed"
    );
    assert_eq!(
        first.mutation_fact_label,
        "social_trust_mutation.bounded_promise_reliability_positive"
    );
    assert_eq!(first.mutation_direction, "positive");
    assert_eq!(first.mutation_magnitude, "categorical");
    assert_eq!(
        first.replay_status,
        C2BoundedPromiseReliabilityReplayStatus::Inserted
    );
    assert_eq!(
        replay.replay_status,
        C2BoundedPromiseReliabilityReplayStatus::ReplayedIdentical
    );
    assert_eq!(first.source_reference_id, replay.source_reference_id);
    assert_eq!(first.mutation_fact_id, replay.mutation_fact_id);
    assert_eq!(first.request_payload_hash, replay.request_payload_hash);
    assert_eq!(
        source_count_for_subject(&client, &subject_account_id).await,
        1
    );
    assert_eq!(
        mutation_count_for_subject(&client, &subject_account_id).await,
        1
    );
    assert_eq!(
        projection_trust_snapshot_count(&client, &subject_account_id).await,
        0
    );
}

#[tokio::test]
async fn accepted_no_effect_correction_freeze_narrowing_and_recovery_are_categorical_only() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("store should connect");

    for (key, source, mutation, direction, magnitude) in [
        (
            "dedupe-no-effect",
            C2BoundedPromiseReliabilitySourceFact::ValidExcusedExit,
            C2BoundedPromiseReliabilityMutationFact::NoEffectValidExcusedExit,
            "no_effect",
            "no_effect",
        ),
        (
            "dedupe-correction",
            C2BoundedPromiseReliabilitySourceFact::SourceFactCorrected,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityCorrection,
            "correction",
            "forward_correction",
        ),
        (
            "dedupe-freeze",
            C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze,
            "freeze",
            "temporary_suppression",
        ),
        (
            "dedupe-narrowing",
            C2BoundedPromiseReliabilitySourceFact::SourceScopeLimitedAfterReview,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityNarrowing,
            "narrowing",
            "scope_limited_restriction",
        ),
        (
            "dedupe-recovery",
            C2BoundedPromiseReliabilitySourceFact::FreezeOrNarrowingReversedAfterReview,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityRecovery,
            "recovery",
            "eligibility_restoration",
        ),
    ] {
        let mut proposal = complete_proposal(key, source, mutation);
        if source == C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection {
            proposal.boundary_posture = C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(
                C2BoundedPromiseReliabilityBoundaryIntersection::AppealCorrectionOrSafetyReview,
            );
        }
        let snapshot = recorded(
            store
                .record_c2_bounded_promise_reliability_fact(record_input(
                    subject_account_id,
                    proposal,
                ))
                .await
                .expect("accepted C2 categorical fact should persist"),
        );

        assert_eq!(snapshot.mutation_fact_label, mutation.as_str());
        assert_eq!(snapshot.mutation_direction, direction);
        assert_eq!(snapshot.mutation_magnitude, magnitude);
    }

    assert_eq!(
        source_count_for_subject(&client, &subject_account_id).await,
        5
    );
    assert_eq!(
        mutation_count_for_subject(&client, &subject_account_id).await,
        5
    );
    assert_eq!(
        projection_trust_snapshot_count(&client, &subject_account_id).await,
        0
    );
}

#[tokio::test]
async fn review_required_boundary_intersection_persists_freeze_without_projection_refresh() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("store should connect");
    let mut proposal = complete_proposal(
        "dedupe-freeze-unresolved-legal-hold",
        C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze,
    );
    proposal.boundary_posture = C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(
        C2BoundedPromiseReliabilityBoundaryIntersection::LegalHold,
    );

    let snapshot = recorded(
        store
            .record_c2_bounded_promise_reliability_fact(record_input(subject_account_id, proposal))
            .await
            .expect("review-required boundary source should persist a categorical freeze"),
    );

    assert_eq!(
        snapshot.source_fact_label,
        "promise_reliability_outcome.review_required_boundary_intersection"
    );
    assert_eq!(
        snapshot.mutation_fact_label,
        "social_trust_mutation.bounded_promise_reliability_freeze"
    );
    assert_eq!(snapshot.mutation_direction, "freeze");
    assert_eq!(snapshot.mutation_magnitude, "temporary_suppression");
    assert_eq!(
        boundary_intersection_label(&client, &snapshot.source_reference_id).await,
        Some("legal_hold".to_owned())
    );
    assert_eq!(
        projection_trust_snapshot_count(&client, &subject_account_id).await,
        0
    );
}

#[tokio::test]
async fn boundary_intersection_label_participates_in_duplicate_drift_detection() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("store should connect");
    let mut first_proposal = complete_proposal(
        "dedupe-freeze-boundary-drift",
        C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze,
    );
    first_proposal.boundary_posture = C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(
        C2BoundedPromiseReliabilityBoundaryIntersection::LegalHold,
    );
    let mut drifted_proposal = complete_proposal(
        "dedupe-freeze-boundary-drift",
        C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze,
    );
    drifted_proposal.boundary_posture = C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(
        C2BoundedPromiseReliabilityBoundaryIntersection::CriticalHarm,
    );

    let _ = recorded(
        store
            .record_c2_bounded_promise_reliability_fact(record_input(
                subject_account_id,
                first_proposal,
            ))
            .await
            .expect("first categorical freeze should persist"),
    );
    let error = store
        .record_c2_bounded_promise_reliability_fact(record_input(
            subject_account_id,
            drifted_proposal,
        ))
        .await
        .expect_err("boundary label drift must fail closed");

    match error {
        SocialTrustMutationPersistenceError::IdempotencyConflict { .. } => {}
        other => panic!("expected idempotency conflict, got {other:?}"),
    }
    assert_eq!(
        source_count_for_subject(&client, &subject_account_id).await,
        1
    );
    assert_eq!(
        mutation_count_for_subject(&client, &subject_account_id).await,
        1
    );
}

#[tokio::test]
async fn missing_realm_and_literal_none_realm_do_not_collapse_in_duplicate_drift_detection() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("store should connect");
    let proposal = complete_proposal(
        "dedupe-realm-none-drift",
        C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
    );

    let _ = recorded(
        store
            .record_c2_bounded_promise_reliability_fact(record_input_with_realm(
                subject_account_id,
                Some("none".to_owned()),
                proposal.clone(),
            ))
            .await
            .expect("first categorical fact should persist"),
    );
    let error = store
        .record_c2_bounded_promise_reliability_fact(record_input_with_realm(
            subject_account_id,
            None,
            proposal,
        ))
        .await
        .expect_err("realm presence drift must fail closed");

    match error {
        SocialTrustMutationPersistenceError::IdempotencyConflict { .. } => {}
        other => panic!("expected idempotency conflict, got {other:?}"),
    }
    assert_eq!(
        source_count_for_subject(&client, &subject_account_id).await,
        1
    );
    assert_eq!(
        mutation_count_for_subject(&client, &subject_account_id).await,
        1
    );
}

#[tokio::test]
async fn duplicate_delivery_with_payload_drift_fails_closed() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("store should connect");
    let first = record_input(
        subject_account_id,
        complete_proposal(
            "dedupe-drift",
            C2BoundedPromiseReliabilitySourceFact::CompletedAfterGovernedReview,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
        ),
    );
    let mut drifted_proposal = complete_proposal(
        "dedupe-drift",
        C2BoundedPromiseReliabilitySourceFact::CompletedAfterGovernedReview,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
    );
    drifted_proposal.audit_reference = Some(AuditReference::new("audit-drift"));
    let drifted = record_input(subject_account_id, drifted_proposal);

    let _ = recorded(
        store
            .record_c2_bounded_promise_reliability_fact(first)
            .await
            .expect("first categorical fact should persist"),
    );
    let error = store
        .record_c2_bounded_promise_reliability_fact(drifted)
        .await
        .expect_err("payload drift must fail closed");

    match error {
        SocialTrustMutationPersistenceError::IdempotencyConflict { .. } => {}
        other => panic!("expected idempotency conflict, got {other:?}"),
    }
    assert_eq!(
        source_count_for_subject(&client, &subject_account_id).await,
        1
    );
    assert_eq!(
        mutation_count_for_subject(&client, &subject_account_id).await,
        1
    );
}

#[tokio::test]
async fn identical_replay_survives_subject_account_suspension() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("store should connect");
    let input = record_input(
        subject_account_id,
        complete_proposal(
            "dedupe-after-suspension",
            C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
        ),
    );

    let first = recorded(
        store
            .record_c2_bounded_promise_reliability_fact(input.clone())
            .await
            .expect("first categorical fact should persist"),
    );
    set_account_state(&client, &subject_account_id, "suspended").await;
    let replay = recorded(
        store
            .record_c2_bounded_promise_reliability_fact(input)
            .await
            .expect("duplicate categorical fact should replay despite later account suspension"),
    );

    assert_eq!(first.source_reference_id, replay.source_reference_id);
    assert_eq!(
        replay.replay_status,
        C2BoundedPromiseReliabilityReplayStatus::ReplayedIdentical
    );
}

#[tokio::test]
async fn new_fact_requires_active_ordinary_account() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "suspended").await;
    let store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("store should connect");

    let error = store
        .record_c2_bounded_promise_reliability_fact(record_input(
            subject_account_id,
            complete_proposal(
                "dedupe-suspended-subject",
                C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
                C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
            ),
        ))
        .await
        .expect_err("new categorical fact should reject inactive subjects");

    assert!(matches!(
        error,
        SocialTrustMutationPersistenceError::BadRequest(_)
    ));
    assert_eq!(
        source_count_for_subject(&client, &subject_account_id).await,
        0
    );
    assert_eq!(
        mutation_count_for_subject(&client, &subject_account_id).await,
        0
    );
}

#[tokio::test]
async fn rejected_source_projection_only_and_unresolved_boundary_fail_before_persistence() {
    let (_test_state, config, client) = test_context().await;
    let subject_account_id = insert_account(&client, "Ordinary Account", "active").await;
    let store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("store should connect");

    let mut rejected_source = complete_proposal(
        "dedupe-rejected-promise-creation",
        C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
    );
    rejected_source.source_fact = C2BoundedPromiseReliabilitySourceFactCandidate::Rejected(
        RejectedC2BoundedPromiseReliabilitySourceFact::PromiseCreation,
    );
    assert_rejected_before_persistence(
        store
            .record_c2_bounded_promise_reliability_fact(record_input(
                subject_account_id,
                rejected_source,
            ))
            .await
            .expect("rejected source should fail before persistence"),
        C2BoundedPromiseReliabilityRejection::RejectedSourceFact {
            source: RejectedC2BoundedPromiseReliabilitySourceFact::PromiseCreation,
        },
    );

    let mut projection_only = complete_proposal(
        "dedupe-projection-only",
        C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
    );
    projection_only.authority_posture = SocialTrustAuthorityPosture::ProjectionOnly;
    assert_rejected_before_persistence(
        store
            .record_c2_bounded_promise_reliability_fact(record_input(
                subject_account_id,
                projection_only,
            ))
            .await
            .expect("projection-only posture should fail before persistence"),
        C2BoundedPromiseReliabilityRejection::ProjectionOnlyAuthority,
    );

    let mut unresolved_boundary = complete_proposal(
        "dedupe-unresolved-boundary",
        C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
    );
    unresolved_boundary.boundary_posture = C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(
        C2BoundedPromiseReliabilityBoundaryIntersection::LegalHold,
    );
    assert_rejected_before_persistence(
        store
            .record_c2_bounded_promise_reliability_fact(record_input(
                subject_account_id,
                unresolved_boundary,
            ))
            .await
            .expect("unresolved boundary should fail before persistence"),
        C2BoundedPromiseReliabilityRejection::BoundaryUnresolved {
            boundary: C2BoundedPromiseReliabilityBoundaryIntersection::LegalHold,
        },
    );

    assert_eq!(
        source_count_for_subject(&client, &subject_account_id).await,
        0
    );
    assert_eq!(
        mutation_count_for_subject(&client, &subject_account_id).await,
        0
    );
}

#[tokio::test]
async fn categorical_schema_has_no_scores_or_projection_authority_columns() {
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

fn complete_proposal(
    idempotency_key: &str,
    source: C2BoundedPromiseReliabilitySourceFact,
    mutation: C2BoundedPromiseReliabilityMutationFact,
) -> ProposedC2BoundedPromiseReliabilityMutationFact {
    ProposedC2BoundedPromiseReliabilityMutationFact {
        source_fact: C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(source),
        requested_mutation_fact: C2BoundedPromiseReliabilityMutationFactCandidate::Accepted(
            mutation,
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
    record_input_with_realm(
        subject_account_id,
        Some("realm-reference-1".to_owned()),
        proposal,
    )
}

fn record_input_with_realm(
    subject_account_id: Uuid,
    realm_reference: Option<String>,
    proposal: ProposedC2BoundedPromiseReliabilityMutationFact,
) -> RecordC2BoundedPromiseReliabilityMutationFactInput {
    RecordC2BoundedPromiseReliabilityMutationFactInput {
        subject_account_id: subject_account_id.to_string(),
        realm_reference,
        proposal,
    }
}

fn recorded(
    outcome: SocialTrustMutationPersistenceOutcome,
) -> musubi_backend::services::social_trust_mutation::C2BoundedPromiseReliabilitySnapshot {
    match outcome {
        SocialTrustMutationPersistenceOutcome::Recorded(snapshot) => snapshot,
        SocialTrustMutationPersistenceOutcome::RejectedBeforePersistence { decision } => {
            panic!("expected recorded C2 fact, got rejected before persistence: {decision:?}")
        }
    }
}

fn assert_rejected_before_persistence(
    outcome: SocialTrustMutationPersistenceOutcome,
    expected: C2BoundedPromiseReliabilityRejection,
) {
    match outcome {
        SocialTrustMutationPersistenceOutcome::RejectedBeforePersistence {
            decision:
                musubi_social_trust_domain::C2BoundedPromiseReliabilityMutationDecision::Reject(actual),
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

async fn boundary_intersection_label(
    client: &tokio_postgres::Client,
    source_reference_id: &str,
) -> Option<String> {
    let source_reference_id =
        Uuid::parse_str(source_reference_id).expect("snapshot source reference id should be UUID");
    let row = client
        .query_one(
            "
            SELECT boundary_intersection_label
            FROM social_trust.categorical_source_references
            WHERE source_reference_id = $1
            ",
            &[&source_reference_id],
        )
        .await
        .expect("boundary intersection label should load");
    row.get("boundary_intersection_label")
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
