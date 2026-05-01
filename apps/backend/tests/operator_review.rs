use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use musubi_backend::{build_app, new_state_from_config, new_test_state};
use musubi_db_runtime::DbConfig;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn operator_review_flow_preserves_writer_truth_and_projects_safe_status() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-review-subject", "review-subject").await;
    let counterparty = sign_in(&app, "pi-user-review-counterparty", "review-counterparty").await;
    let client = test_db_client().await;
    let reviewer_id = insert_operator_account(&client, "reviewer").await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let support_id = insert_operator_account(&client, "support").await;

    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(subject.token.as_str()),
        json!({
            "internal_idempotency_key": "operator-review-promise",
            "realm_id": "realm-operator-review",
            "counterparty_account_id": counterparty.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;
    assert_eq!(create_promise.status, StatusCode::OK);
    let promise_intent_id = create_promise.body["promise_intent_id"]
        .as_str()
        .expect("promise_intent_id must exist")
        .to_owned();
    let settlement_case_id = create_promise.body["settlement_case_id"]
        .as_str()
        .expect("settlement_case_id must exist")
        .to_owned();
    let original_settlement_status = settlement_status(&client, &settlement_case_id).await;

    let create_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &reviewer_id,
        json!({
            "case_type": "promise_dispute",
            "severity": "sev2",
            "subject_account_id": subject.account_id,
            "related_promise_intent_id": promise_intent_id,
            "related_settlement_case_id": settlement_case_id,
            "related_realm_id": "realm-operator-review",
            "opened_reason_code": "promise_completion_under_review",
            "source_fact_kind": "settlement_case",
            "source_fact_id": settlement_case_id,
            "source_snapshot_json": {
                "case_status": original_settlement_status
            },
            "request_idempotency_key": "review-case-001"
        }),
    )
    .await;
    assert_eq!(create_case.status, StatusCode::OK);
    assert_eq!(create_case.body["review_status"], "open");
    let review_case_id = create_case.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let initial_status = get_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/status"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(initial_status.status, StatusCode::OK);
    assert_eq!(initial_status.body["user_facing_status"], "pending_review");
    assert_eq!(
        initial_status.body["user_facing_reason_code"],
        "promise_completion_under_review"
    );
    assert_eq!(initial_status.body["source_fact_count"], 1);

    let attach_evidence = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/evidence-bundles"),
        &reviewer_id,
        json!({
            "evidence_visibility": "redacted_raw",
            "summary_json": {
                "badge": "timeline_ready",
                "safe_summary": "Promise timeline and proof summary are ready for review."
            },
            "raw_locator_json": {
                "raw_callback_locator": "private-raw-callback-uri"
            },
            "retention_class": "R6"
        }),
    )
    .await;
    assert_eq!(attach_evidence.status, StatusCode::OK);
    assert!(attach_evidence.body.get("raw_locator_json").is_none());
    assert!(!attach_evidence
        .body
        .to_string()
        .contains("private-raw-callback-uri"));
    let evidence_bundle_id = attach_evidence.body["evidence_bundle_id"]
        .as_str()
        .expect("evidence_bundle_id must exist")
        .to_owned();

    let after_evidence_status = get_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/status"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(after_evidence_status.status, StatusCode::OK);
    assert_eq!(after_evidence_status.body["source_fact_count"], 2);

    let reviewer_grant_attempt = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/evidence-access-grants"),
        &reviewer_id,
        json!({
            "evidence_bundle_id": evidence_bundle_id,
            "grantee_operator_id": reviewer_id,
            "access_scope": "summary_only",
            "grant_reason": "reviewer needs the bounded case summary",
            "expires_at": future_timestamp()
        }),
    )
    .await;
    assert_eq!(reviewer_grant_attempt.status, StatusCode::UNAUTHORIZED);

    let summary_grant = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/evidence-access-grants"),
        &approver_id,
        json!({
            "evidence_bundle_id": evidence_bundle_id,
            "grantee_operator_id": reviewer_id,
            "access_scope": "summary_only",
            "grant_reason": "reviewer needs the bounded case summary",
            "expires_at": future_timestamp()
        }),
    )
    .await;
    assert_eq!(summary_grant.status, StatusCode::OK);
    assert_eq!(summary_grant.body["access_scope"], "summary_only");

    let after_grant_status = get_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/status"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(after_grant_status.status, StatusCode::OK);
    assert_eq!(after_grant_status.body["source_fact_count"], 3);

    let support_raw_grant = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/evidence-access-grants"),
        &approver_id,
        json!({
            "evidence_bundle_id": evidence_bundle_id,
            "grantee_operator_id": support_id,
            "access_scope": "full_raw",
            "grant_reason": "support should not get full raw access",
            "expires_at": future_timestamp()
        }),
    )
    .await;
    assert_eq!(support_raw_grant.status, StatusCode::UNAUTHORIZED);

    let invalid_reason_decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "internal_abuse_label",
            "operator_note_internal": "private operator note must not leak",
            "decision_payload_json": {
                "internal": "not user facing"
            },
            "decision_idempotency_key": "decision-invalid"
        }),
    )
    .await;
    assert_eq!(invalid_reason_decision.status, StatusCode::BAD_REQUEST);

    let reviewer_decision_attempt = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &reviewer_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "restricted_after_review",
            "operator_note_internal": "private operator note must not leak",
            "decision_payload_json": {
                "internal": "not user facing"
            },
            "decision_idempotency_key": "decision-unauthorized"
        }),
    )
    .await;
    assert_eq!(reviewer_decision_attempt.status, StatusCode::UNAUTHORIZED);

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "restricted_after_review",
            "operator_note_internal": "private operator note must not leak",
            "decision_payload_json": {
                "internal": "not user facing"
            },
            "decision_idempotency_key": "decision-001"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);
    let decision_fact_id = decision.body["operator_decision_fact_id"]
        .as_str()
        .expect("decision fact id must exist")
        .to_owned();

    let replayed_decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "restricted_after_review",
            "operator_note_internal": "private operator note must not leak",
            "decision_payload_json": {
                "internal": "not user facing"
            },
            "decision_idempotency_key": "decision-001"
        }),
    )
    .await;
    assert_eq!(replayed_decision.status, StatusCode::OK);
    assert_eq!(
        replayed_decision.body["operator_decision_fact_id"],
        decision_fact_id
    );
    assert_eq!(
        decision_count(&client, &review_case_id).await,
        1,
        "idempotent replay must not append a second decision fact"
    );
    assert_eq!(
        settlement_status(&client, &settlement_case_id).await,
        original_settlement_status,
        "operator decision must not mutate settlement writer truth"
    );

    let decided_status = get_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/status"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(decided_status.status, StatusCode::OK);
    assert_eq!(
        decided_status.body["user_facing_status"],
        "sealed_or_restricted"
    );
    assert_eq!(
        decided_status.body["user_facing_reason_code"],
        "restricted_after_review"
    );
    assert!(decided_status.body.get("operator_note_internal").is_none());
    assert!(!decided_status
        .body
        .to_string()
        .contains("private operator note"));

    let review_detail = operator_get_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}"),
        &approver_id,
    )
    .await;
    assert_eq!(review_detail.status, StatusCode::OK);
    assert!(review_detail.body.get("operator_note_internal").is_none());
    assert!(!review_detail
        .body
        .to_string()
        .contains("private operator note"));
    assert!(!review_detail
        .body
        .to_string()
        .contains("private-raw-callback-uri"));
    assert_eq!(
        review_detail.body["operator_decision_facts"]
            .as_array()
            .expect("decision facts must be an array")
            .len(),
        1
    );

    let appeal = post_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/appeals"),
        Some(subject.token.as_str()),
        json!({
            "source_decision_fact_id": decision_fact_id,
            "submitted_reason_code": "appeal_received",
            "appellant_statement": "I have additional context.",
            "new_evidence_summary_json": {
                "safe_summary": "Additional user-provided context is available."
            },
            "appeal_idempotency_key": "appeal-001"
        }),
    )
    .await;
    assert_eq!(appeal.status, StatusCode::OK);
    assert_eq!(appeal.body["source_review_case_id"], review_case_id);
    assert_eq!(appeal.body["source_decision_fact_id"], decision_fact_id);
    assert_eq!(appeal.body["appeal_status"], "submitted");

    let appeals = get_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/appeals"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(appeals.status, StatusCode::OK);
    assert_eq!(
        appeals
            .body
            .as_array()
            .expect("appeals must be an array")
            .len(),
        1
    );

    let appealed_status = get_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/status"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(appealed_status.status, StatusCode::OK);
    assert_eq!(
        appealed_status.body["user_facing_status"],
        "appeal_submitted"
    );
    assert_eq!(appealed_status.body["appeal_status"], "submitted");
    assert_eq!(
        appealed_status.body["user_facing_reason_code"],
        "appeal_received"
    );

    let cross_user_status = get_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/status"),
        Some(counterparty.token.as_str()),
    )
    .await;
    assert_eq!(cross_user_status.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn operator_review_private_fields_do_not_leak_to_participant_status_projection() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(
        &app,
        "pi-user-review-private-redaction",
        "review-private-redaction",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let operator_note_sentinel = "operator-note-leak-sentinel-phase3";
    let source_snapshot_sentinel = "source-snapshot-leak-sentinel-phase3";
    let decision_payload_sentinel = "decision-payload-leak-sentinel-phase3";
    let private_source_fact_sentinel = "private-source-fact-leak-sentinel-phase3";
    let raw_evidence_locator_sentinel = "raw-evidence-locator-leak-sentinel-phase3";

    let create_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "proof_anomaly",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-review-private-redaction",
            "opened_reason_code": "proof_inconclusive",
            "source_fact_kind": "proof_submission",
            "source_fact_id": private_source_fact_sentinel,
            "source_snapshot_json": {
                "internal_review_reason": source_snapshot_sentinel,
                "raw_evidence_locator": raw_evidence_locator_sentinel,
                "operator_private_context": "source snapshot must stay operator-side"
            },
            "request_idempotency_key": "review-case-private-redaction"
        }),
    )
    .await;
    assert_eq!(create_case.status, StatusCode::OK);
    let review_case_id = create_case.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "restricted_after_review",
            "operator_note_internal": operator_note_sentinel,
            "decision_payload_json": {
                "internal_review_reason": decision_payload_sentinel,
                "raw_evidence_locator": raw_evidence_locator_sentinel,
                "operator_private_context": "decision payload must stay operator-side"
            },
            "decision_idempotency_key": "decision-private-redaction"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);
    let decision_fact_id = decision.body["operator_decision_fact_id"]
        .as_str()
        .expect("decision fact id must exist")
        .to_owned();

    let review_case = client
        .query_one(
            "
            SELECT source_fact_id, source_snapshot_json
            FROM dao.review_cases
            WHERE review_case_id::text = $1
            ",
            &[&review_case_id],
        )
        .await
        .expect("review case private source metadata must remain readable");
    assert_eq!(
        review_case.get::<_, String>("source_fact_id"),
        private_source_fact_sentinel
    );
    let source_snapshot = review_case.get::<_, Value>("source_snapshot_json");
    assert_eq!(
        source_snapshot["internal_review_reason"],
        source_snapshot_sentinel
    );
    assert_eq!(
        source_snapshot["raw_evidence_locator"],
        raw_evidence_locator_sentinel
    );

    let decision_fact = client
        .query_one(
            "
            SELECT operator_note_internal, decision_payload_json
            FROM dao.operator_decision_facts
            WHERE operator_decision_fact_id::text = $1
            ",
            &[&decision_fact_id],
        )
        .await
        .expect("operator decision private fields must remain readable");
    assert_eq!(
        decision_fact.get::<_, Option<String>>("operator_note_internal"),
        Some(operator_note_sentinel.to_owned())
    );
    let decision_payload = decision_fact.get::<_, Value>("decision_payload_json");
    assert_eq!(
        decision_payload["internal_review_reason"],
        decision_payload_sentinel
    );
    assert_eq!(
        decision_payload["raw_evidence_locator"],
        raw_evidence_locator_sentinel
    );

    let projection = client
        .query_one(
            "
            SELECT to_jsonb(review_status_view) AS projection_json
            FROM projection.review_status_views AS review_status_view
            WHERE review_case_id::text = $1
            ",
            &[&review_case_id],
        )
        .await
        .expect("review status projection must be queryable");
    let projection_json = projection.get::<_, Value>("projection_json");
    assert_eq!(
        projection_json["user_facing_status"],
        "sealed_or_restricted"
    );
    assert_eq!(
        projection_json["user_facing_reason_code"],
        "restricted_after_review"
    );
    assert_eq!(
        projection_json["latest_decision_fact_id"].as_str(),
        Some(decision_fact_id.as_str())
    );

    let participant_status = get_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/status"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(participant_status.status, StatusCode::OK);
    assert_eq!(
        participant_status.body["user_facing_status"],
        "sealed_or_restricted"
    );
    assert_eq!(
        participant_status.body["user_facing_reason_code"],
        "restricted_after_review"
    );
    assert_eq!(
        participant_status.body["latest_decision_fact_id"].as_str(),
        Some(decision_fact_id.as_str())
    );

    let forbidden_values = [
        operator_note_sentinel,
        source_snapshot_sentinel,
        decision_payload_sentinel,
        private_source_fact_sentinel,
        raw_evidence_locator_sentinel,
        "operator_note_internal",
        "source_snapshot_json",
        "decision_payload_json",
        "internal_review_reason",
        "raw_evidence_locator",
        "operator_private_context",
    ];
    let projection_text = projection_json.to_string();
    let participant_status_text = participant_status.body.to_string();
    for forbidden in forbidden_values {
        assert!(
            !projection_text.contains(forbidden),
            "review status projection must not leak operator private field: {forbidden}"
        );
        assert!(
            !participant_status_text.contains(forbidden),
            "participant review status response must not leak operator private field: {forbidden}"
        );
    }
}

#[tokio::test]
async fn operator_note_source_snapshot_does_not_mutate_settlement_writer_truth() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(
        &app,
        "pi-user-review-source-authority",
        "review-source-authority",
    )
    .await;
    let counterparty = sign_in(
        &app,
        "pi-user-review-source-authority-b",
        "review-source-authority-b",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(subject.token.as_str()),
        json!({
            "internal_idempotency_key": "operator-source-authority-promise",
            "realm_id": "realm-operator-source-authority",
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
    let original_settlement = settlement_writer_snapshot(&client, &settlement_case_id).await;
    assert_eq!(original_settlement.case_status, "pending_funding");

    let misleading_source_fact_id =
        format!("settlement_case:{settlement_case_id}:operator-note-claims-funded");
    let create_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "settlement_conflict",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_settlement_case_id": settlement_case_id,
            "related_realm_id": "realm-operator-source-authority",
            "opened_reason_code": "policy_review",
            "source_fact_kind": "settlement_case",
            "source_fact_id": misleading_source_fact_id,
            "source_snapshot_json": {
                "settlement_case_id": settlement_case_id,
                "case_status": "funded",
                "payment_receipt_status": "verified",
                "ledger_journal_count": 1,
                "repair_authority": "operator_note_internal",
                "consent_override": true,
                "social_trust_delta": 99,
                "relationship_depth_delta": 99
            },
            "request_idempotency_key": "operator-source-authority-review"
        }),
    )
    .await;
    assert_eq!(create_case.status, StatusCode::OK);
    let review_case_id = create_case.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let review_source = client
        .query_one(
            "
            SELECT source_fact_kind, source_fact_id, source_snapshot_json
            FROM dao.review_cases
            WHERE review_case_id::text = $1
            ",
            &[&review_case_id],
        )
        .await
        .expect("review case source metadata must remain readable");
    assert_eq!(
        review_source.get::<_, String>("source_fact_kind"),
        "settlement_case"
    );
    assert_eq!(
        review_source.get::<_, String>("source_fact_id"),
        misleading_source_fact_id
    );
    let source_snapshot = review_source.get::<_, Value>("source_snapshot_json");
    assert_eq!(source_snapshot["case_status"], "funded");
    assert_eq!(
        source_snapshot["repair_authority"],
        "operator_note_internal"
    );

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "no_action",
            "user_facing_reason_code": "resolved_no_action",
            "operator_note_internal": format!(
                "operator note claims settlement {settlement_case_id} is funded and repairable"
            ),
            "decision_payload_json": {
                "settlement_case_id": settlement_case_id,
                "case_status": "funded",
                "payment_receipt_status": "verified",
                "ledger_journal_count": 1,
                "repair_authority": "operator_note_internal",
                "consent_override": true,
                "social_trust_delta": 99,
                "relationship_depth_delta": 99
            },
            "decision_idempotency_key": "operator-source-authority-decision"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);
    let decision_fact_id = decision.body["operator_decision_fact_id"]
        .as_str()
        .expect("decision fact id must exist")
        .to_owned();

    let decision_fact = client
        .query_one(
            "
            SELECT operator_note_internal, decision_payload_json
            FROM dao.operator_decision_facts
            WHERE operator_decision_fact_id::text = $1
            ",
            &[&decision_fact_id],
        )
        .await
        .expect("operator decision fact must remain readable");
    let operator_note = decision_fact
        .get::<_, Option<String>>("operator_note_internal")
        .expect("operator note fixture must be stored on the operator decision fact");
    assert!(operator_note.contains("is funded and repairable"));
    let decision_payload = decision_fact.get::<_, Value>("decision_payload_json");
    assert_eq!(decision_payload["case_status"], "funded");
    assert_eq!(
        decision_payload["repair_authority"],
        "operator_note_internal"
    );

    assert_eq!(decision_count(&client, &review_case_id).await, 1);
    assert_eq!(
        settlement_writer_snapshot(&client, &settlement_case_id).await,
        original_settlement,
        "operator notes and source snapshots must not mutate settlement writer truth"
    );
}

#[tokio::test]
async fn distinct_operator_decisions_append_new_facts_without_rewriting_source_truth() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-review-append", "review-append").await;
    let counterparty = sign_in(&app, "pi-user-review-append-b", "review-append-b").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(subject.token.as_str()),
        json!({
            "internal_idempotency_key": "operator-review-append-promise",
            "realm_id": "realm-operator-review-append",
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
    let original_settlement_status = settlement_status(&client, &settlement_case_id).await;

    let create_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "settlement_conflict",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_settlement_case_id": settlement_case_id,
            "related_realm_id": "realm-operator-review-append",
            "opened_reason_code": "policy_review",
            "source_fact_kind": "settlement_case",
            "source_fact_id": settlement_case_id,
            "source_snapshot_json": {
                "case_status": original_settlement_status
            },
            "request_idempotency_key": "review-case-append"
        }),
    )
    .await;
    assert_eq!(create_case.status, StatusCode::OK);
    let review_case_id = create_case.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let request_more_evidence = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "request_more_evidence",
            "user_facing_reason_code": "proof_inconclusive",
            "operator_note_internal": "requesting private details for internal review",
            "decision_payload_json": {
                "requested": "bounded evidence summary"
            },
            "decision_idempotency_key": "append-decision-001"
        }),
    )
    .await;
    assert_eq!(request_more_evidence.status, StatusCode::OK);

    let restore = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restore",
            "user_facing_reason_code": "restored_after_review",
            "operator_note_internal": "restoration rationale is internal",
            "decision_payload_json": {
                "resolution": "restore"
            },
            "decision_idempotency_key": "append-decision-002"
        }),
    )
    .await;
    assert_eq!(restore.status, StatusCode::OK);
    assert_ne!(
        request_more_evidence.body["operator_decision_fact_id"],
        restore.body["operator_decision_fact_id"]
    );
    assert_eq!(decision_count(&client, &review_case_id).await, 2);
    assert_eq!(
        settlement_status(&client, &settlement_case_id).await,
        original_settlement_status
    );

    let status = get_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/status"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(status.status, StatusCode::OK);
    assert_eq!(status.body["user_facing_status"], "appeal_available");
    assert_eq!(
        status.body["user_facing_reason_code"],
        "restored_after_review"
    );
    assert_eq!(
        status.body["latest_decision_fact_id"],
        restore.body["operator_decision_fact_id"]
    );
}

#[tokio::test]
async fn review_case_idempotency_replay_accepts_canonical_json_key_order() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(
        &app,
        "pi-user-review-canonical-json-v2",
        "review-canonical-json-v2",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let first_body = format!(
        r#"{{
            "case_type": "proof_anomaly",
            "severity": "sev1",
            "subject_account_id": "{subject_account_id}",
            "related_realm_id": "realm-review-canonical-json-v2",
            "opened_reason_code": "proof_inconclusive",
            "source_fact_kind": "proof_submission",
            "source_fact_id": "proof-source-canonical-json-v2",
            "source_snapshot_json": {{
                "outer": {{ "b": 2, "a": 1 }},
                "array": [{{ "z": 3, "y": 2 }}]
            }},
            "request_idempotency_key": "review-case-canonical-json-v2"
        }}"#,
        subject_account_id = subject.account_id
    );
    let first = operator_post_raw_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        &first_body,
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    let review_case_id = first.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let replay_body = format!(
        r#"{{
            "request_idempotency_key": "review-case-canonical-json-v2",
            "source_snapshot_json": {{
                "array": [{{ "y": 2, "z": 3 }}],
                "outer": {{ "a": 1, "b": 2 }}
            }},
            "source_fact_id": "proof-source-canonical-json-v2",
            "source_fact_kind": "proof_submission",
            "opened_reason_code": "proof_inconclusive",
            "related_realm_id": "realm-review-canonical-json-v2",
            "subject_account_id": "{subject_account_id}",
            "severity": "sev1",
            "case_type": "proof_anomaly"
        }}"#,
        subject_account_id = subject.account_id
    );
    let replay = operator_post_raw_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        &replay_body,
    )
    .await;
    assert_eq!(replay.status, StatusCode::OK);
    assert_eq!(replay.body["review_case_id"], review_case_id);
    assert_eq!(review_case_count(&client, &review_case_id).await, 1);
}

#[tokio::test]
async fn review_case_idempotency_replay_backfills_missing_payload_hash() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-review-null-hash-v2", "review-null-hash-v2").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let request = json!({
        "case_type": "proof_anomaly",
        "severity": "sev1",
        "subject_account_id": subject.account_id,
        "related_realm_id": "realm-review-null-hash-v2",
        "opened_reason_code": "proof_inconclusive",
        "source_fact_kind": "proof_submission",
        "source_fact_id": "proof-source-null-hash-v2",
        "source_snapshot_json": {
            "source": "proof"
        },
        "request_idempotency_key": "review-case-null-hash-v2"
    });

    let first = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        request.clone(),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    let review_case_id = first.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    client
        .execute(
            "
            UPDATE dao.review_cases
            SET request_payload_hash = NULL,
                related_realm_id = '  realm-review-null-hash-v2  '
            WHERE review_case_id::text = $1
            ",
            &[&review_case_id],
        )
        .await
        .expect("review case payload hash must be nullable for legacy rows");

    let replay = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        request,
    )
    .await;
    assert_eq!(replay.status, StatusCode::OK);
    assert_eq!(replay.body["review_case_id"], review_case_id);
    assert_eq!(review_case_count(&client, &review_case_id).await, 1);
    let row = client
        .query_one(
            "
            SELECT request_payload_hash
            FROM dao.review_cases
            WHERE review_case_id::text = $1
            ",
            &[&review_case_id],
        )
        .await
        .expect("review case must exist");
    let stored_hash: Option<String> = row.get("request_payload_hash");
    assert!(stored_hash.is_some());
}

#[tokio::test]
async fn reused_review_case_idempotency_key_rejects_mismatched_payload() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(
        &app,
        "pi-user-review-case-mismatch-v2",
        "review-case-mismatch-v2",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let first = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "proof_anomaly",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-review-case-mismatch-v2",
            "opened_reason_code": "proof_inconclusive",
            "source_fact_kind": "proof_submission",
            "source_fact_id": "proof-source-case-mismatch-v2",
            "source_snapshot_json": {
                "source": "proof",
                "version": 1
            },
            "request_idempotency_key": "review-case-mismatch-v2"
        }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    let review_case_id = first.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let second = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "proof_anomaly",
            "severity": "sev2",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-review-case-mismatch-v2",
            "opened_reason_code": "proof_inconclusive",
            "source_fact_kind": "proof_submission",
            "source_fact_id": "proof-source-case-mismatch-v2",
            "source_snapshot_json": {
                "source": "proof",
                "version": 2
            },
            "request_idempotency_key": "review-case-mismatch-v2"
        }),
    )
    .await;
    assert_eq!(second.status, StatusCode::BAD_REQUEST);
    assert_eq!(review_case_count(&client, &review_case_id).await, 1);
}

#[tokio::test]
async fn reused_decision_idempotency_key_rejects_mismatched_payload() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-review-mismatch", "review-mismatch").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let create_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "proof_anomaly",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-review-mismatch",
            "opened_reason_code": "proof_inconclusive",
            "source_fact_kind": "proof_submission",
            "source_fact_id": "proof-source-mismatch",
            "source_snapshot_json": {
                "source": "proof"
            },
            "request_idempotency_key": "review-case-mismatch"
        }),
    )
    .await;
    assert_eq!(create_case.status, StatusCode::OK);
    let review_case_id = create_case.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let first = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "restricted_after_review",
            "operator_note_internal": "first decision",
            "decision_payload_json": {
                "resolution": "restrict"
            },
            "decision_idempotency_key": "decision-mismatch"
        }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);

    let second = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restore",
            "user_facing_reason_code": "restored_after_review",
            "operator_note_internal": "second decision should fail",
            "decision_payload_json": {
                "resolution": "restore"
            },
            "decision_idempotency_key": "decision-mismatch"
        }),
    )
    .await;
    assert_eq!(second.status, StatusCode::BAD_REQUEST);
    assert_eq!(decision_count(&client, &review_case_id).await, 1);
}

#[tokio::test]
async fn decision_idempotency_replay_backfills_missing_payload_hash() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(
        &app,
        "pi-user-review-decision-null-hash-v2",
        "review-decision-null-hash-v2",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let create_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "proof_anomaly",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-review-decision-null-hash-v2",
            "opened_reason_code": "proof_inconclusive",
            "source_fact_kind": "proof_submission",
            "source_fact_id": "proof-source-decision-null-hash-v2",
            "source_snapshot_json": {
                "source": "proof"
            },
            "request_idempotency_key": "review-case-decision-null-hash-v2"
        }),
    )
    .await;
    assert_eq!(create_case.status, StatusCode::OK);
    let review_case_id = create_case.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let request = json!({
        "decision_kind": "restrict",
        "user_facing_reason_code": "restricted_after_review",
        "operator_note_internal": "legacy null hash replay",
        "decision_payload_json": {
            "resolution": "restrict"
        },
        "decision_idempotency_key": "decision-null-hash-v2"
    });
    let first = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        request.clone(),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    let decision_fact_id = first.body["operator_decision_fact_id"]
        .as_str()
        .expect("decision fact id must exist")
        .to_owned();

    client
        .execute(
            "
            UPDATE dao.operator_decision_facts
            SET decision_payload_hash = NULL
            WHERE operator_decision_fact_id::text = $1
            ",
            &[&decision_fact_id],
        )
        .await
        .expect("decision payload hash must be nullable for legacy rows");

    let replay = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        request,
    )
    .await;
    assert_eq!(replay.status, StatusCode::OK);
    assert_eq!(replay.body["operator_decision_fact_id"], decision_fact_id);
    assert_eq!(decision_count(&client, &review_case_id).await, 1);
    let row = client
        .query_one(
            "
            SELECT decision_payload_hash
            FROM dao.operator_decision_facts
            WHERE operator_decision_fact_id::text = $1
            ",
            &[&decision_fact_id],
        )
        .await
        .expect("decision fact must exist");
    let stored_hash: Option<String> = row.get("decision_payload_hash");
    assert!(stored_hash.is_some());
}

#[tokio::test]
async fn reused_appeal_idempotency_key_rejects_mismatched_payload() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(
        &app,
        "pi-user-review-appeal-mismatch-v2",
        "review-appeal-mismatch-v2",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let create_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "proof_anomaly",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-review-appeal-mismatch-v2",
            "opened_reason_code": "proof_inconclusive",
            "source_fact_kind": "proof_submission",
            "source_fact_id": "proof-source-appeal-mismatch-v2",
            "source_snapshot_json": {
                "source": "proof"
            },
            "request_idempotency_key": "review-case-appeal-mismatch-v2"
        }),
    )
    .await;
    assert_eq!(create_case.status, StatusCode::OK);
    let review_case_id = create_case.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "restricted_after_review",
            "operator_note_internal": "private restriction detail",
            "decision_payload_json": {
                "resolution": "restrict"
            },
            "decision_idempotency_key": "decision-appeal-mismatch-v2"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);
    let decision_fact_id = decision.body["operator_decision_fact_id"]
        .as_str()
        .expect("decision fact id must exist")
        .to_owned();

    let first = post_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/appeals"),
        Some(subject.token.as_str()),
        json!({
            "source_decision_fact_id": decision_fact_id,
            "submitted_reason_code": "appeal_received",
            "appellant_statement": "I have new context.",
            "new_evidence_summary_json": {
                "safe_summary": "first appeal summary"
            },
            "appeal_idempotency_key": "appeal-mismatch-v2"
        }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);

    let second = post_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/appeals"),
        Some(subject.token.as_str()),
        json!({
            "source_decision_fact_id": decision_fact_id,
            "submitted_reason_code": "appeal_received",
            "appellant_statement": "A different appeal payload should fail.",
            "new_evidence_summary_json": {
                "safe_summary": "second appeal summary"
            },
            "appeal_idempotency_key": "appeal-mismatch-v2"
        }),
    )
    .await;
    assert_eq!(second.status, StatusCode::BAD_REQUEST);
    assert_eq!(appeal_count(&client, &review_case_id).await, 1);
}

#[tokio::test]
async fn appeal_idempotency_replay_backfills_missing_payload_hash() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(
        &app,
        "pi-user-review-appeal-null-hash-v2",
        "review-appeal-null-hash-v2",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let create_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "proof_anomaly",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-review-appeal-null-hash-v2",
            "opened_reason_code": "proof_inconclusive",
            "source_fact_kind": "proof_submission",
            "source_fact_id": "proof-source-appeal-null-hash-v2",
            "source_snapshot_json": {
                "source": "proof"
            },
            "request_idempotency_key": "review-case-appeal-null-hash-v2"
        }),
    )
    .await;
    assert_eq!(create_case.status, StatusCode::OK);
    let review_case_id = create_case.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "restricted_after_review",
            "operator_note_internal": "appeal null hash setup",
            "decision_payload_json": {
                "resolution": "restrict"
            },
            "decision_idempotency_key": "decision-appeal-null-hash-v2"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);
    let decision_fact_id = decision.body["operator_decision_fact_id"]
        .as_str()
        .expect("decision fact id must exist")
        .to_owned();

    let request = json!({
        "source_decision_fact_id": decision_fact_id,
        "submitted_reason_code": "appeal_received",
        "appellant_statement": "I have new context.",
        "new_evidence_summary_json": {
            "safe_summary": "legacy null hash replay"
        },
        "appeal_idempotency_key": "appeal-null-hash-v2"
    });
    let first = post_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/appeals"),
        Some(subject.token.as_str()),
        request.clone(),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    let appeal_case_id = first.body["appeal_case_id"]
        .as_str()
        .expect("appeal_case_id must exist")
        .to_owned();

    client
        .execute(
            "
            UPDATE dao.appeal_cases
            SET appeal_payload_hash = NULL
            WHERE appeal_case_id::text = $1
            ",
            &[&appeal_case_id],
        )
        .await
        .expect("appeal payload hash must be nullable for legacy rows");

    let replay = post_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/appeals"),
        Some(subject.token.as_str()),
        request,
    )
    .await;
    assert_eq!(replay.status, StatusCode::OK);
    assert_eq!(replay.body["appeal_case_id"], appeal_case_id);
    assert_eq!(appeal_count(&client, &review_case_id).await, 1);
    let row = client
        .query_one(
            "
            SELECT appeal_payload_hash
            FROM dao.appeal_cases
            WHERE appeal_case_id::text = $1
            ",
            &[&appeal_case_id],
        )
        .await
        .expect("appeal case must exist");
    let stored_hash: Option<String> = row.get("appeal_payload_hash");
    assert!(stored_hash.is_some());
}

#[tokio::test]
async fn concurrent_decision_replay_returns_existing_fact_across_two_app_states() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let database_url = std::env::var("MUSUBI_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("test database url must be present");
    let config = DbConfig::from_lookup(|name| match name {
        "APP_ENV" => Some("local".to_owned()),
        "DATABASE_URL" => Some(database_url.clone()),
        _ => std::env::var(name).ok(),
    })
    .expect("db config");
    let second_state = new_state_from_config(&config).await.expect("second state");
    let second_app = build_app(second_state.clone());
    let subject = sign_in(&app, "pi-user-review-concurrent", "review-concurrent").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let create_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "proof_anomaly",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-review-concurrent",
            "opened_reason_code": "proof_inconclusive",
            "source_fact_kind": "proof_submission",
            "source_fact_id": "proof-source-concurrent",
            "source_snapshot_json": {
                "source": "proof"
            },
            "request_idempotency_key": "review-case-concurrent"
        }),
    )
    .await;
    assert_eq!(create_case.status, StatusCode::OK);
    let review_case_id = create_case.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let path = format!("/api/internal/operator/review-cases/{review_case_id}/decisions");
    let body = json!({
        "decision_kind": "restrict",
        "user_facing_reason_code": "restricted_after_review",
        "operator_note_internal": "concurrent replay",
        "decision_payload_json": {
            "resolution": "restrict"
        },
        "decision_idempotency_key": "decision-concurrent"
    });
    let (first, second) = tokio::join!(
        operator_post_json(&app, &path, &approver_id, body.clone()),
        operator_post_json(&second_app, &path, &approver_id, body)
    );
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(
        first.body["operator_decision_fact_id"],
        second.body["operator_decision_fact_id"]
    );
    assert_eq!(decision_count(&client, &review_case_id).await, 1);
}

#[tokio::test]
async fn evidence_refresh_repairs_stale_case_status_from_latest_decision_fact() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-review-stale", "review-stale").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let create_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "proof_anomaly",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-review-stale",
            "opened_reason_code": "proof_inconclusive",
            "source_fact_kind": "proof_submission",
            "source_fact_id": "proof-source-stale",
            "source_snapshot_json": {
                "source": "proof"
            },
            "request_idempotency_key": "review-case-stale"
        }),
    )
    .await;
    assert_eq!(create_case.status, StatusCode::OK);
    let review_case_id = create_case.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "restricted_after_review",
            "operator_note_internal": "final restriction",
            "decision_payload_json": {
                "resolution": "restrict"
            },
            "decision_idempotency_key": "decision-stale"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);

    client
        .execute(
            "
            UPDATE dao.review_cases
            SET review_status = 'awaiting_evidence',
                updated_at = CURRENT_TIMESTAMP
            WHERE review_case_id::text = $1
            ",
            &[&review_case_id],
        )
        .await
        .expect("stale review status must update");

    let attach_evidence = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/evidence-bundles"),
        &approver_id,
        json!({
            "evidence_visibility": "summary_only",
            "summary_json": {
                "safe_summary": "bounded summary"
            },
            "raw_locator_json": {},
            "retention_class": "R4"
        }),
    )
    .await;
    assert_eq!(attach_evidence.status, StatusCode::OK);

    let status = get_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/status"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(status.status, StatusCode::OK);
    assert_eq!(status.body["user_facing_status"], "sealed_or_restricted");
    assert_eq!(status.body["appeal_status"], "appeal_available");
    assert_eq!(status.body["appeal_available"], true);
    assert_eq!(
        status.body["latest_decision_fact_id"],
        decision.body["operator_decision_fact_id"]
    );

    let review_detail = operator_get_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}"),
        &approver_id,
    )
    .await;
    assert_eq!(review_detail.status, StatusCode::OK);
    assert_eq!(
        review_detail.body["review_case"]["review_status"],
        "decided"
    );
}

#[tokio::test]
async fn request_more_evidence_decision_is_not_appealable() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(
        &app,
        "pi-user-review-request-more-evidence",
        "review-request-more-evidence",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let create_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "proof_anomaly",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-review-request-more-evidence",
            "opened_reason_code": "proof_inconclusive",
            "source_fact_kind": "proof_submission",
            "source_fact_id": "proof-source-request-more-evidence",
            "source_snapshot_json": {
                "source": "proof"
            },
            "request_idempotency_key": "review-case-request-more-evidence"
        }),
    )
    .await;
    assert_eq!(create_case.status, StatusCode::OK);
    let review_case_id = create_case.body["review_case_id"]
        .as_str()
        .expect("review_case_id must exist")
        .to_owned();

    let request_more_evidence = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "request_more_evidence",
            "user_facing_reason_code": "proof_inconclusive",
            "operator_note_internal": "need more proof context",
            "decision_payload_json": {
                "requested": "bounded evidence summary"
            },
            "decision_idempotency_key": "decision-request-more-evidence"
        }),
    )
    .await;
    assert_eq!(request_more_evidence.status, StatusCode::OK);
    let decision_fact_id = request_more_evidence.body["operator_decision_fact_id"]
        .as_str()
        .expect("decision fact id must exist")
        .to_owned();

    let appeal = post_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/appeals"),
        Some(subject.token.as_str()),
        json!({
            "source_decision_fact_id": decision_fact_id,
            "submitted_reason_code": "appeal_received",
            "appellant_statement": "I want to appeal before sending more proof.",
            "new_evidence_summary_json": {
                "safe_summary": "appeal should be rejected for non-final decisions"
            },
            "appeal_idempotency_key": "appeal-request-more-evidence"
        }),
    )
    .await;
    assert_eq!(appeal.status, StatusCode::BAD_REQUEST);

    let status = get_json(
        &app,
        &format!("/api/review-cases/{review_case_id}/status"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(status.status, StatusCode::OK);
    assert_eq!(status.body["user_facing_status"], "evidence_requested");
    assert_eq!(status.body["appeal_status"], "none");
    assert_eq!(status.body["appeal_available"], false);
}

struct SignedInUser {
    token: String,
    account_id: String,
}

struct JsonResponse {
    status: StatusCode,
    body: Value,
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

async fn insert_operator_account(client: &tokio_postgres::Client, role: &str) -> String {
    let account_id = Uuid::new_v4();
    client
        .execute(
            "
            INSERT INTO core.accounts (account_id, account_class, account_state)
            VALUES ($1, 'Controlled Exceptional Account', 'active')
            ",
            &[&account_id],
        )
        .await
        .expect("operator account must insert");
    client
        .execute(
            "
            INSERT INTO core.operator_role_assignments (
                operator_role_assignment_id,
                operator_account_id,
                operator_role,
                grant_reason
            )
            VALUES ($1, $2, $3, 'operator review test role')
            ",
            &[&Uuid::new_v4(), &account_id, &role],
        )
        .await
        .expect("operator role assignment must insert");
    account_id.to_string()
}

async fn settlement_status(client: &tokio_postgres::Client, settlement_case_id: &str) -> String {
    client
        .query_one(
            "
            SELECT case_status
            FROM dao.settlement_cases
            WHERE settlement_case_id::text = $1
            ",
            &[&settlement_case_id],
        )
        .await
        .expect("settlement case must exist")
        .get("case_status")
}

#[derive(Debug, PartialEq, Eq)]
struct SettlementWriterSnapshot {
    case_status: String,
    payment_receipt_count: i64,
    journal_count: i64,
    posting_count: i64,
    observation_count: i64,
    submission_count: i64,
}

async fn settlement_writer_snapshot(
    client: &tokio_postgres::Client,
    settlement_case_id: &str,
) -> SettlementWriterSnapshot {
    let row = client
        .query_one(
            "
            SELECT
                settlement.case_status,
                (SELECT count(*) FROM core.payment_receipts receipt
                 WHERE receipt.settlement_case_id::text = $1) AS payment_receipt_count,
                (SELECT count(*) FROM ledger.journal_entries journal
                 WHERE journal.settlement_case_id::text = $1) AS journal_count,
                (SELECT count(*)
                 FROM ledger.account_postings posting
                 JOIN ledger.journal_entries journal
                   ON journal.journal_entry_id = posting.journal_entry_id
                 WHERE journal.settlement_case_id::text = $1) AS posting_count,
                (SELECT count(*) FROM dao.settlement_observations observation
                 WHERE observation.settlement_case_id::text = $1) AS observation_count,
                (SELECT count(*) FROM dao.settlement_submissions submission
                 WHERE submission.settlement_case_id::text = $1) AS submission_count
            FROM dao.settlement_cases settlement
            WHERE settlement.settlement_case_id::text = $1
            ",
            &[&settlement_case_id],
        )
        .await
        .expect("settlement writer snapshot must be queryable");

    SettlementWriterSnapshot {
        case_status: row.get("case_status"),
        payment_receipt_count: row.get("payment_receipt_count"),
        journal_count: row.get("journal_count"),
        posting_count: row.get("posting_count"),
        observation_count: row.get("observation_count"),
        submission_count: row.get("submission_count"),
    }
}

async fn decision_count(client: &tokio_postgres::Client, review_case_id: &str) -> i64 {
    client
        .query_one(
            "
            SELECT count(*) AS count
            FROM dao.operator_decision_facts
            WHERE review_case_id::text = $1
            ",
            &[&review_case_id],
        )
        .await
        .expect("operator decision fact count must be queryable")
        .get("count")
}

async fn review_case_count(client: &tokio_postgres::Client, review_case_id: &str) -> i64 {
    client
        .query_one(
            "
            SELECT count(*) AS count
            FROM dao.review_cases
            WHERE review_case_id::text = $1
            ",
            &[&review_case_id],
        )
        .await
        .expect("review case count must be queryable")
        .get("count")
}

async fn appeal_count(client: &tokio_postgres::Client, review_case_id: &str) -> i64 {
    client
        .query_one(
            "
            SELECT count(*) AS count
            FROM dao.appeal_cases
            WHERE source_review_case_id::text = $1
            ",
            &[&review_case_id],
        )
        .await
        .expect("appeal count must be queryable")
        .get("count")
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

fn future_timestamp() -> String {
    (chrono::Utc::now() + chrono::Duration::hours(2)).to_rfc3339()
}

async fn operator_post_json(
    app: &Router,
    path: &str,
    operator_id: &str,
    body: Value,
) -> JsonResponse {
    request_json(app, "POST", path, None, Some(operator_id), Some(body)).await
}

async fn operator_post_raw_json(
    app: &Router,
    path: &str,
    operator_id: &str,
    body: &str,
) -> JsonResponse {
    request_raw_json(app, "POST", path, None, Some(operator_id), Some(body)).await
}

async fn operator_get_json(app: &Router, path: &str, operator_id: &str) -> JsonResponse {
    request_json(app, "GET", path, None, Some(operator_id), None).await
}

async fn post_json(
    app: &Router,
    path: &str,
    bearer_token: Option<&str>,
    body: Value,
) -> JsonResponse {
    request_json(app, "POST", path, bearer_token, None, Some(body)).await
}

async fn get_json(app: &Router, path: &str, bearer_token: Option<&str>) -> JsonResponse {
    request_json(app, "GET", path, bearer_token, None, None).await
}

async fn request_json(
    app: &Router,
    method: &str,
    path: &str,
    bearer_token: Option<&str>,
    operator_id: Option<&str>,
    body: Option<Value>,
) -> JsonResponse {
    let raw_body = body.map(|body| body.to_string());
    request_raw_json(
        app,
        method,
        path,
        bearer_token,
        operator_id,
        raw_body.as_deref(),
    )
    .await
}

async fn request_raw_json(
    app: &Router,
    method: &str,
    path: &str,
    bearer_token: Option<&str>,
    operator_id: Option<&str>,
    body: Option<&str>,
) -> JsonResponse {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = bearer_token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    if let Some(operator_id) = operator_id {
        builder = builder.header("x-musubi-operator-id", operator_id);
    }

    let request = builder
        .header("content-type", "application/json")
        .body(match body {
            Some(body) => Body::from(body.to_owned()),
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
