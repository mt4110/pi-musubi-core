use std::path::PathBuf;

use musubi_backend::{
    new_test_state,
    services::promise_completion::{
        PromiseCompletionAuthorityPosture, PromiseCompletionForbiddenSourceRouteClass,
        PromiseCompletionProjectionNonAuthorityPosture, PromiseCompletionSourceRouteClass,
        PromiseCompletionStateClass, PromiseCompletionWriterFactFamily,
        PromiseCompletionWriterFactPersistenceError, PromiseCompletionWriterFactReplayStatus,
        PromiseCompletionWriterFactStore, ProposedPromiseCompletionWriterFact,
        RecordPromiseCompletionWriterFactInput,
    },
};
use musubi_db_runtime::DbConfig;
use tokio_postgres::NoTls;

fn assert_append_only_error(error: &tokio_postgres::Error) {
    let db_error = error
        .as_db_error()
        .expect("append-only guard should return a database error");
    assert_eq!(
        db_error.message(),
        "promise_completion.writer_fact_records is append-only"
    );
}

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
async fn mutual_acknowledgement_writer_fact_persists_once_and_replays_identically() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("mutual-replay");
    let input = record_input(complete_fact(
        &idempotency_key,
        PromiseCompletionSourceRouteClass::MutualAccountableCompletionAcknowledgement,
        PromiseCompletionStateClass::CompletionAccepted,
    ));

    let first = store
        .record_writer_fact(input.clone())
        .await
        .expect("first mutual acknowledgement writer fact should persist");
    let replay = store
        .record_writer_fact(input)
        .await
        .expect("identical mutual acknowledgement duplicate should replay");

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
        writer_fact_count_for_promise(&client, &first.promise_reference).await,
        1
    );
}

#[tokio::test]
async fn governed_review_writer_fact_persists_once_and_replays_identically() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("governed-replay");
    let input = record_input(complete_fact(
        &idempotency_key,
        PromiseCompletionSourceRouteClass::GovernedReviewCompletion,
        PromiseCompletionStateClass::CompletionAccepted,
    ));

    let first = store
        .record_writer_fact(input.clone())
        .await
        .expect("first governed review writer fact should persist");
    let replay = store
        .record_writer_fact(input)
        .await
        .expect("identical governed review duplicate should replay");

    assert_eq!(first.source_route_class, "governed_review_completion");
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
        writer_fact_count_for_promise(&client, &first.promise_reference).await,
        1
    );
}

#[tokio::test]
async fn duplicate_delivery_with_payload_drift_fails_closed() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("payload-drift");
    let first = record_input(complete_fact(
        &idempotency_key,
        PromiseCompletionSourceRouteClass::MutualAccountableCompletionAcknowledgement,
        PromiseCompletionStateClass::CompletionAccepted,
    ));
    let mut drifted = complete_fact(
        &idempotency_key,
        PromiseCompletionSourceRouteClass::MutualAccountableCompletionAcknowledgement,
        PromiseCompletionStateClass::CompletionAccepted,
    );
    drifted.reason_code_class = Some("completion_accepted_after_drift".to_owned());

    let snapshot = store
        .record_writer_fact(first)
        .await
        .expect("first writer fact should persist");
    let error = store
        .record_writer_fact(record_input(drifted))
        .await
        .expect_err("payload drift must fail closed");

    match error {
        PromiseCompletionWriterFactPersistenceError::IdempotencyConflict { .. } => {}
        other => panic!("expected idempotency conflict, got {other:?}"),
    }
    assert_eq!(
        writer_fact_count_for_promise(&client, &snapshot.promise_reference).await,
        1
    );
}

#[tokio::test]
async fn completed_reference_eligible_true_fails_unless_completion_is_accepted() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("eligible-rejected-state");
    let mut fact = complete_fact(
        &idempotency_key,
        PromiseCompletionSourceRouteClass::MutualAccountableCompletionAcknowledgement,
        PromiseCompletionStateClass::CompletionRejected,
    );
    fact.completed_reference_eligible = true;
    let promise_reference = fact.promise_reference.clone().expect("promise reference");

    let error = store
        .record_writer_fact(record_input(fact))
        .await
        .expect_err("completed reference eligibility must fail for non-accepted state");

    assert!(matches!(
        error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
    assert_eq!(
        writer_fact_count_for_promise(&client, &promise_reference).await,
        0
    );
}

#[tokio::test]
async fn forbidden_source_route_classes_fail_before_persistence() {
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
        let fact = complete_fact(
            &idempotency_key,
            PromiseCompletionSourceRouteClass::Forbidden(forbidden_route),
            PromiseCompletionStateClass::CompletionAccepted,
        );
        let promise_reference = fact.promise_reference.clone().expect("promise reference");
        let error = store
            .record_writer_fact(record_input(fact))
            .await
            .expect_err("forbidden source route should fail closed");

        assert!(matches!(
            error,
            PromiseCompletionWriterFactPersistenceError::BadRequest(_)
        ));
        assert_eq!(
            writer_fact_count_for_promise(&client, &promise_reference).await,
            0
        );
    }
}

#[tokio::test]
async fn missing_required_boundary_references_fail_before_persistence() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");

    let cases: &[(&str, fn(&mut ProposedPromiseCompletionWriterFact))] = &[
        (
            "missing-consent-formation",
            |fact: &mut ProposedPromiseCompletionWriterFact| {
                fact.consent_at_formation_reference = None;
            },
        ),
        (
            "missing-mutual-ordinary-ack",
            |fact: &mut ProposedPromiseCompletionWriterFact| {
                fact.ordinary_participant_acknowledgement_reference = None;
            },
        ),
        (
            "missing-governed-review",
            |fact: &mut ProposedPromiseCompletionWriterFact| {
                fact.source_route_class =
                    PromiseCompletionSourceRouteClass::GovernedReviewCompletion;
                fact.previous_completion_state_class =
                    Some(PromiseCompletionStateClass::CompletionUnderGovernedReview);
                fact.ordinary_participant_acknowledgement_reference = None;
                fact.governed_review_reference = None;
                fact.review_authority_reference =
                    Some("review-authority-missing-governed-review".to_owned());
            },
        ),
        (
            "one-sided-proof-reference",
            |fact: &mut ProposedPromiseCompletionWriterFact| {
                fact.proof_eligibility_reference =
                    Some("proof-eligibility-one-sided-proof-reference".to_owned());
                fact.proof_evidence_writer_fact_reference = None;
            },
        ),
    ];

    for &(suffix, mutation) in cases {
        let idempotency_key = unique_idempotency_key(suffix);
        let mut fact = complete_fact(
            &idempotency_key,
            PromiseCompletionSourceRouteClass::MutualAccountableCompletionAcknowledgement,
            PromiseCompletionStateClass::CompletionAccepted,
        );
        mutation(&mut fact);
        let promise_reference = fact.promise_reference.clone().expect("promise reference");

        let error = store
            .record_writer_fact(record_input(fact))
            .await
            .expect_err("missing required boundary reference should fail closed");

        assert!(matches!(
            error,
            PromiseCompletionWriterFactPersistenceError::BadRequest(_)
        ));
        assert_eq!(
            writer_fact_count_for_promise(&client, &promise_reference).await,
            0
        );
    }
}

#[tokio::test]
async fn persistence_creates_no_projection_trust_depth_settlement_room_or_coordination_effects() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let before = side_effect_counts(&client).await;
    let idempotency_key = unique_idempotency_key("side-effect-free");
    let input = record_input(complete_fact(
        &idempotency_key,
        PromiseCompletionSourceRouteClass::GovernedReviewCompletion,
        PromiseCompletionStateClass::CompletionAccepted,
    ));

    let snapshot = store
        .record_writer_fact(input)
        .await
        .expect("writer fact should persist without product side effects");
    let after = side_effect_counts(&client).await;

    assert_eq!(before, after);
    assert_eq!(
        writer_fact_count_for_promise(&client, &snapshot.promise_reference).await,
        1
    );
}

#[tokio::test]
async fn writer_fact_records_reject_update_and_delete() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("append-only");
    let snapshot = store
        .record_writer_fact(record_input(complete_fact(
            &idempotency_key,
            PromiseCompletionSourceRouteClass::MutualAccountableCompletionAcknowledgement,
            PromiseCompletionStateClass::CompletionAccepted,
        )))
        .await
        .expect("writer fact should persist");

    let update_error = client
        .execute(
            "
            UPDATE promise_completion.writer_fact_records
            SET reason_code_class = 'mutated'
            WHERE writer_fact_id = $1
            ",
            &[&snapshot.writer_fact_id.parse::<uuid::Uuid>().expect("uuid")],
        )
        .await
        .expect_err("writer fact updates must be rejected by the database");
    assert_append_only_error(&update_error);

    let delete_error = client
        .execute(
            "
            DELETE FROM promise_completion.writer_fact_records
            WHERE writer_fact_id = $1
            ",
            &[&snapshot.writer_fact_id.parse::<uuid::Uuid>().expect("uuid")],
        )
        .await
        .expect_err("writer fact deletes must be rejected by the database");
    assert_append_only_error(&delete_error);
    assert_eq!(
        writer_fact_count_for_promise(&client, &snapshot.promise_reference).await,
        1
    );
}

#[tokio::test]
async fn promise_completion_schema_is_narrow_and_contains_no_raw_payload_or_product_effect_columns()
{
    let (_test_state, _config, client) = test_context().await;

    let table_row = client
        .query_one(
            "
            SELECT COUNT(*)::bigint AS count
            FROM information_schema.tables
            WHERE table_schema = 'promise_completion'
              AND table_name <> 'writer_fact_records'
            ",
            &[],
        )
        .await
        .expect("schema table guard should run");
    let extra_table_count: i64 = table_row.get("count");

    let column_row = client
        .query_one(
            "
            SELECT COUNT(*)::bigint AS count
            FROM information_schema.columns
            WHERE table_schema = 'promise_completion'
              AND (
                  column_name LIKE '%score%'
                  OR column_name LIKE '%weight%'
                  OR column_name LIKE '%rank%'
                  OR column_name LIKE '%display%'
                  OR column_name LIKE '%amount%'
                  OR column_name LIKE '%settlement%'
                  OR column_name LIKE '%relationship_depth%'
                  OR column_name LIKE '%room%'
                  OR column_name LIKE '%direct_message%'
                  OR column_name LIKE '%recommendation%'
                  OR column_name LIKE '%discovery%'
                  OR column_name IN (
                      'raw_personal_data',
                      'raw_evidence_payload',
                      'provider_payload',
                      'provider_callback_payload',
                      'api_route',
                      'ui_state',
                      'outbox_event_id',
                      'inbox_entry_id'
                  )
              )
            ",
            &[],
        )
        .await
        .expect("schema column guard should run");
    let forbidden_column_count: i64 = column_row.get("count");

    assert_eq!(extra_table_count, 0);
    assert_eq!(forbidden_column_count, 0);

    let dedupe_row = client
        .query_one(
            "
            SELECT COUNT(*)::bigint AS count
            FROM pg_indexes
            WHERE schemaname = 'promise_completion'
              AND tablename = 'writer_fact_records'
              AND indexname = 'promise_completion_writer_fact_dedupe_unique'
              AND indexdef LIKE '%UNIQUE%'
              AND indexdef LIKE '%realm_id%'
              AND indexdef LIKE '%promise_reference%'
              AND indexdef LIKE '%policy_version%'
              AND indexdef LIKE '%fact_idempotency_key%'
            ",
            &[],
        )
        .await
        .expect("schema idempotency guard should run");
    let dedupe_index_count: i64 = dedupe_row.get("count");

    assert_eq!(dedupe_index_count, 1);
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

fn complete_fact(
    idempotency_key: &str,
    source_route: PromiseCompletionSourceRouteClass,
    completion_state: PromiseCompletionStateClass,
) -> ProposedPromiseCompletionWriterFact {
    let is_governed = source_route == PromiseCompletionSourceRouteClass::GovernedReviewCompletion;
    ProposedPromiseCompletionWriterFact {
        promise_reference: Some(format!("promise-completion-{idempotency_key}")),
        realm_id: Some(format!("realm-completion-{idempotency_key}")),
        fact_family: PromiseCompletionWriterFactFamily::CompletionOutcomeReference,
        source_route_class: source_route,
        previous_completion_state_class: Some(if is_governed {
            PromiseCompletionStateClass::CompletionUnderGovernedReview
        } else {
            PromiseCompletionStateClass::CompletionPendingMutualAcknowledgement
        }),
        completion_state_class: completion_state,
        completed_reference_eligible: completion_state
            == PromiseCompletionStateClass::CompletionAccepted,
        promise_terms_reference: Some(format!("promise-terms-{idempotency_key}")),
        participant_set_reference: Some(format!("participant-set-{idempotency_key}")),
        ordinary_participant_acknowledgement_reference: if is_governed {
            None
        } else {
            Some(format!("ordinary-acknowledgement-{idempotency_key}"))
        },
        governed_review_reference: if is_governed {
            Some(format!("governed-review-{idempotency_key}"))
        } else {
            None
        },
        review_authority_reference: if is_governed {
            Some(format!("review-authority-{idempotency_key}"))
        } else {
            None
        },
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
        fact_idempotency_key: Some(idempotency_key.to_owned()),
        retention_class_reference: Some("R4 Trust / moderation / case".to_owned()),
        access_audit_reference: Some(format!("access-audit-{idempotency_key}")),
        projection_non_authority_posture: Some(
            PromiseCompletionProjectionNonAuthorityPosture::ProjectionNonAuthoritative,
        ),
        authority_posture: Some(PromiseCompletionAuthorityPosture::WriterTruthOnly),
    }
}

fn record_input(
    fact: ProposedPromiseCompletionWriterFact,
) -> RecordPromiseCompletionWriterFactInput {
    RecordPromiseCompletionWriterFactInput { fact }
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
