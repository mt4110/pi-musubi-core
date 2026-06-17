use std::path::PathBuf;

use musubi_backend::{
    new_test_state,
    services::promise_completion::{
        PromiseCompletionAuthorityPosture, PromiseCompletionForbiddenSourceRouteClass,
        PromiseCompletionProjectionNonAuthorityPosture, PromiseCompletionSourceRouteClass,
        PromiseCompletionStateClass, PromiseCompletionWriterFactFamily,
        PromiseCompletionWriterFactPersistenceError, PromiseCompletionWriterFactReplayStatus,
        PromiseCompletionWriterFactStore, ProposedMutualAcknowledgementAcceptedTransition,
        ProposedPromiseCompletionWriterFact, RecordMutualAcknowledgementAcceptedTransitionInput,
        RecordPromiseCompletionWriterFactInput,
    },
};
use musubi_db_runtime::DbConfig;
use tokio_postgres::NoTls;

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
async fn mutual_acknowledgement_accepted_transition_persists_once_and_replays_identically() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("accepted-replay");
    let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;
    let input = transition_input(accepted_transition_fact(
        &idempotency_key,
        &prior.writer_fact_id,
    ));

    let first = store
        .record_mutual_acknowledgement_accepted_transition(input.clone())
        .await
        .expect("first accepted transition should persist");
    let replay = store
        .record_mutual_acknowledgement_accepted_transition(input)
        .await
        .expect("identical accepted transition should replay");

    assert_eq!(first.fact_family, "completion_state_transition");
    assert_eq!(
        first.source_route_class,
        "mutual_accountable_completion_acknowledgement"
    );
    assert_eq!(first.completion_state_class, "completion_accepted");
    assert!(first.completed_reference_eligible);
    assert_eq!(
        first.replay_status,
        PromiseCompletionWriterFactReplayStatus::Inserted
    );
    assert_eq!(
        replay.replay_status,
        PromiseCompletionWriterFactReplayStatus::ReplayedIdentical
    );
    assert_eq!(first.writer_fact_id, replay.writer_fact_id);
    assert_eq!(first.request_payload_hash, replay.request_payload_hash);
    assert_eq!(first.decision_payload_hash, replay.decision_payload_hash);
    assert_eq!(
        writer_fact_count_for_promise_and_family(
            &client,
            &first.promise_reference,
            "completion_state_transition",
        )
        .await,
        1
    );
}

#[tokio::test]
async fn duplicate_accepted_transition_with_payload_drift_fails_closed() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("accepted-drift");
    let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;
    let first = accepted_transition_fact(&idempotency_key, &prior.writer_fact_id);
    let mut drifted = accepted_transition_fact(&idempotency_key, &prior.writer_fact_id);
    drifted.reason_code_class = Some(format!("completion-accepted-drifted-{idempotency_key}"));

    let snapshot = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(first))
        .await
        .expect("first accepted transition should persist");
    let error = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(drifted))
        .await
        .expect_err("payload drift must fail closed");

    match error {
        PromiseCompletionWriterFactPersistenceError::IdempotencyConflict { .. } => {}
        other => panic!("expected idempotency conflict, got {other:?}"),
    }
    assert_eq!(
        writer_fact_count_for_promise_and_family(
            &client,
            &snapshot.promise_reference,
            "completion_state_transition",
        )
        .await,
        1
    );
}

#[tokio::test]
async fn governed_review_completion_is_rejected_before_transition_persistence() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("governed-rejected");
    let mut fact = accepted_transition_fact(&idempotency_key, &uuid::Uuid::new_v4().to_string());
    fact.source_route_class = PromiseCompletionSourceRouteClass::GovernedReviewCompletion;
    fact.governed_review_reference = Some(format!("governed-review-{idempotency_key}"));
    fact.review_authority_reference = Some(format!("review-authority-{idempotency_key}"));
    let promise_reference = fact.promise_reference.clone().expect("promise reference");

    let error = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(fact))
        .await
        .expect_err("governed review completion must be rejected by first candidate helper");

    assert!(matches!(
        error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
    assert_eq!(
        writer_fact_count_for_promise_and_family(
            &client,
            &promise_reference,
            "completion_state_transition",
        )
        .await,
        0
    );
}

#[tokio::test]
async fn forbidden_source_route_classes_fail_before_transition_persistence() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");

    for (suffix, forbidden_route) in [
        (
            "proof-only",
            PromiseCompletionForbiddenSourceRouteClass::ProofOnlyCompletion,
        ),
        (
            "settlement-only",
            PromiseCompletionForbiddenSourceRouteClass::SettlementOnlyCompletion,
        ),
        (
            "payment-only",
            PromiseCompletionForbiddenSourceRouteClass::PaymentOnlyCompletion,
        ),
        (
            "provider-only",
            PromiseCompletionForbiddenSourceRouteClass::ProviderCallbackOnlyCompletion,
        ),
        (
            "operator-note-only",
            PromiseCompletionForbiddenSourceRouteClass::OperatorNoteOnlyCompletion,
        ),
        (
            "projection-only",
            PromiseCompletionForbiddenSourceRouteClass::ProjectionOnlyCompletion,
        ),
        (
            "model-output-only",
            PromiseCompletionForbiddenSourceRouteClass::ModelOutputOnlyCompletion,
        ),
        (
            "venue-staff-judgment-only",
            PromiseCompletionForbiddenSourceRouteClass::VenueStaffJudgmentOnlyCompletion,
        ),
        (
            "client-state-only",
            PromiseCompletionForbiddenSourceRouteClass::ClientStateOnlyCompletion,
        ),
        (
            "support-status",
            PromiseCompletionForbiddenSourceRouteClass::SupportStatusCompletion,
        ),
        (
            "implementation-convenience",
            PromiseCompletionForbiddenSourceRouteClass::ImplementationConvenienceCompletion,
        ),
        (
            "silence-based",
            PromiseCompletionForbiddenSourceRouteClass::SilenceBasedCompletion,
        ),
        (
            "popularity-based",
            PromiseCompletionForbiddenSourceRouteClass::PopularityBasedCompletion,
        ),
    ] {
        let idempotency_key = unique_idempotency_key(suffix);
        let mut fact =
            accepted_transition_fact(&idempotency_key, &uuid::Uuid::new_v4().to_string());
        fact.source_route_class = PromiseCompletionSourceRouteClass::Forbidden(forbidden_route);
        let promise_reference = fact.promise_reference.clone().expect("promise reference");

        let error = store
            .record_mutual_acknowledgement_accepted_transition(transition_input(fact))
            .await
            .expect_err("forbidden source route should fail closed");

        assert!(matches!(
            error,
            PromiseCompletionWriterFactPersistenceError::BadRequest(_)
        ));
        assert_eq!(
            writer_fact_count_for_promise_and_family(
                &client,
                &promise_reference,
                "completion_state_transition",
            )
            .await,
            0
        );
    }
}

#[tokio::test]
async fn non_selected_transition_shapes_fail_before_persistence() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");

    let cases: &[(&str, fn(&mut ProposedPromiseCompletionWriterFact))] = &[
        (
            "wrong-family",
            |fact: &mut ProposedPromiseCompletionWriterFact| {
                fact.fact_family = PromiseCompletionWriterFactFamily::CompletionOutcomeReference;
            },
        ),
        (
            "wrong-previous-state",
            |fact: &mut ProposedPromiseCompletionWriterFact| {
                fact.previous_completion_state_class =
                    Some(PromiseCompletionStateClass::CompletionUnavailable);
            },
        ),
        (
            "wrong-next-state",
            |fact: &mut ProposedPromiseCompletionWriterFact| {
                fact.completion_state_class = PromiseCompletionStateClass::CompletionRejected;
                fact.completed_reference_eligible = false;
            },
        ),
        (
            "non-accepted-eligible",
            |fact: &mut ProposedPromiseCompletionWriterFact| {
                fact.completion_state_class = PromiseCompletionStateClass::CompletionRejected;
                fact.completed_reference_eligible = true;
            },
        ),
    ];

    for &(suffix, mutation) in cases {
        let idempotency_key = unique_idempotency_key(suffix);
        let mut fact =
            accepted_transition_fact(&idempotency_key, &uuid::Uuid::new_v4().to_string());
        mutation(&mut fact);
        let promise_reference = fact.promise_reference.clone().expect("promise reference");

        let error = store
            .record_mutual_acknowledgement_accepted_transition(transition_input(fact))
            .await
            .expect_err("non-selected transition shape should fail closed");

        assert!(matches!(
            error,
            PromiseCompletionWriterFactPersistenceError::BadRequest(_)
        ));
        assert_eq!(
            writer_fact_count_for_promise_and_family(
                &client,
                &promise_reference,
                "completion_state_transition",
            )
            .await,
            0
        );
    }
}

#[tokio::test]
async fn missing_acknowledgement_prior_or_boundary_references_fail_closed() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");

    let missing_ack_key = unique_idempotency_key("missing-ack");
    let mut missing_ack =
        accepted_transition_fact(&missing_ack_key, &uuid::Uuid::new_v4().to_string());
    missing_ack.ordinary_participant_acknowledgement_reference = None;
    let missing_ack_promise = missing_ack
        .promise_reference
        .clone()
        .expect("promise reference");
    let missing_ack_error = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(missing_ack))
        .await
        .expect_err("missing Ordinary Account acknowledgement should fail");
    assert!(matches!(
        missing_ack_error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
    assert_eq!(
        writer_fact_count_for_promise_and_family(
            &client,
            &missing_ack_promise,
            "completion_state_transition",
        )
        .await,
        0
    );

    let missing_prior_key = unique_idempotency_key("missing-prior");
    let mut missing_prior =
        accepted_transition_fact(&missing_prior_key, &uuid::Uuid::new_v4().to_string());
    missing_prior.prior_writer_fact_id = None;
    let missing_prior_promise = missing_prior
        .promise_reference
        .clone()
        .expect("promise reference");
    let missing_prior_error = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(missing_prior))
        .await
        .expect_err("missing prior writer fact reference should fail");
    assert!(matches!(
        missing_prior_error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
    assert_eq!(
        writer_fact_count_for_promise_and_family(
            &client,
            &missing_prior_promise,
            "completion_state_transition",
        )
        .await,
        0
    );

    let missing_boundary_key = unique_idempotency_key("missing-boundary");
    let prior = record_prior_pending_mutual_acknowledgement(&store, &missing_boundary_key).await;
    let mut missing_boundary =
        accepted_transition_fact(&missing_boundary_key, &prior.writer_fact_id);
    missing_boundary.participant_set_reference = None;
    let missing_boundary_error = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(missing_boundary))
        .await
        .expect_err("missing boundary reference should fail");
    assert!(matches!(
        missing_boundary_error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
    assert_eq!(
        writer_fact_count_for_promise_and_family(
            &client,
            &prior.promise_reference,
            "completion_state_transition",
        )
        .await,
        0
    );
}

#[tokio::test]
async fn accepted_transition_requires_matching_prior_writer_truth_posture() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("wrong-prior");
    let wrong_prior = record_prior_with_completion_state(
        &store,
        &idempotency_key,
        PromiseCompletionStateClass::CompletionRejected,
    )
    .await;
    let fact = accepted_transition_fact(&idempotency_key, &wrong_prior.writer_fact_id);

    let error = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(fact))
        .await
        .expect_err("wrong prior writer truth posture should fail");

    assert!(matches!(
        error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
    assert_eq!(
        writer_fact_count_for_promise_and_family(
            &client,
            &wrong_prior.promise_reference,
            "completion_state_transition",
        )
        .await,
        0
    );
}

#[tokio::test]
async fn accepted_transition_requires_matching_prior_boundary_references() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let cases: &[(&str, fn(&mut ProposedPromiseCompletionWriterFact))] = &[
        (
            "drifted-terms",
            |fact: &mut ProposedPromiseCompletionWriterFact| {
                fact.promise_terms_reference = Some("promise-terms-drifted".to_owned());
            },
        ),
        (
            "drifted-participant-set",
            |fact: &mut ProposedPromiseCompletionWriterFact| {
                fact.participant_set_reference = Some("participant-set-drifted".to_owned());
            },
        ),
        (
            "drifted-ordinary-ack",
            |fact: &mut ProposedPromiseCompletionWriterFact| {
                fact.ordinary_participant_acknowledgement_reference =
                    Some("ordinary-acknowledgement-drifted".to_owned());
            },
        ),
    ];

    for &(suffix, mutation) in cases {
        let idempotency_key = unique_idempotency_key(suffix);
        let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;
        let mut fact = accepted_transition_fact(&idempotency_key, &prior.writer_fact_id);
        mutation(&mut fact);

        let error = store
            .record_mutual_acknowledgement_accepted_transition(transition_input(fact))
            .await
            .expect_err("prior boundary reference drift should fail closed");

        assert!(matches!(
            error,
            PromiseCompletionWriterFactPersistenceError::BadRequest(_)
        ));
        assert_eq!(
            writer_fact_count_for_promise_and_family(
                &client,
                &prior.promise_reference,
                "completion_state_transition",
            )
            .await,
            0
        );
    }
}

#[tokio::test]
async fn accepted_transition_creates_no_projection_trust_depth_settlement_or_coordination_effects()
{
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let before = side_effect_counts(&client).await;
    let idempotency_key = unique_idempotency_key("side-effect-free");
    let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;

    let snapshot = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(&idempotency_key, &prior.writer_fact_id),
        ))
        .await
        .expect("accepted transition should persist without product side effects");
    let after = side_effect_counts(&client).await;

    assert_eq!(before, after);
    assert_eq!(
        writer_fact_count_for_promise_and_family(
            &client,
            &snapshot.promise_reference,
            "completion_state_transition",
        )
        .await,
        1
    );
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

fn unique_idempotency_key(label: &str) -> String {
    format!("{label}-{}", uuid::Uuid::new_v4())
}

async fn record_prior_pending_mutual_acknowledgement(
    store: &PromiseCompletionWriterFactStore,
    idempotency_key: &str,
) -> musubi_backend::services::promise_completion::PromiseCompletionWriterFactSnapshot {
    record_prior_with_completion_state(
        store,
        idempotency_key,
        PromiseCompletionStateClass::CompletionPendingMutualAcknowledgement,
    )
    .await
}

async fn record_prior_with_completion_state(
    store: &PromiseCompletionWriterFactStore,
    idempotency_key: &str,
    completion_state: PromiseCompletionStateClass,
) -> musubi_backend::services::promise_completion::PromiseCompletionWriterFactSnapshot {
    store
        .record_writer_fact(RecordPromiseCompletionWriterFactInput {
            fact: prior_mutual_acknowledgement_fact(idempotency_key, completion_state),
        })
        .await
        .expect("prior writer fact should persist")
}

fn prior_mutual_acknowledgement_fact(
    idempotency_key: &str,
    completion_state: PromiseCompletionStateClass,
) -> ProposedPromiseCompletionWriterFact {
    let mut fact = base_fact(idempotency_key);
    fact.fact_family = PromiseCompletionWriterFactFamily::SourceRouteCandidate;
    fact.previous_completion_state_class = None;
    fact.completion_state_class = completion_state;
    fact.completed_reference_eligible = false;
    fact.fact_idempotency_key = Some(format!("prior-{idempotency_key}"));
    fact.reason_code_class = Some(format!("completion-pending-mutual-ack-{idempotency_key}"));
    fact
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
    fact.fact_idempotency_key = Some(format!("accepted-transition-{idempotency_key}"));
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

async fn writer_fact_count_for_promise_and_family(
    client: &tokio_postgres::Client,
    promise_reference: &str,
    fact_family: &str,
) -> i64 {
    let row = client
        .query_one(
            "
            SELECT COUNT(*)::bigint AS count
            FROM promise_completion.writer_fact_records
            WHERE promise_reference = $1
              AND fact_family = $2
            ",
            &[&promise_reference, &fact_family],
        )
        .await
        .expect("writer fact count should load");
    row.get("count")
}

#[derive(Debug, PartialEq, Eq)]
struct SideEffectCounts {
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
