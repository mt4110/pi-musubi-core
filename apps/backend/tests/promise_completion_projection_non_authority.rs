use std::path::PathBuf;

use musubi_backend::{
    new_test_state,
    services::promise_completion::{
        PromiseCompletionAuthorityPosture, PromiseCompletionForbiddenSourceRouteClass,
        PromiseCompletionProjectionNonAuthorityPosture, PromiseCompletionSourceRouteClass,
        PromiseCompletionStateClass, PromiseCompletionWriterFactFamily,
        PromiseCompletionWriterFactPersistenceError, PromiseCompletionWriterFactStore,
        ProposedMutualAcknowledgementAcceptedTransition, ProposedPromiseCompletionWriterFact,
        RecordMutualAcknowledgementAcceptedTransitionInput, RecordPromiseCompletionWriterFactInput,
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
async fn accepted_writer_truth_derives_non_authoritative_projection_snapshot() {
    let (_test_state, config, _client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("projection-positive");
    let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;
    let accepted = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(&idempotency_key, &prior.writer_fact_id),
        ))
        .await
        .expect("accepted transition should persist");

    let snapshots = store
        .derive_accepted_completion_non_authority_projection_snapshots(
            &accepted.promise_reference,
            &accepted.realm_id,
        )
        .await
        .expect("projection snapshot should derive from accepted writer truth");

    assert_eq!(snapshots.len(), 1);
    let snapshot = &snapshots[0];
    assert_eq!(snapshot.accepted_writer_fact_id, accepted.writer_fact_id);
    assert_eq!(snapshot.prior_writer_fact_id, prior.writer_fact_id);
    assert_eq!(snapshot.promise_reference, accepted.promise_reference);
    assert_eq!(snapshot.realm_id, accepted.realm_id);
    assert_eq!(
        snapshot.promise_terms_reference,
        format!("promise-terms-{idempotency_key}")
    );
    assert_eq!(
        snapshot.participant_set_reference,
        format!("participant-set-{idempotency_key}")
    );
    assert_eq!(
        snapshot.source_route_class,
        "mutual_accountable_completion_acknowledgement"
    );
    assert_eq!(snapshot.completion_state_class, "completion_accepted");
    assert!(snapshot.completed_reference_eligible);
    assert_eq!(snapshot.policy_version, 1);
    assert_eq!(
        snapshot.projection_non_authority_posture,
        "projection_non_authoritative"
    );
    assert_eq!(snapshot.authority_posture, "writer_truth_only");
    assert_eq!(snapshot.writer_recorded_at, accepted.created_at);
}

#[tokio::test]
async fn missing_required_writer_posture_returns_no_projection_snapshot() {
    let (_test_state, config, _client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");

    let pending_key = unique_idempotency_key("projection-pending-only");
    let pending_prior = record_prior_pending_mutual_acknowledgement(&store, &pending_key).await;
    assert!(
        store
            .derive_accepted_completion_non_authority_projection_snapshots(
                &pending_prior.promise_reference,
                &pending_prior.realm_id,
            )
            .await
            .expect("pending-only posture should be readable")
            .is_empty()
    );

    let ineligible_key = unique_idempotency_key("projection-ineligible");
    let ineligible_prior =
        record_prior_pending_mutual_acknowledgement(&store, &ineligible_key).await;
    let mut ineligible =
        accepted_transition_fact(&ineligible_key, &ineligible_prior.writer_fact_id);
    ineligible.completed_reference_eligible = false;
    let ineligible = record_writer_fact(&store, ineligible).await;
    assert!(
        store
            .derive_accepted_completion_non_authority_projection_snapshots(
                &ineligible.promise_reference,
                &ineligible.realm_id,
            )
            .await
            .expect("ineligible accepted posture should be readable")
            .is_empty()
    );

    let missing_prior_key = unique_idempotency_key("projection-missing-prior");
    let mut missing_prior =
        accepted_transition_fact(&missing_prior_key, &uuid::Uuid::new_v4().to_string());
    missing_prior.prior_writer_fact_id = None;
    missing_prior.fact_idempotency_key = Some(format!("missing-prior-{missing_prior_key}"));
    let missing_prior = record_writer_fact(&store, missing_prior).await;
    assert!(
        store
            .derive_accepted_completion_non_authority_projection_snapshots(
                &missing_prior.promise_reference,
                &missing_prior.realm_id,
            )
            .await
            .expect("missing-prior accepted posture should be readable")
            .is_empty()
    );

    let wrong_prior_key = unique_idempotency_key("projection-wrong-prior");
    let wrong_prior = record_prior_with_completion_state(
        &store,
        &wrong_prior_key,
        PromiseCompletionStateClass::CompletionRejected,
    )
    .await;
    let wrong_prior_accepted = record_writer_fact(
        &store,
        accepted_transition_fact(&wrong_prior_key, &wrong_prior.writer_fact_id),
    )
    .await;
    assert!(
        store
            .derive_accepted_completion_non_authority_projection_snapshots(
                &wrong_prior_accepted.promise_reference,
                &wrong_prior_accepted.realm_id,
            )
            .await
            .expect("wrong-prior accepted posture should be readable")
            .is_empty()
    );

    let unbound_key = unique_idempotency_key("projection-unbound-accepted");
    let unbound_prior = record_prior_pending_mutual_acknowledgement(&store, &unbound_key).await;
    let mut unbound = accepted_transition_fact(&unbound_key, &unbound_prior.writer_fact_id);
    unbound.fact_idempotency_key = Some(format!("unbound-accepted-{unbound_key}"));
    let unbound = record_writer_fact(&store, unbound).await;
    assert!(
        store
            .derive_accepted_completion_non_authority_projection_snapshots(
                &unbound.promise_reference,
                &unbound.realm_id,
            )
            .await
            .expect("unbound accepted posture should be readable")
            .is_empty()
    );
}

#[tokio::test]
async fn governed_review_and_forbidden_routes_do_not_enter_first_projection_route() {
    let (_test_state, config, _client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");

    let governed_key = unique_idempotency_key("projection-governed");
    let governed_prior = record_prior_pending_mutual_acknowledgement(&store, &governed_key).await;
    let mut governed = accepted_transition_fact(&governed_key, &governed_prior.writer_fact_id);
    governed.source_route_class = PromiseCompletionSourceRouteClass::GovernedReviewCompletion;
    governed.governed_review_reference = Some(format!("governed-review-{governed_key}"));
    governed.review_authority_reference = Some(format!("review-authority-{governed_key}"));
    let governed = record_writer_fact(&store, governed).await;
    assert!(
        store
            .derive_accepted_completion_non_authority_projection_snapshots(
                &governed.promise_reference,
                &governed.realm_id,
            )
            .await
            .expect("governed route writer fact should be readable")
            .is_empty()
    );

    let accepted_with_review_key = unique_idempotency_key("projection-accepted-review-fields");
    let accepted_with_review_prior =
        record_prior_pending_mutual_acknowledgement(&store, &accepted_with_review_key).await;
    let mut accepted_with_review = accepted_transition_fact(
        &accepted_with_review_key,
        &accepted_with_review_prior.writer_fact_id,
    );
    accepted_with_review.governed_review_reference =
        Some(format!("governed-review-{accepted_with_review_key}"));
    accepted_with_review.review_authority_reference =
        Some(format!("review-authority-{accepted_with_review_key}"));
    let accepted_with_review = record_writer_fact(&store, accepted_with_review).await;
    assert!(
        store
            .derive_accepted_completion_non_authority_projection_snapshots(
                &accepted_with_review.promise_reference,
                &accepted_with_review.realm_id,
            )
            .await
            .expect("mutual route with accepted governed review fields should be readable")
            .is_empty()
    );

    let prior_with_review_key = unique_idempotency_key("projection-prior-review-fields");
    let mut prior_with_review = prior_mutual_acknowledgement_fact(
        &prior_with_review_key,
        PromiseCompletionStateClass::CompletionPendingMutualAcknowledgement,
    );
    prior_with_review.governed_review_reference =
        Some(format!("governed-review-{prior_with_review_key}"));
    prior_with_review.review_authority_reference =
        Some(format!("review-authority-{prior_with_review_key}"));
    let prior_with_review = record_writer_fact(&store, prior_with_review).await;
    let accepted_after_review_prior = record_writer_fact(
        &store,
        accepted_transition_fact(&prior_with_review_key, &prior_with_review.writer_fact_id),
    )
    .await;
    assert!(
        store
            .derive_accepted_completion_non_authority_projection_snapshots(
                &accepted_after_review_prior.promise_reference,
                &accepted_after_review_prior.realm_id,
            )
            .await
            .expect("mutual route with prior governed review fields should be readable")
            .is_empty()
    );

    let forbidden_key = unique_idempotency_key("projection-forbidden");
    let forbidden_prior = record_prior_pending_mutual_acknowledgement(&store, &forbidden_key).await;
    let mut forbidden = accepted_transition_fact(&forbidden_key, &forbidden_prior.writer_fact_id);
    forbidden.source_route_class = PromiseCompletionSourceRouteClass::Forbidden(
        PromiseCompletionForbiddenSourceRouteClass::ProjectionOnlyCompletion,
    );
    let promise_reference = forbidden
        .promise_reference
        .clone()
        .expect("promise reference");
    let realm_id = forbidden.realm_id.clone().expect("realm_id");

    let error = store
        .record_writer_fact(RecordPromiseCompletionWriterFactInput { fact: forbidden })
        .await
        .expect_err("forbidden source route should fail before persistence");
    assert!(matches!(
        error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
    assert!(
        store
            .derive_accepted_completion_non_authority_projection_snapshots(
                &promise_reference,
                &realm_id,
            )
            .await
            .expect("forbidden route absence should be readable")
            .is_empty()
    );
}

#[tokio::test]
async fn proof_backed_mutual_rows_do_not_enter_first_projection_route() {
    let (_test_state, config, _client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");

    let accepted_with_proof_key = unique_idempotency_key("projection-accepted-proof-fields");
    let accepted_with_proof_prior =
        record_prior_pending_mutual_acknowledgement(&store, &accepted_with_proof_key).await;
    let mut accepted_with_proof = accepted_transition_fact(
        &accepted_with_proof_key,
        &accepted_with_proof_prior.writer_fact_id,
    );
    accepted_with_proof.proof_eligibility_reference =
        Some(format!("proof-eligibility-{accepted_with_proof_key}"));
    accepted_with_proof.proof_evidence_writer_fact_reference =
        Some(format!("proof-evidence-{accepted_with_proof_key}"));
    let accepted_with_proof = record_writer_fact(&store, accepted_with_proof).await;
    assert!(
        store
            .derive_accepted_completion_non_authority_projection_snapshots(
                &accepted_with_proof.promise_reference,
                &accepted_with_proof.realm_id,
            )
            .await
            .expect("mutual route with accepted proof fields should be readable")
            .is_empty()
    );

    let prior_with_proof_key = unique_idempotency_key("projection-prior-proof-fields");
    let mut prior_with_proof = prior_mutual_acknowledgement_fact(
        &prior_with_proof_key,
        PromiseCompletionStateClass::CompletionPendingMutualAcknowledgement,
    );
    prior_with_proof.proof_eligibility_reference =
        Some(format!("proof-eligibility-{prior_with_proof_key}"));
    prior_with_proof.proof_evidence_writer_fact_reference =
        Some(format!("proof-evidence-{prior_with_proof_key}"));
    let prior_with_proof = record_writer_fact(&store, prior_with_proof).await;
    let accepted_after_proof_prior = record_writer_fact(
        &store,
        accepted_transition_fact(&prior_with_proof_key, &prior_with_proof.writer_fact_id),
    )
    .await;
    assert!(
        store
            .derive_accepted_completion_non_authority_projection_snapshots(
                &accepted_after_proof_prior.promise_reference,
                &accepted_after_proof_prior.realm_id,
            )
            .await
            .expect("mutual route with prior proof fields should be readable")
            .is_empty()
    );
}

#[tokio::test]
async fn governed_source_candidate_coexisting_with_valid_acceptance_fails_closed() {
    let (_test_state, config, _client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("projection-governed-candidate-coexists");
    let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;

    store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(&idempotency_key, &prior.writer_fact_id),
        ))
        .await
        .expect("valid accepted transition should persist");

    let mut governed_candidate = prior_mutual_acknowledgement_fact(
        &idempotency_key,
        PromiseCompletionStateClass::CompletionUnderGovernedReview,
    );
    governed_candidate.source_route_class =
        PromiseCompletionSourceRouteClass::GovernedReviewCompletion;
    governed_candidate.governed_review_reference =
        Some(format!("governed-review-{idempotency_key}"));
    governed_candidate.review_authority_reference =
        Some(format!("review-authority-{idempotency_key}"));
    governed_candidate.fact_idempotency_key =
        Some(format!("governed-source-candidate-{idempotency_key}"));
    governed_candidate.reason_code_class =
        Some(format!("governed-source-candidate-{idempotency_key}"));
    record_writer_fact(&store, governed_candidate).await;

    let error = store
        .derive_accepted_completion_non_authority_projection_snapshots(
            &prior.promise_reference,
            &prior.realm_id,
        )
        .await
        .expect_err("coexisting governed source candidate writer truth must fail closed");

    assert!(matches!(
        error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
}

#[tokio::test]
async fn non_prior_source_candidate_coexisting_with_valid_acceptance_fails_closed() {
    let (_test_state, config, _client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("projection-source-candidate-coexists");
    let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;

    store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(&idempotency_key, &prior.writer_fact_id),
        ))
        .await
        .expect("valid accepted transition should persist");

    let mut rejected_candidate = prior_mutual_acknowledgement_fact(
        &idempotency_key,
        PromiseCompletionStateClass::CompletionRejected,
    );
    rejected_candidate.fact_idempotency_key =
        Some(format!("rejected-source-candidate-{idempotency_key}"));
    rejected_candidate.reason_code_class =
        Some(format!("rejected-source-candidate-{idempotency_key}"));
    record_writer_fact(&store, rejected_candidate).await;

    let error = store
        .derive_accepted_completion_non_authority_projection_snapshots(
            &prior.promise_reference,
            &prior.realm_id,
        )
        .await
        .expect_err("coexisting non-prior source candidate writer truth must fail closed");

    assert!(matches!(
        error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
}

#[tokio::test]
async fn contradictory_writer_truth_for_same_boundary_fails_closed() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("projection-contradiction");
    let first_prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;
    let mut second_prior = prior_mutual_acknowledgement_fact(
        &idempotency_key,
        PromiseCompletionStateClass::CompletionPendingMutualAcknowledgement,
    );
    second_prior.fact_idempotency_key = Some(format!("prior-second-{idempotency_key}"));
    second_prior.reason_code_class = Some(format!(
        "completion-pending-mutual-ack-second-{idempotency_key}"
    ));
    let second_prior = record_writer_fact(&store, second_prior).await;

    record_writer_fact(
        &store,
        accepted_transition_fact(&idempotency_key, &first_prior.writer_fact_id),
    )
    .await;
    record_writer_fact(
        &store,
        accepted_transition_fact(&idempotency_key, &second_prior.writer_fact_id),
    )
    .await;

    let error = store
        .derive_accepted_completion_non_authority_projection_snapshots(
            &first_prior.promise_reference,
            &first_prior.realm_id,
        )
        .await
        .expect_err("contradictory accepted writer truth must fail closed");

    assert!(matches!(
        error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
    assert_eq!(
        writer_fact_count_for_promise(&client, &first_prior.promise_reference).await,
        4
    );
}

#[tokio::test]
async fn wrong_key_accepted_writer_truth_coexisting_with_valid_acceptance_fails_closed() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("projection-wrong-key-coexists");
    let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;

    store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(&idempotency_key, &prior.writer_fact_id),
        ))
        .await
        .expect("valid accepted transition should persist");
    let mut wrong_key = accepted_transition_fact(&idempotency_key, &prior.writer_fact_id);
    wrong_key.fact_idempotency_key = Some(format!("unbound-accepted-{idempotency_key}"));
    record_writer_fact(&store, wrong_key).await;

    let error = store
        .derive_accepted_completion_non_authority_projection_snapshots(
            &prior.promise_reference,
            &prior.realm_id,
        )
        .await
        .expect_err("coexisting wrong-key accepted writer truth must fail closed");

    assert!(matches!(
        error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
    assert_eq!(
        writer_fact_count_for_promise(&client, &prior.promise_reference).await,
        3
    );
}

#[tokio::test]
async fn non_accepted_transition_truth_coexisting_with_valid_acceptance_fails_closed() {
    let (_test_state, config, _client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");

    for state in [
        PromiseCompletionStateClass::CompletionRejected,
        PromiseCompletionStateClass::CompletionExpired,
        PromiseCompletionStateClass::CompletionCorrectedOrSuperseded,
    ] {
        let state_name = state.as_str();
        let idempotency_key = unique_idempotency_key(&format!("projection-{state_name}-coexists"));
        let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;

        store
            .record_mutual_acknowledgement_accepted_transition(transition_input(
                accepted_transition_fact(&idempotency_key, &prior.writer_fact_id),
            ))
            .await
            .expect("valid accepted transition should persist");

        let mut non_accepted = accepted_transition_fact(&idempotency_key, &prior.writer_fact_id);
        non_accepted.completion_state_class = state;
        non_accepted.completed_reference_eligible = false;
        non_accepted.fact_idempotency_key = Some(format!("{state_name}-{idempotency_key}"));
        non_accepted.reason_code_class = Some(format!("{state_name}-{idempotency_key}"));
        record_writer_fact(&store, non_accepted).await;

        let error = store
            .derive_accepted_completion_non_authority_projection_snapshots(
                &prior.promise_reference,
                &prior.realm_id,
            )
            .await
            .expect_err("coexisting non-accepted transition writer truth must fail closed");

        assert!(matches!(
            error,
            PromiseCompletionWriterFactPersistenceError::BadRequest(_)
        ));
    }
}

#[tokio::test]
async fn correction_or_supersession_truth_coexisting_with_valid_acceptance_fails_closed() {
    let (_test_state, config, _client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("projection-correction-coexists");
    let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;

    let accepted = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(&idempotency_key, &prior.writer_fact_id),
        ))
        .await
        .expect("valid accepted transition should persist");

    let mut correction = accepted_transition_fact(&idempotency_key, &accepted.writer_fact_id);
    correction.fact_family = PromiseCompletionWriterFactFamily::CorrectionOrSupersession;
    correction.previous_completion_state_class =
        Some(PromiseCompletionStateClass::CompletionAccepted);
    correction.completion_state_class =
        PromiseCompletionStateClass::CompletionCorrectedOrSuperseded;
    correction.completed_reference_eligible = false;
    correction.fact_idempotency_key = Some(format!("correction-or-supersession-{idempotency_key}"));
    correction.reason_code_class = Some(format!("correction-or-supersession-{idempotency_key}"));
    correction.correction_or_supersession_reference = Some(format!(
        "correction-or-supersession-reference-{idempotency_key}"
    ));
    record_writer_fact(&store, correction).await;

    let error = store
        .derive_accepted_completion_non_authority_projection_snapshots(
            &prior.promise_reference,
            &prior.realm_id,
        )
        .await
        .expect_err("coexisting correction or supersession writer truth must fail closed");

    assert!(matches!(
        error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
}

#[tokio::test]
async fn outcome_reference_truth_coexisting_with_valid_acceptance_fails_closed() {
    let (_test_state, config, _client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("projection-outcome-coexists");
    let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;

    let accepted = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(&idempotency_key, &prior.writer_fact_id),
        ))
        .await
        .expect("valid accepted transition should persist");

    let mut outcome = accepted_transition_fact(&idempotency_key, &accepted.writer_fact_id);
    outcome.fact_family = PromiseCompletionWriterFactFamily::CompletionOutcomeReference;
    outcome.previous_completion_state_class = Some(PromiseCompletionStateClass::CompletionAccepted);
    outcome.completion_state_class = PromiseCompletionStateClass::CompletionRejected;
    outcome.completed_reference_eligible = false;
    outcome.fact_idempotency_key = Some(format!("outcome-reference-{idempotency_key}"));
    outcome.reason_code_class = Some(format!("outcome-reference-{idempotency_key}"));
    record_writer_fact(&store, outcome).await;

    let error = store
        .derive_accepted_completion_non_authority_projection_snapshots(
            &prior.promise_reference,
            &prior.realm_id,
        )
        .await
        .expect_err("coexisting outcome reference writer truth must fail closed");

    assert!(matches!(
        error,
        PromiseCompletionWriterFactPersistenceError::BadRequest(_)
    ));
}

#[tokio::test]
async fn distinct_policy_versions_for_same_boundary_do_not_conflict() {
    let (_test_state, config, _client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("projection-policy-boundary");
    let policy_one_prior =
        record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;
    let policy_one = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(&idempotency_key, &policy_one_prior.writer_fact_id),
        ))
        .await
        .expect("policy one accepted transition should persist");

    let mut policy_two_prior = prior_mutual_acknowledgement_fact(
        &idempotency_key,
        PromiseCompletionStateClass::CompletionPendingMutualAcknowledgement,
    );
    policy_two_prior.policy_version = Some(2);
    let policy_two_prior = record_writer_fact(&store, policy_two_prior).await;
    let mut policy_two_fact =
        accepted_transition_fact(&idempotency_key, &policy_two_prior.writer_fact_id);
    policy_two_fact.policy_version = Some(2);
    let _policy_two = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(policy_two_fact))
        .await
        .expect("policy two accepted transition should persist");

    let snapshots = store
        .derive_accepted_completion_non_authority_projection_snapshots(
            &policy_one.promise_reference,
            &policy_one.realm_id,
        )
        .await
        .expect("distinct policy versions should remain distinct projection boundaries");

    assert_eq!(snapshots.len(), 2);
    let policy_versions: Vec<i32> = snapshots
        .iter()
        .map(|snapshot| snapshot.policy_version)
        .collect();
    assert!(policy_versions.contains(&1));
    assert!(policy_versions.contains(&2));
}

#[tokio::test]
async fn duplicate_projection_reads_are_deterministic_and_side_effect_free() {
    let (_test_state, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let idempotency_key = unique_idempotency_key("projection-repeat-read");
    let prior = record_prior_pending_mutual_acknowledgement(&store, &idempotency_key).await;
    let accepted = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(&idempotency_key, &prior.writer_fact_id),
        ))
        .await
        .expect("accepted transition should persist");
    let writer_fact_count_before =
        writer_fact_count_for_promise(&client, &accepted.promise_reference).await;
    let side_effects_before = side_effect_counts(&client).await;

    let first = store
        .derive_accepted_completion_non_authority_projection_snapshots(
            &accepted.promise_reference,
            &accepted.realm_id,
        )
        .await
        .expect("first projection read should derive");
    let second = store
        .derive_accepted_completion_non_authority_projection_snapshots(
            &accepted.promise_reference,
            &accepted.realm_id,
        )
        .await
        .expect("second projection read should derive");

    assert_eq!(first, second);
    assert_eq!(
        writer_fact_count_for_promise(&client, &accepted.promise_reference).await,
        writer_fact_count_before
    );
    assert_eq!(side_effect_counts(&client).await, side_effects_before);
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
    record_writer_fact(
        store,
        prior_mutual_acknowledgement_fact(idempotency_key, completion_state),
    )
    .await
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
