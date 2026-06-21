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
async fn narrow_api_exposes_availability_only_to_directly_involved_ordinary_account() {
    let (_test_state, app, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let involved = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-involved",
        "promise-completion-narrow-api-involved",
    )
    .await;
    let outsider = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-outsider",
        "promise-completion-narrow-api-outsider",
    )
    .await;
    let controlled = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-controlled",
        "promise-completion-narrow-api-controlled",
    )
    .await;

    let idempotency_key = unique_idempotency_key("participant-display-narrow-api");
    let promise_case = create_promise_intent(&app, &involved, &controlled, &idempotency_key).await;
    set_account_class(
        &client,
        &controlled.account_id,
        "Controlled Exceptional Account",
    )
    .await;

    let acknowledgement_reference = format!("ordinary-acknowledgement-{idempotency_key}");
    let prior = record_prior_pending_mutual_acknowledgement(
        &store,
        &idempotency_key,
        &acknowledgement_reference,
        &promise_case.promise_reference,
        &promise_case.realm_id,
    )
    .await;
    let accepted = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(
                &idempotency_key,
                &prior.writer_fact_id,
                &acknowledgement_reference,
                &promise_case.promise_reference,
                &promise_case.realm_id,
            ),
        ))
        .await
        .expect("accepted transition should persist");
    let context = ExposureContext {
        accepted_writer_fact_id: accepted.writer_fact_id.clone(),
        prior_writer_fact_id: prior.writer_fact_id.clone(),
        promise_reference: accepted.promise_reference.clone(),
        realm_id: accepted.realm_id.clone(),
        promise_terms_reference: format!("promise-terms-{idempotency_key}"),
        participant_set_reference: format!("participant-set-{idempotency_key}"),
        acknowledgement_reference,
        idempotency_key: idempotency_key.clone(),
    };
    let path = availability_path(&context.promise_reference, &context.realm_id);
    let path_with_caller_supplied_truth = format!(
        "{}&participant_set_reference={}&ordinary_participant_acknowledgement_reference={}",
        path, context.participant_set_reference, context.acknowledgement_reference
    );
    let path_with_wrong_caller_supplied_truth = format!(
        "{path}&participant_set_reference=wrong-participant-set-{idempotency_key}&ordinary_participant_acknowledgement_reference=wrong-acknowledgement-{idempotency_key}"
    );
    let side_effects_before = side_effect_counts(&client).await;

    let unauthenticated = get_http(&app, &path, None).await;
    assert_eq!(unauthenticated.status, StatusCode::UNAUTHORIZED);
    assert_no_completion_exposure(&unauthenticated, &context);

    let unauthenticated_without_realm = get_http(
        &app,
        &availability_path_without_realm(&context.promise_reference),
        None,
    )
    .await;
    assert_eq!(
        unauthenticated_without_realm.status,
        StatusCode::UNAUTHORIZED
    );
    assert_no_completion_exposure(&unauthenticated_without_realm, &context);

    let involved_without_realm = get_http(
        &app,
        &availability_path_without_realm(&context.promise_reference),
        Some(involved.token.as_str()),
    )
    .await;
    assert_unavailable_response(&involved_without_realm, &context);

    let involved_with_blank_realm = get_http(
        &app,
        &format!(
            "/api/promise-completion/participant-safe-display-availability/{}?realm_id=",
            context.promise_reference
        ),
        Some(involved.token.as_str()),
    )
    .await;
    assert_unavailable_response(&involved_with_blank_realm, &context);

    let outsider_response = get_http(&app, &path, Some(outsider.token.as_str())).await;
    assert_unavailable_response(&outsider_response, &context);

    let outsider_with_truth = get_http(
        &app,
        &path_with_caller_supplied_truth,
        Some(outsider.token.as_str()),
    )
    .await;
    assert_unavailable_response(&outsider_with_truth, &context);

    let controlled_response = get_http(&app, &path, Some(controlled.token.as_str())).await;
    assert_unavailable_response(&controlled_response, &context);

    let wrong_promise = get_http(
        &app,
        &availability_path(
            &format!("wrong-promise-{idempotency_key}"),
            &context.realm_id,
        ),
        Some(involved.token.as_str()),
    )
    .await;
    assert_unavailable_response(&wrong_promise, &context);

    let wrong_realm = get_http(
        &app,
        &availability_path(
            &context.promise_reference,
            &format!("wrong-realm-{idempotency_key}"),
        ),
        Some(involved.token.as_str()),
    )
    .await;
    assert_unavailable_response(&wrong_realm, &context);

    let involved_response = get_http(&app, &path, Some(involved.token.as_str())).await;
    assert_available_response(&involved_response, &context);

    let involved_with_wrong_truth = get_http(
        &app,
        &path_with_wrong_caller_supplied_truth,
        Some(involved.token.as_str()),
    )
    .await;
    assert_available_response(&involved_with_wrong_truth, &context);

    assert_eq!(side_effect_counts(&client).await, side_effects_before);
}

#[tokio::test]
async fn narrow_api_returns_unavailable_for_suppressed_writer_truth_without_reason_leakage() {
    let (_test_state, app, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let involved = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-suppressed",
        "promise-completion-narrow-api-suppressed",
    )
    .await;
    let counterparty = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-suppressed-counterparty",
        "promise-completion-narrow-api-suppressed-counterparty",
    )
    .await;
    let idempotency_key = unique_idempotency_key("participant-display-narrow-api-suppressed");
    let promise_case =
        create_promise_intent(&app, &involved, &counterparty, &idempotency_key).await;
    let acknowledgement_reference = format!("ordinary-acknowledgement-{idempotency_key}");
    let prior = record_prior_pending_mutual_acknowledgement(
        &store,
        &idempotency_key,
        &acknowledgement_reference,
        &promise_case.promise_reference,
        &promise_case.realm_id,
    )
    .await;
    let mut accepted_fact = accepted_transition_fact(
        &idempotency_key,
        &prior.writer_fact_id,
        &acknowledgement_reference,
        &promise_case.promise_reference,
        &promise_case.realm_id,
    );
    accepted_fact.legal_hold_intersection_reference =
        Some(format!("legal-hold-active-{idempotency_key}"));
    let accepted = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(accepted_fact))
        .await
        .expect("suppressed accepted transition should persist");
    let context = ExposureContext {
        accepted_writer_fact_id: accepted.writer_fact_id.clone(),
        prior_writer_fact_id: prior.writer_fact_id.clone(),
        promise_reference: accepted.promise_reference.clone(),
        realm_id: accepted.realm_id.clone(),
        promise_terms_reference: format!("promise-terms-{idempotency_key}"),
        participant_set_reference: format!("participant-set-{idempotency_key}"),
        acknowledgement_reference,
        idempotency_key,
    };
    let side_effects_before = side_effect_counts(&client).await;

    let response = get_http(
        &app,
        &availability_path(&context.promise_reference, &context.realm_id),
        Some(involved.token.as_str()),
    )
    .await;

    assert_unavailable_response(&response, &context);
    assert_eq!(side_effect_counts(&client).await, side_effects_before);
}

#[tokio::test]
async fn narrow_api_hides_boundary_condition_families_without_reason_leakage() {
    let (_test_state, app, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let involved = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-boundary-families",
        "promise-completion-narrow-api-boundary-families",
    )
    .await;
    let counterparty = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-boundary-families-counterparty",
        "promise-completion-narrow-api-boundary-families-counterparty",
    )
    .await;

    for boundary_row in ["accepted", "prior"] {
        for boundary_field in [
            "block_withdrawal_state_reference",
            "age_assurance_state_reference",
            "legal_hold_intersection_reference",
            "critical_harm_case_reference",
            "account_lifecycle_reference",
            "anti_abuse_continuity_reference",
            "safety_case_reference",
        ] {
            let case_label = format!("{boundary_row}-{boundary_field}");
            let idempotency_key = unique_idempotency_key(&format!(
                "participant-display-narrow-api-boundary-{case_label}"
            ));
            let promise_case =
                create_promise_intent(&app, &involved, &counterparty, &idempotency_key).await;
            let acknowledgement_reference = format!("ordinary-acknowledgement-{idempotency_key}");
            let mut prior_fact = prior_pending_mutual_acknowledgement_fact(
                &idempotency_key,
                &acknowledgement_reference,
                &promise_case.promise_reference,
                &promise_case.realm_id,
            );
            if boundary_row == "prior" {
                apply_suppressed_boundary_reference(
                    &mut prior_fact,
                    boundary_field,
                    &idempotency_key,
                );
            }
            let prior = record_writer_fact(&store, prior_fact).await;
            let mut accepted_fact = accepted_transition_fact(
                &idempotency_key,
                &prior.writer_fact_id,
                &acknowledgement_reference,
                &promise_case.promise_reference,
                &promise_case.realm_id,
            );
            if boundary_row == "accepted" {
                apply_suppressed_boundary_reference(
                    &mut accepted_fact,
                    boundary_field,
                    &idempotency_key,
                );
            }
            let accepted = store
                .record_mutual_acknowledgement_accepted_transition(transition_input(accepted_fact))
                .await
                .expect("suppressed boundary transition should persist");
            let context = ExposureContext {
                accepted_writer_fact_id: accepted.writer_fact_id.clone(),
                prior_writer_fact_id: prior.writer_fact_id.clone(),
                promise_reference: accepted.promise_reference.clone(),
                realm_id: accepted.realm_id.clone(),
                promise_terms_reference: format!("promise-terms-{idempotency_key}"),
                participant_set_reference: format!("participant-set-{idempotency_key}"),
                acknowledgement_reference,
                idempotency_key,
            };
            let side_effects_before = side_effect_counts(&client).await;

            let response = get_http(
                &app,
                &availability_path(&context.promise_reference, &context.realm_id),
                Some(involved.token.as_str()),
            )
            .await;

            assert_unavailable_response(&response, &context);
            assert_eq!(
                side_effect_counts(&client).await,
                side_effects_before,
                "participant-safe display availability must be side-effect free for {case_label}",
            );
        }
    }
}

#[tokio::test]
async fn narrow_api_hides_writer_truth_contradictions_without_reason_leakage() {
    let (_test_state, app, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let involved = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-contradiction",
        "promise-completion-narrow-api-contradiction",
    )
    .await;
    let counterparty = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-contradiction-counterparty",
        "promise-completion-narrow-api-contradiction-counterparty",
    )
    .await;

    for state in [
        PromiseCompletionStateClass::CompletionRejected,
        PromiseCompletionStateClass::CompletionExpired,
        PromiseCompletionStateClass::CompletionCorrectedOrSuperseded,
    ] {
        let state_name = state.as_str();
        let idempotency_key =
            unique_idempotency_key(&format!("participant-display-narrow-api-{state_name}"));
        let promise_case =
            create_promise_intent(&app, &involved, &counterparty, &idempotency_key).await;
        let acknowledgement_reference = format!("ordinary-acknowledgement-{idempotency_key}");
        let prior = record_prior_pending_mutual_acknowledgement(
            &store,
            &idempotency_key,
            &acknowledgement_reference,
            &promise_case.promise_reference,
            &promise_case.realm_id,
        )
        .await;
        let accepted = store
            .record_mutual_acknowledgement_accepted_transition(transition_input(
                accepted_transition_fact(
                    &idempotency_key,
                    &prior.writer_fact_id,
                    &acknowledgement_reference,
                    &promise_case.promise_reference,
                    &promise_case.realm_id,
                ),
            ))
            .await
            .expect("valid accepted transition should persist");
        let mut non_accepted = accepted_transition_fact(
            &idempotency_key,
            &prior.writer_fact_id,
            &acknowledgement_reference,
            &promise_case.promise_reference,
            &promise_case.realm_id,
        );
        non_accepted.completion_state_class = state;
        non_accepted.completed_reference_eligible = false;
        non_accepted.fact_idempotency_key = Some(format!("{state_name}-{idempotency_key}"));
        non_accepted.reason_code_class = Some(format!("{state_name}-{idempotency_key}"));
        let non_accepted = record_writer_fact(&store, non_accepted).await;
        let context = ExposureContext {
            accepted_writer_fact_id: accepted.writer_fact_id.clone(),
            prior_writer_fact_id: prior.writer_fact_id.clone(),
            promise_reference: accepted.promise_reference.clone(),
            realm_id: accepted.realm_id.clone(),
            promise_terms_reference: format!("promise-terms-{idempotency_key}"),
            participant_set_reference: format!("participant-set-{idempotency_key}"),
            acknowledgement_reference: acknowledgement_reference.clone(),
            idempotency_key: idempotency_key.clone(),
        };
        let non_accepted_context = ExposureContext {
            accepted_writer_fact_id: non_accepted.writer_fact_id.clone(),
            prior_writer_fact_id: prior.writer_fact_id.clone(),
            promise_reference: non_accepted.promise_reference.clone(),
            realm_id: non_accepted.realm_id.clone(),
            promise_terms_reference: format!("promise-terms-{idempotency_key}"),
            participant_set_reference: format!("participant-set-{idempotency_key}"),
            acknowledgement_reference,
            idempotency_key,
        };
        let side_effects_before = side_effect_counts(&client).await;

        let response = get_http(
            &app,
            &availability_path(&context.promise_reference, &context.realm_id),
            Some(involved.token.as_str()),
        )
        .await;

        assert_unavailable_response(&response, &context);
        assert_no_completion_exposure(&response, &non_accepted_context);
        assert_eq!(
            side_effect_counts(&client).await,
            side_effects_before,
            "participant-safe display availability must be side-effect free for {state_name}",
        );
    }
}

#[tokio::test]
async fn narrow_api_hides_prior_linked_correction_drift() {
    let (_test_state, app, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let involved = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-linked-correction",
        "promise-completion-narrow-api-linked-correction",
    )
    .await;
    let counterparty = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-linked-correction-counterparty",
        "promise-completion-narrow-api-linked-correction-counterparty",
    )
    .await;
    let idempotency_key =
        unique_idempotency_key("participant-display-narrow-api-linked-correction");
    let promise_case =
        create_promise_intent(&app, &involved, &counterparty, &idempotency_key).await;
    let acknowledgement_reference = format!("ordinary-acknowledgement-{idempotency_key}");
    let prior = record_prior_pending_mutual_acknowledgement(
        &store,
        &idempotency_key,
        &acknowledgement_reference,
        &promise_case.promise_reference,
        &promise_case.realm_id,
    )
    .await;
    let accepted = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(
                &idempotency_key,
                &prior.writer_fact_id,
                &acknowledgement_reference,
                &promise_case.promise_reference,
                &promise_case.realm_id,
            ),
        ))
        .await
        .expect("valid accepted transition should persist");
    let mut correction = accepted_transition_fact(
        &idempotency_key,
        &accepted.writer_fact_id,
        &acknowledgement_reference,
        &promise_case.promise_reference,
        &promise_case.realm_id,
    );
    correction.fact_family = PromiseCompletionWriterFactFamily::CorrectionOrSupersession;
    correction.previous_completion_state_class =
        Some(PromiseCompletionStateClass::CompletionAccepted);
    correction.completion_state_class =
        PromiseCompletionStateClass::CompletionCorrectedOrSuperseded;
    correction.completed_reference_eligible = false;
    correction.promise_terms_reference = Some(format!("drifted-promise-terms-{idempotency_key}"));
    correction.participant_set_reference =
        Some(format!("drifted-participant-set-{idempotency_key}"));
    correction.policy_version = Some(2);
    correction.fact_idempotency_key =
        Some(format!("prior-linked-correction-drift-{idempotency_key}"));
    correction.reason_code_class = Some(format!("prior-linked-correction-drift-{idempotency_key}"));
    correction.correction_or_supersession_reference = Some(format!(
        "prior-linked-correction-reference-{idempotency_key}"
    ));
    let correction = record_writer_fact(&store, correction).await;
    let context = ExposureContext {
        accepted_writer_fact_id: accepted.writer_fact_id.clone(),
        prior_writer_fact_id: prior.writer_fact_id.clone(),
        promise_reference: accepted.promise_reference.clone(),
        realm_id: accepted.realm_id.clone(),
        promise_terms_reference: format!("promise-terms-{idempotency_key}"),
        participant_set_reference: format!("participant-set-{idempotency_key}"),
        acknowledgement_reference: acknowledgement_reference.clone(),
        idempotency_key: idempotency_key.clone(),
    };
    let correction_context = ExposureContext {
        accepted_writer_fact_id: correction.writer_fact_id.clone(),
        prior_writer_fact_id: accepted.writer_fact_id.clone(),
        promise_reference: correction.promise_reference.clone(),
        realm_id: correction.realm_id.clone(),
        promise_terms_reference: format!("drifted-promise-terms-{idempotency_key}"),
        participant_set_reference: format!("drifted-participant-set-{idempotency_key}"),
        acknowledgement_reference,
        idempotency_key,
    };
    let side_effects_before = side_effect_counts(&client).await;

    let response = get_http(
        &app,
        &availability_path(&context.promise_reference, &context.realm_id),
        Some(involved.token.as_str()),
    )
    .await;

    assert_unavailable_response(&response, &context);
    assert_no_completion_exposure(&response, &correction_context);
    assert_eq!(side_effect_counts(&client).await, side_effects_before);
}

#[tokio::test]
async fn narrow_api_fails_closed_when_competing_snapshot_is_suppressed() {
    let (_test_state, app, config, client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let involved = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-competing",
        "promise-completion-narrow-api-competing",
    )
    .await;
    let counterparty = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-competing-counterparty",
        "promise-completion-narrow-api-competing-counterparty",
    )
    .await;
    let promise_key = unique_idempotency_key("participant-display-narrow-api-competing");
    let promise_case = create_promise_intent(&app, &involved, &counterparty, &promise_key).await;

    let clear_key = unique_idempotency_key("participant-display-narrow-api-clear");
    let clear_acknowledgement_reference = format!("ordinary-acknowledgement-{clear_key}");
    let clear_prior = record_prior_pending_mutual_acknowledgement(
        &store,
        &clear_key,
        &clear_acknowledgement_reference,
        &promise_case.promise_reference,
        &promise_case.realm_id,
    )
    .await;
    let clear = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(
                &clear_key,
                &clear_prior.writer_fact_id,
                &clear_acknowledgement_reference,
                &promise_case.promise_reference,
                &promise_case.realm_id,
            ),
        ))
        .await
        .expect("clear accepted transition should persist");

    let suppressed_key = unique_idempotency_key("participant-display-narrow-api-competing-held");
    let suppressed_acknowledgement_reference = format!("ordinary-acknowledgement-{suppressed_key}");
    let suppressed_prior = record_prior_pending_mutual_acknowledgement(
        &store,
        &suppressed_key,
        &suppressed_acknowledgement_reference,
        &promise_case.promise_reference,
        &promise_case.realm_id,
    )
    .await;
    let mut suppressed_fact = accepted_transition_fact(
        &suppressed_key,
        &suppressed_prior.writer_fact_id,
        &suppressed_acknowledgement_reference,
        &promise_case.promise_reference,
        &promise_case.realm_id,
    );
    suppressed_fact.legal_hold_intersection_reference =
        Some(format!("legal-hold-active-{suppressed_key}"));
    let suppressed = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(suppressed_fact))
        .await
        .expect("suppressed accepted transition should persist");
    let context = ExposureContext {
        accepted_writer_fact_id: clear.writer_fact_id.clone(),
        prior_writer_fact_id: clear_prior.writer_fact_id.clone(),
        promise_reference: clear.promise_reference.clone(),
        realm_id: clear.realm_id.clone(),
        promise_terms_reference: format!("promise-terms-{clear_key}"),
        participant_set_reference: format!("participant-set-{clear_key}"),
        acknowledgement_reference: clear_acknowledgement_reference,
        idempotency_key: clear_key,
    };
    let suppressed_context = ExposureContext {
        accepted_writer_fact_id: suppressed.writer_fact_id.clone(),
        prior_writer_fact_id: suppressed_prior.writer_fact_id.clone(),
        promise_reference: suppressed.promise_reference.clone(),
        realm_id: suppressed.realm_id.clone(),
        promise_terms_reference: format!("promise-terms-{suppressed_key}"),
        participant_set_reference: format!("participant-set-{suppressed_key}"),
        acknowledgement_reference: suppressed_acknowledgement_reference,
        idempotency_key: suppressed_key,
    };
    let side_effects_before = side_effect_counts(&client).await;

    let response = get_http(
        &app,
        &availability_path(&context.promise_reference, &context.realm_id),
        Some(involved.token.as_str()),
    )
    .await;

    assert_unavailable_response(&response, &context);
    assert_no_completion_exposure(&response, &suppressed_context);
    assert_eq!(side_effect_counts(&client).await, side_effects_before);
}

#[tokio::test]
async fn narrow_api_does_not_treat_acknowledgement_reference_format_as_membership() {
    let (_test_state, app, config, _client) = test_context().await;
    let store = PromiseCompletionWriterFactStore::connect(&config)
        .await
        .expect("store should connect");
    let initiator = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-membership-initiator",
        "promise-completion-narrow-api-membership-initiator",
    )
    .await;
    let counterparty = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-membership-counterparty",
        "promise-completion-narrow-api-membership-counterparty",
    )
    .await;
    let outsider = sign_in(
        &app,
        "pi-user-promise-completion-narrow-api-membership-outsider",
        "promise-completion-narrow-api-membership-outsider",
    )
    .await;

    let idempotency_key = unique_idempotency_key("participant-display-narrow-api-membership");
    let promise_case =
        create_promise_intent(&app, &initiator, &counterparty, &idempotency_key).await;
    let acknowledgement_reference = account_acknowledgement_reference(&outsider.account_id);
    let prior = record_prior_pending_mutual_acknowledgement(
        &store,
        &idempotency_key,
        &acknowledgement_reference,
        &promise_case.promise_reference,
        &promise_case.realm_id,
    )
    .await;
    let accepted = store
        .record_mutual_acknowledgement_accepted_transition(transition_input(
            accepted_transition_fact(
                &idempotency_key,
                &prior.writer_fact_id,
                &acknowledgement_reference,
                &promise_case.promise_reference,
                &promise_case.realm_id,
            ),
        ))
        .await
        .expect("accepted transition should persist");
    let context = ExposureContext {
        accepted_writer_fact_id: accepted.writer_fact_id.clone(),
        prior_writer_fact_id: prior.writer_fact_id.clone(),
        promise_reference: accepted.promise_reference.clone(),
        realm_id: accepted.realm_id.clone(),
        promise_terms_reference: format!("promise-terms-{idempotency_key}"),
        participant_set_reference: format!("participant-set-{idempotency_key}"),
        acknowledgement_reference,
        idempotency_key,
    };

    let response = get_http(
        &app,
        &availability_path(&context.promise_reference, &context.realm_id),
        Some(outsider.token.as_str()),
    )
    .await;

    assert_unavailable_response(&response, &context);
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

fn account_acknowledgement_reference(account_id: &str) -> String {
    format!("ordinary-participant-acknowledgement-{account_id}")
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
    acknowledgement_reference: &str,
    promise_reference: &str,
    realm_id: &str,
) -> musubi_backend::services::promise_completion::PromiseCompletionWriterFactSnapshot {
    record_writer_fact(
        store,
        prior_pending_mutual_acknowledgement_fact(
            idempotency_key,
            acknowledgement_reference,
            promise_reference,
            realm_id,
        ),
    )
    .await
}

fn prior_pending_mutual_acknowledgement_fact(
    idempotency_key: &str,
    acknowledgement_reference: &str,
    promise_reference: &str,
    realm_id: &str,
) -> ProposedPromiseCompletionWriterFact {
    let mut fact = base_fact(
        idempotency_key,
        acknowledgement_reference,
        promise_reference,
        realm_id,
    );
    fact.fact_family = PromiseCompletionWriterFactFamily::SourceRouteCandidate;
    fact.previous_completion_state_class = None;
    fact.completion_state_class =
        PromiseCompletionStateClass::CompletionPendingMutualAcknowledgement;
    fact.completed_reference_eligible = false;
    fact.fact_idempotency_key = Some(format!("prior-{idempotency_key}"));
    fact.reason_code_class = Some(format!("completion-pending-mutual-ack-{idempotency_key}"));
    fact
}

fn accepted_transition_fact(
    idempotency_key: &str,
    prior_writer_fact_id: &str,
    acknowledgement_reference: &str,
    promise_reference: &str,
    realm_id: &str,
) -> ProposedPromiseCompletionWriterFact {
    let mut fact = base_fact(
        idempotency_key,
        acknowledgement_reference,
        promise_reference,
        realm_id,
    );
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

fn apply_suppressed_boundary_reference(
    fact: &mut ProposedPromiseCompletionWriterFact,
    boundary_field: &str,
    idempotency_key: &str,
) {
    let value = match boundary_field {
        "block_withdrawal_state_reference" => {
            format!("block-withdrawal-active-{idempotency_key}")
        }
        "age_assurance_state_reference" => format!("age-assurance-conflict-{idempotency_key}"),
        "legal_hold_intersection_reference" => format!("legal-hold-active-{idempotency_key}"),
        "critical_harm_case_reference" => format!("critical-harm-open-{idempotency_key}"),
        "account_lifecycle_reference" => format!("account-lifecycle-held-{idempotency_key}"),
        "anti_abuse_continuity_reference" => format!("anti-abuse-interrupted-{idempotency_key}"),
        "safety_case_reference" => format!("safety-case-open-{idempotency_key}"),
        _ => panic!("unsupported boundary field {boundary_field}"),
    };

    match boundary_field {
        "block_withdrawal_state_reference" => {
            fact.block_withdrawal_state_reference = Some(value);
        }
        "age_assurance_state_reference" => {
            fact.age_assurance_state_reference = Some(value);
        }
        "legal_hold_intersection_reference" => {
            fact.legal_hold_intersection_reference = Some(value);
        }
        "critical_harm_case_reference" => {
            fact.critical_harm_case_reference = Some(value);
        }
        "account_lifecycle_reference" => {
            fact.account_lifecycle_reference = Some(value);
        }
        "anti_abuse_continuity_reference" => {
            fact.anti_abuse_continuity_reference = Some(value);
        }
        "safety_case_reference" => {
            fact.safety_case_reference = Some(value);
        }
        _ => panic!("unsupported boundary field {boundary_field}"),
    }
}

fn base_fact(
    idempotency_key: &str,
    acknowledgement_reference: &str,
    promise_reference: &str,
    realm_id: &str,
) -> ProposedPromiseCompletionWriterFact {
    ProposedPromiseCompletionWriterFact {
        promise_reference: Some(promise_reference.to_owned()),
        realm_id: Some(realm_id.to_owned()),
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
        ordinary_participant_acknowledgement_reference: Some(acknowledgement_reference.to_owned()),
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

fn availability_path(promise_reference: &str, realm_id: &str) -> String {
    format!(
        "/api/promise-completion/participant-safe-display-availability/{promise_reference}?realm_id={realm_id}"
    )
}

fn availability_path_without_realm(promise_reference: &str) -> String {
    format!("/api/promise-completion/participant-safe-display-availability/{promise_reference}")
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

async fn set_account_class(client: &tokio_postgres::Client, account_id: &str, account_class: &str) {
    let account_id = uuid::Uuid::parse_str(account_id).expect("account id must be UUID");
    client
        .execute(
            "
            UPDATE core.accounts
            SET account_class = $2
            WHERE account_id = $1
            ",
            &[&account_id, &account_class],
        )
        .await
        .expect("account class should update");
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

fn assert_available_response(response: &HttpResponse, context: &ExposureContext) {
    assert_eq!(response.status, StatusCode::OK);
    let body = response
        .body_json
        .as_ref()
        .expect("availability response should be json");
    assert_minimized_response_fields(body);
    assert_eq!(body["display_availability"], "available");
    assert_eq!(body["completed_reference_available"], true);
    assert_no_completion_exposure(response, context);
}

fn assert_unavailable_response(response: &HttpResponse, context: &ExposureContext) {
    assert_eq!(response.status, StatusCode::OK);
    let body = response
        .body_json
        .as_ref()
        .expect("unavailable response should be json");
    assert_minimized_response_fields(body);
    assert_eq!(body["display_availability"], "unavailable");
    assert_eq!(body["completed_reference_available"], false);
    assert_no_completion_exposure(response, context);
}

fn assert_minimized_response_fields(body: &Value) {
    let object = body.as_object().expect("response must be an object");
    assert_eq!(object.len(), 2);
    assert!(object.contains_key("display_availability"));
    assert!(object.contains_key("completed_reference_available"));
}

fn assert_no_completion_exposure(response: &HttpResponse, context: &ExposureContext) {
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
        "legal-hold-active",
    ] {
        assert!(
            !response.body_text.contains(sensitive_value),
            "response must not expose Promise completion sensitive value `{sensitive_value}`: {:?}",
            response
        );
    }

    for field in [
        "display_audience",
        "display_meaning",
        "display_class",
        "policy_version",
        "accepted_writer_fact_id",
        "prior_writer_fact_id",
        "participant_set_reference",
        "ordinary_participant_acknowledgement_reference",
        "fact_idempotency_key",
        "source_route_class",
        "completion_state_class",
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
    ] {
        assert!(
            !response.body_text.contains(field),
            "response must not expose Promise completion field `{field}`: {:?}",
            response
        );
    }
}

#[derive(Debug)]
struct SignedInUser {
    token: String,
    account_id: String,
}

#[derive(Debug)]
struct PromiseCase {
    promise_reference: String,
    realm_id: String,
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
    }
}

async fn create_promise_intent(
    app: &Router,
    initiator: &SignedInUser,
    counterparty: &SignedInUser,
    suffix: &str,
) -> PromiseCase {
    let realm_id = format!("realm-{suffix}");
    let response = post_json(
        app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": format!("promise-intent-{suffix}"),
            "realm_id": realm_id,
            "counterparty_account_id": counterparty.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    let body = response
        .body_json
        .expect("create Promise response should be json");
    PromiseCase {
        promise_reference: body["promise_intent_id"]
            .as_str()
            .expect("promise_intent_id must exist")
            .to_owned(),
        realm_id,
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
