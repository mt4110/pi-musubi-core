use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use musubi_backend::{build_app, new_test_state};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn room_progression_follows_normal_path_and_keeps_view_private() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-normal-a", "room-normal-a").await;
    let counterparty = sign_in(&app, "pi-user-room-normal-b", "room-normal-b").await;
    let outsider = sign_in(&app, "pi-user-room-normal-c", "room-normal-c").await;

    let room = internal_post_json(
        &app,
        "/api/internal/room-progressions",
        json!({
            "realm_id": "realm-room-normal",
            "participant_account_ids": [
                subject.account_id,
                counterparty.account_id
            ],
            "user_facing_reason_code": "room_created",
            "source_fact_kind": "intent_room_request",
            "source_fact_id": "room-normal-source",
            "source_snapshot_json": {
                "private_internal_note": "must not leak"
            },
            "request_idempotency_key": "room-normal-create"
        }),
    )
    .await;
    assert_eq!(room.status, StatusCode::OK);
    assert_eq!(room.body["current_stage"], "intent");
    let room_progression_id = room.body["room_progression_id"]
        .as_str()
        .expect("room progression id must exist")
        .to_owned();

    let outsider_view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(outsider.token.as_str()),
    )
    .await;
    assert_eq!(outsider_view.status, StatusCode::NOT_FOUND);

    let coordination = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "mutual_intent_acknowledged",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-normal-coordinate",
            "source_snapshot_json": {
                "operator_internal_note": "must not leak"
            },
            "fact_idempotency_key": "room-normal-coordinate"
        }),
    )
    .await;
    assert_eq!(coordination.status, StatusCode::OK);
    assert_eq!(coordination.body["from_stage"], "intent");
    assert_eq!(coordination.body["to_stage"], "coordination");

    let relationship = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_relationship",
            "to_stage": "relationship",
            "user_facing_reason_code": "qualifying_promise_completed",
            "triggered_by_kind": "system",
            "source_fact_kind": "qualifying_promise_completion",
            "source_fact_id": "room-normal-relationship",
            "source_snapshot_json": {
                "raw_source_snapshot": "must not leak"
            },
            "fact_idempotency_key": "room-normal-relationship"
        }),
    )
    .await;
    assert_eq!(relationship.status, StatusCode::OK);

    let view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(view.status, StatusCode::OK);
    assert_eq!(view.body["visible_stage"], "relationship");
    assert_eq!(view.body["status_code"], "relationship_open");
    assert_eq!(
        view.body["user_facing_reason_code"],
        "qualifying_promise_completed"
    );
    assert_eq!(view.body["source_fact_count"], 3);
    assert!(!view.body.to_string().contains("must not leak"));
    assert!(view.body.get("source_snapshot_json").is_none());
    assert!(view.body.get("triggered_by_account_id").is_none());
}

#[tokio::test]
async fn room_progression_view_treats_invalid_id_as_not_found() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-invalid-view-a", "room-invalid-view-a").await;

    let response = get_json(
        &app,
        "/api/projection/room-progression-views/not-a-uuid",
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn room_progression_create_requires_idempotency_key() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-create-key-a", "room-create-key-a").await;
    let counterparty = sign_in(&app, "pi-user-room-create-key-b", "room-create-key-b").await;

    let response = internal_post_json(
        &app,
        "/api/internal/room-progressions",
        json!({
            "realm_id": "realm-room-create-key",
            "participant_account_ids": [
                subject.account_id,
                counterparty.account_id
            ],
            "user_facing_reason_code": "room_created",
            "source_fact_kind": "intent_room_request",
            "source_fact_id": "room-create-key-source",
            "source_snapshot_json": {}
        }),
    )
    .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn mute_transition_preserves_visible_stage() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-mute-a", "room-mute-a").await;
    let counterparty = sign_in(&app, "pi-user-room-mute-b", "room-mute-b").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let coordination = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "mutual_intent_acknowledged",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-mute-coordinate",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-mute-coordinate"
        }),
    )
    .await;
    assert_eq!(coordination.status, StatusCode::OK);

    let muted = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "mute",
            "to_stage": "coordination",
            "user_facing_reason_code": "user_muted",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "participant_safety_control",
            "source_fact_id": "room-mute-toggle",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-mute-toggle"
        }),
    )
    .await;
    assert_eq!(muted.status, StatusCode::OK);
    assert_eq!(muted.body["from_stage"], "coordination");
    assert_eq!(muted.body["to_stage"], "coordination");
    assert_eq!(muted.body["status_code"], "muted");

    let view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(view.status, StatusCode::OK);
    assert_eq!(view.body["visible_stage"], "coordination");
    assert_eq!(view.body["status_code"], "muted");
}

#[tokio::test]
async fn blocked_room_becomes_terminal_without_changing_stage() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-block-a", "room-block-a").await;
    let counterparty = sign_in(&app, "pi-user-room-block-b", "room-block-b").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let blocked = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "block",
            "to_stage": "intent",
            "user_facing_reason_code": "user_blocked",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "participant_safety_control",
            "source_fact_id": "room-block-toggle",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-block-toggle"
        }),
    )
    .await;
    assert_eq!(blocked.status, StatusCode::OK);
    assert_eq!(blocked.body["from_stage"], "intent");
    assert_eq!(blocked.body["to_stage"], "intent");
    assert_eq!(blocked.body["status_code"], "blocked");

    let view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(view.status, StatusCode::OK);
    assert_eq!(view.body["visible_stage"], "intent");
    assert_eq!(view.body["status_code"], "blocked");

    let progression_after_block = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "mutual_intent_acknowledged",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-block-after",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-block-after"
        }),
    )
    .await;
    assert_eq!(progression_after_block.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn withdrawn_room_becomes_terminal_without_changing_stage() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-withdraw-a", "room-withdraw-a").await;
    let counterparty = sign_in(&app, "pi-user-room-withdraw-b", "room-withdraw-b").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let coordination = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "mutual_intent_acknowledged",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-withdraw-coordinate",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-withdraw-coordinate"
        }),
    )
    .await;
    assert_eq!(coordination.status, StatusCode::OK);

    let withdrawn = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "withdraw",
            "to_stage": "coordination",
            "user_facing_reason_code": "user_withdrew",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "participant_safety_control",
            "source_fact_id": "room-withdraw-toggle",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-withdraw-toggle"
        }),
    )
    .await;
    assert_eq!(withdrawn.status, StatusCode::OK);
    assert_eq!(withdrawn.body["from_stage"], "coordination");
    assert_eq!(withdrawn.body["to_stage"], "coordination");
    assert_eq!(withdrawn.body["status_code"], "withdrawn");

    let view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(view.status, StatusCode::OK);
    assert_eq!(view.body["visible_stage"], "coordination");
    assert_eq!(view.body["status_code"], "withdrawn");

    let progression_after_withdraw = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_relationship",
            "to_stage": "relationship",
            "user_facing_reason_code": "qualifying_promise_completed",
            "triggered_by_kind": "system",
            "source_fact_kind": "qualifying_promise_completion",
            "source_fact_id": "room-withdraw-after",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-withdraw-after"
        }),
    )
    .await;
    assert_eq!(progression_after_withdraw.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn room_progression_rejects_skipped_transition() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-skip-a", "room-skip-a").await;
    let counterparty = sign_in(&app, "pi-user-room-skip-b", "room-skip-b").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let skipped = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_relationship",
            "to_stage": "relationship",
            "user_facing_reason_code": "qualifying_promise_completed",
            "triggered_by_kind": "system",
            "source_fact_kind": "invalid_skip",
            "source_fact_id": "room-skip-invalid",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-skip-invalid"
        }),
    )
    .await;
    assert_eq!(skipped.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sealed_fallback_links_review_without_leaking_evidence_or_notes() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-sealed-a", "room-sealed-a").await;
    let counterparty = sign_in(&app, "pi-user-room-sealed-b", "room-sealed-b").await;
    let client = test_db_client().await;
    let reviewer_id = insert_operator_account(&client, "reviewer").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let review_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &reviewer_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": room_progression_id,
            "source_snapshot_json": {
                "raw_evidence_locator": "private-room-evidence-uri",
                "internal_safety_classification": "do-not-display"
            },
            "request_idempotency_key": "room-sealed-review"
        }),
    )
    .await;
    assert_eq!(review_case.status, StatusCode::OK);
    let review_case_id = review_case.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let sealed = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "system",
            "source_fact_kind": "review_case",
            "source_fact_id": review_case_id,
            "source_snapshot_json": {
                "operator_note_internal": "must not leak",
                "raw_evidence_locator": "private-room-evidence-uri"
            },
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-sealed"
        }),
    )
    .await;
    assert_eq!(sealed.status, StatusCode::OK);

    let view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(view.status, StatusCode::OK);
    assert_eq!(view.body["visible_stage"], "sealed");
    assert_eq!(view.body["status_code"], "sealed_under_review");
    assert_eq!(view.body["review_case_id"], review_case_id);
    assert_eq!(view.body["review_pending"], true);
    assert_eq!(view.body["review_status"], "pending_review");
    assert!(!view.body.to_string().contains("private-room-evidence-uri"));
    assert!(!view.body.to_string().contains("operator_note_internal"));
    assert!(!view.body.to_string().contains("do-not-display"));
    assert!(!view.body.to_string().contains(&reviewer_id));
}

#[tokio::test]
async fn sealed_room_can_record_restriction_follow_up_without_reopening() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-restrict-a", "room-restrict-a").await;
    let counterparty = sign_in(&app, "pi-user-room-restrict-b", "room-restrict-b").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let review_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": room_progression_id,
            "source_snapshot_json": {},
            "request_idempotency_key": "room-restrict-review"
        }),
    )
    .await;
    assert_eq!(review_case.status, StatusCode::OK);
    let review_case_id = review_case.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let sealed = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "system",
            "source_fact_kind": "review_case",
            "source_fact_id": review_case_id,
            "source_snapshot_json": {},
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restrict-sealed"
        }),
    )
    .await;
    assert_eq!(sealed.status, StatusCode::OK);

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "restricted_after_review",
            "operator_note_internal": "restriction rationale is internal",
            "decision_payload_json": {
                "resolution": "restrict"
            },
            "decision_idempotency_key": "room-restrict-decision"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);
    let decision_fact_id = decision.body["operator_decision_fact_id"]
        .as_str()
        .expect("operator decision fact id must exist")
        .to_owned();

    let restricted = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "restricted_after_review",
            "triggered_by_kind": "operator",
            "triggered_by_account_id": approver_id,
            "source_fact_kind": "operator_decision",
            "source_fact_id": decision_fact_id,
            "source_snapshot_json": {
                "resolution": "restrict"
            },
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restrict-follow-up"
        }),
    )
    .await;
    assert_eq!(restricted.status, StatusCode::OK);
    assert_eq!(restricted.body["from_stage"], "sealed");
    assert_eq!(restricted.body["to_stage"], "sealed");
    assert_eq!(restricted.body["status_code"], "sealed_restricted");

    let view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(view.status, StatusCode::OK);
    assert_eq!(view.body["visible_stage"], "sealed");
    assert_eq!(view.body["status_code"], "sealed_restricted");
    assert_eq!(view.body["review_case_id"], review_case_id);
}

#[tokio::test]
async fn restricted_seal_requires_writer_owned_restrict_decision() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-restrict-gate-a", "room-restrict-gate-a").await;
    let counterparty = sign_in(&app, "pi-user-room-restrict-gate-b", "room-restrict-gate-b").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let missing_review = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "restricted_after_review",
            "triggered_by_kind": "operator",
            "triggered_by_account_id": approver_id,
            "source_fact_kind": "operator_decision",
            "source_fact_id": "room-restrict-missing-review",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-restrict-missing-review"
        }),
    )
    .await;
    assert_eq!(missing_review.status, StatusCode::BAD_REQUEST);

    let review_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": room_progression_id,
            "source_snapshot_json": {},
            "request_idempotency_key": "room-restrict-gate-review"
        }),
    )
    .await;
    assert_eq!(review_case.status, StatusCode::OK);
    let review_case_id = review_case.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let without_decision = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "restricted_after_review",
            "triggered_by_kind": "operator",
            "triggered_by_account_id": approver_id,
            "source_fact_kind": "operator_decision",
            "source_fact_id": "room-restrict-without-decision",
            "source_snapshot_json": {},
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restrict-without-decision"
        }),
    )
    .await;
    assert_eq!(without_decision.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn live_room_seal_rejects_decided_review_case() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-stale-review-a", "room-stale-review-a").await;
    let counterparty = sign_in(&app, "pi-user-room-stale-review-b", "room-stale-review-b").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let review_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": room_progression_id,
            "source_snapshot_json": {},
            "request_idempotency_key": "room-stale-review-create"
        }),
    )
    .await;
    assert_eq!(review_case.status, StatusCode::OK);
    let review_case_id = review_case.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "restricted_after_review",
            "operator_note_internal": "restriction rationale is internal",
            "decision_payload_json": {
                "resolution": "restrict"
            },
            "decision_idempotency_key": "room-stale-review-decision"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);

    let sealed = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "system",
            "source_fact_kind": "review_case",
            "source_fact_id": review_case_id,
            "source_snapshot_json": {},
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-stale-review-seal"
        }),
    )
    .await;
    assert_eq!(sealed.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn restricted_seal_rejects_participant_actor() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(
        &app,
        "pi-user-room-restrict-actor-a",
        "room-restrict-actor-a",
    )
    .await;
    let counterparty = sign_in(
        &app,
        "pi-user-room-restrict-actor-b",
        "room-restrict-actor-b",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let review_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": room_progression_id,
            "source_snapshot_json": {},
            "request_idempotency_key": "room-restrict-actor-review"
        }),
    )
    .await;
    assert_eq!(review_case.status, StatusCode::OK);
    let review_case_id = review_case.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let sealed = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "system",
            "source_fact_kind": "review_case",
            "source_fact_id": review_case_id,
            "source_snapshot_json": {},
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restrict-actor-sealed"
        }),
    )
    .await;
    assert_eq!(sealed.status, StatusCode::OK);

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "restricted_after_review",
            "operator_note_internal": "restriction rationale is internal",
            "decision_payload_json": {
                "resolution": "restrict"
            },
            "decision_idempotency_key": "room-restrict-actor-decision"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);
    let decision_fact_id = decision.body["operator_decision_fact_id"]
        .as_str()
        .expect("operator decision fact id must exist")
        .to_owned();

    let restricted = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "restricted_after_review",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "operator_decision",
            "source_fact_id": decision_fact_id,
            "source_snapshot_json": {
                "resolution": "restrict"
            },
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restrict-actor-follow-up"
        }),
    )
    .await;
    assert_eq!(restricted.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn restricted_seal_cannot_downgrade_via_seal_follow_up() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(
        &app,
        "pi-user-room-restrict-downgrade-a",
        "room-restrict-downgrade-a",
    )
    .await;
    let counterparty = sign_in(
        &app,
        "pi-user-room-restrict-downgrade-b",
        "room-restrict-downgrade-b",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let review_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": room_progression_id,
            "source_snapshot_json": {},
            "request_idempotency_key": "room-restrict-downgrade-review"
        }),
    )
    .await;
    assert_eq!(review_case.status, StatusCode::OK);
    let review_case_id = review_case.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let sealed = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "system",
            "source_fact_kind": "review_case",
            "source_fact_id": review_case_id,
            "source_snapshot_json": {},
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restrict-downgrade-sealed"
        }),
    )
    .await;
    assert_eq!(sealed.status, StatusCode::OK);

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restrict",
            "user_facing_reason_code": "restricted_after_review",
            "operator_note_internal": "restriction rationale is internal",
            "decision_payload_json": {
                "resolution": "restrict"
            },
            "decision_idempotency_key": "room-restrict-downgrade-decision"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);
    let decision_fact_id = decision.body["operator_decision_fact_id"]
        .as_str()
        .expect("operator decision fact id must exist")
        .to_owned();

    let restricted = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "restricted_after_review",
            "triggered_by_kind": "operator",
            "triggered_by_account_id": approver_id,
            "source_fact_kind": "operator_decision",
            "source_fact_id": decision_fact_id,
            "source_snapshot_json": {
                "resolution": "restrict"
            },
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restrict-downgrade-follow-up"
        }),
    )
    .await;
    assert_eq!(restricted.status, StatusCode::OK);

    let downgrade_attempt = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "participant_retry",
            "source_fact_id": "room-restrict-downgrade-attempt",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-restrict-downgrade-attempt"
        }),
    )
    .await;
    assert_eq!(downgrade_attempt.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn fact_append_requires_idempotency_key() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-no-key-a", "room-no-key-a").await;
    let counterparty = sign_in(&app, "pi-user-room-no-key-b", "room-no-key-b").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let response = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "mutual_intent_acknowledged",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-no-key-coordinate",
            "source_snapshot_json": {}
        }),
    )
    .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn live_room_seal_requires_room_scoped_review_case() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-seal-scope-a", "room-seal-scope-a").await;
    let counterparty = sign_in(&app, "pi-user-room-seal-scope-b", "room-seal-scope-b").await;
    let outsider_subject = sign_in(&app, "pi-user-room-seal-scope-c", "room-seal-scope-c").await;
    let client = test_db_client().await;
    let reviewer_id = insert_operator_account(&client, "reviewer").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;
    let other_room_progression_id =
        create_room(&app, &subject.account_id, &outsider_subject.account_id).await;

    let missing_review_case = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "system",
            "source_fact_kind": "review_case",
            "source_fact_id": "room-seal-scope-missing",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-seal-scope-missing"
        }),
    )
    .await;
    assert_eq!(missing_review_case.status, StatusCode::BAD_REQUEST);

    let unrelated_review_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &reviewer_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": other_room_progression_id,
            "source_snapshot_json": {},
            "request_idempotency_key": "room-seal-scope-unrelated"
        }),
    )
    .await;
    assert_eq!(unrelated_review_case.status, StatusCode::OK);
    let unrelated_review_case_id = unrelated_review_case.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let wrong_scope = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "system",
            "source_fact_kind": "review_case",
            "source_fact_id": unrelated_review_case_id,
            "source_snapshot_json": {},
            "review_case_id": unrelated_review_case_id,
            "fact_idempotency_key": "room-seal-scope-wrong"
        }),
    )
    .await;
    assert_eq!(wrong_scope.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn system_triggered_facts_reject_triggered_by_account_id() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-system-actor-a", "room-system-actor-a").await;
    let counterparty = sign_in(&app, "pi-user-room-system-actor-b", "room-system-actor-b").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let invalid = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "mutual_intent_acknowledged",
            "triggered_by_kind": "system",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-system-actor-invalid",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-system-actor-invalid"
        }),
    )
    .await;
    assert_eq!(invalid.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sealed_room_cannot_rebind_review_case_on_follow_up_fact() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-rebind-a", "room-rebind-a").await;
    let counterparty = sign_in(&app, "pi-user-room-rebind-b", "room-rebind-b").await;
    let client = test_db_client().await;
    let reviewer_id = insert_operator_account(&client, "reviewer").await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let review_case_a = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &reviewer_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": room_progression_id,
            "source_snapshot_json": {},
            "request_idempotency_key": "room-rebind-review-a"
        }),
    )
    .await;
    assert_eq!(review_case_a.status, StatusCode::OK);
    let review_case_a_id = review_case_a.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let sealed = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "system",
            "source_fact_kind": "review_case",
            "source_fact_id": review_case_a_id,
            "source_snapshot_json": {},
            "review_case_id": review_case_a_id,
            "fact_idempotency_key": "room-rebind-sealed"
        }),
    )
    .await;
    assert_eq!(sealed.status, StatusCode::OK);

    let review_case_b = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &reviewer_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": room_progression_id,
            "source_snapshot_json": {},
            "request_idempotency_key": "room-rebind-review-b"
        }),
    )
    .await;
    assert_eq!(review_case_b.status, StatusCode::OK);
    let review_case_b_id = review_case_b.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let rebind_attempt = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "operator",
            "triggered_by_account_id": approver_id,
            "source_fact_kind": "review_case",
            "source_fact_id": review_case_b_id,
            "source_snapshot_json": {},
            "review_case_id": review_case_b_id,
            "fact_idempotency_key": "room-rebind-attempt"
        }),
    )
    .await;
    assert_eq!(rebind_attempt.status, StatusCode::BAD_REQUEST);

    let view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(view.status, StatusCode::OK);
    assert_eq!(view.body["review_case_id"], review_case_a_id);
}

#[tokio::test]
async fn operator_triggered_facts_require_operator_role_assignment() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-operator-role-a", "room-operator-role-a").await;
    let counterparty = sign_in(&app, "pi-user-room-operator-role-b", "room-operator-role-b").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let unauthorized = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "operator",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "operator_hold",
            "source_fact_id": "room-operator-role-missing",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-operator-role-missing"
        }),
    )
    .await;
    assert_eq!(unauthorized.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn restore_clears_review_link_from_live_room_projection() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-restore-a", "room-restore-a").await;
    let counterparty = sign_in(&app, "pi-user-room-restore-b", "room-restore-b").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let coordination = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "mutual_intent_acknowledged",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-restore-coordinate",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-restore-coordinate"
        }),
    )
    .await;
    assert_eq!(coordination.status, StatusCode::OK);

    let review_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": room_progression_id,
            "source_snapshot_json": {},
            "request_idempotency_key": "room-restore-review"
        }),
    )
    .await;
    assert_eq!(review_case.status, StatusCode::OK);
    let review_case_id = review_case.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let sealed = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "system",
            "source_fact_kind": "review_case",
            "source_fact_id": review_case_id,
            "source_snapshot_json": {},
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restore-sealed"
        }),
    )
    .await;
    assert_eq!(sealed.status, StatusCode::OK);

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restore",
            "user_facing_reason_code": "restored_after_review",
            "operator_note_internal": "restore rationale is internal",
            "decision_payload_json": {
                "resolution": "restore"
            },
            "decision_idempotency_key": "room-restore-decision"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);
    let decision_fact_id = decision.body["operator_decision_fact_id"]
        .as_str()
        .expect("operator decision fact id must exist")
        .to_owned();

    let restored = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "restore",
            "to_stage": "coordination",
            "user_facing_reason_code": "restored_after_review",
            "triggered_by_kind": "operator",
            "triggered_by_account_id": approver_id,
            "source_fact_kind": "operator_decision",
            "source_fact_id": decision_fact_id,
            "source_snapshot_json": {
                "resolution": "restore"
            },
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restore-transition"
        }),
    )
    .await;
    assert_eq!(restored.status, StatusCode::OK);
    assert_eq!(restored.body["to_stage"], "coordination");

    let restored_view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(restored_view.status, StatusCode::OK);
    assert_eq!(restored_view.body["visible_stage"], "coordination");
    assert_eq!(restored_view.body["status_code"], "coordination_open");
    assert!(restored_view.body["review_case_id"].is_null());
    assert_eq!(restored_view.body["review_pending"], false);
    assert!(restored_view.body["review_status"].is_null());

    let relationship = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_relationship",
            "to_stage": "relationship",
            "user_facing_reason_code": "qualifying_promise_completed",
            "triggered_by_kind": "system",
            "source_fact_kind": "qualifying_promise_completion",
            "source_fact_id": "room-restore-relationship",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-restore-relationship"
        }),
    )
    .await;
    assert_eq!(relationship.status, StatusCode::OK);

    let relationship_view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(relationship_view.status, StatusCode::OK);
    assert_eq!(relationship_view.body["visible_stage"], "relationship");
    assert_eq!(relationship_view.body["status_code"], "relationship_open");
    assert!(relationship_view.body["review_case_id"].is_null());
    assert!(relationship_view.body["review_status"].is_null());
}

#[tokio::test]
async fn intent_room_restore_returns_to_intent_open() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(
        &app,
        "pi-user-room-restore-intent-a",
        "room-restore-intent-a",
    )
    .await;
    let counterparty = sign_in(
        &app,
        "pi-user-room-restore-intent-b",
        "room-restore-intent-b",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let review_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": room_progression_id,
            "source_snapshot_json": {},
            "request_idempotency_key": "room-restore-intent-review"
        }),
    )
    .await;
    assert_eq!(review_case.status, StatusCode::OK);
    let review_case_id = review_case.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let sealed = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "system",
            "source_fact_kind": "review_case",
            "source_fact_id": review_case_id,
            "source_snapshot_json": {},
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restore-intent-sealed"
        }),
    )
    .await;
    assert_eq!(sealed.status, StatusCode::OK);

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restore",
            "user_facing_reason_code": "restored_after_review",
            "operator_note_internal": "restore rationale is internal",
            "decision_payload_json": {
                "resolution": "restore"
            },
            "decision_idempotency_key": "room-restore-intent-decision"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);
    let decision_fact_id = decision.body["operator_decision_fact_id"]
        .as_str()
        .expect("operator decision fact id must exist")
        .to_owned();

    let restored = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "restore",
            "to_stage": "intent",
            "user_facing_reason_code": "restored_after_review",
            "triggered_by_kind": "operator",
            "triggered_by_account_id": approver_id,
            "source_fact_kind": "operator_decision",
            "source_fact_id": decision_fact_id,
            "source_snapshot_json": {
                "resolution": "restore"
            },
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restore-intent-transition"
        }),
    )
    .await;
    assert_eq!(restored.status, StatusCode::OK);
    assert_eq!(restored.body["to_stage"], "intent");
    assert_eq!(restored.body["status_code"], "intent_open");

    let restored_view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(restored_view.status, StatusCode::OK);
    assert_eq!(restored_view.body["visible_stage"], "intent");
    assert_eq!(restored_view.body["status_code"], "intent_open");
    assert!(restored_view.body["review_case_id"].is_null());
}

#[tokio::test]
async fn restore_rejects_non_operator_actor() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-restore-actor-a", "room-restore-actor-a").await;
    let counterparty = sign_in(&app, "pi-user-room-restore-actor-b", "room-restore-actor-b").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let review_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": room_progression_id,
            "source_snapshot_json": {},
            "request_idempotency_key": "room-restore-actor-review"
        }),
    )
    .await;
    assert_eq!(review_case.status, StatusCode::OK);
    let review_case_id = review_case.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let sealed = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "system",
            "source_fact_kind": "review_case",
            "source_fact_id": review_case_id,
            "source_snapshot_json": {},
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restore-actor-sealed"
        }),
    )
    .await;
    assert_eq!(sealed.status, StatusCode::OK);

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restore",
            "user_facing_reason_code": "restored_after_review",
            "operator_note_internal": "restore rationale is internal",
            "decision_payload_json": {
                "resolution": "restore"
            },
            "decision_idempotency_key": "room-restore-actor-decision"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);
    let decision_fact_id = decision.body["operator_decision_fact_id"]
        .as_str()
        .expect("operator decision fact id must exist")
        .to_owned();

    let restored = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "restore",
            "to_stage": "intent",
            "user_facing_reason_code": "restored_after_review",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "operator_decision",
            "source_fact_id": decision_fact_id,
            "source_snapshot_json": {
                "resolution": "restore"
            },
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restore-actor-transition"
        }),
    )
    .await;
    assert_eq!(restored.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn restore_must_return_to_stage_before_current_seal() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-restore-stage-a", "room-restore-stage-a").await;
    let counterparty = sign_in(&app, "pi-user-room-restore-stage-b", "room-restore-stage-b").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let review_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "sealed_room_fallback",
            "severity": "sev1",
            "subject_account_id": subject.account_id,
            "related_realm_id": "realm-room-default",
            "opened_reason_code": "manual_hold_safety_review",
            "source_fact_kind": "room_progression",
            "source_fact_id": room_progression_id,
            "source_snapshot_json": {},
            "request_idempotency_key": "room-restore-stage-review"
        }),
    )
    .await;
    assert_eq!(review_case.status, StatusCode::OK);
    let review_case_id = review_case.body["review_case_id"]
        .as_str()
        .expect("review case id must exist")
        .to_owned();

    let sealed = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "seal",
            "to_stage": "sealed",
            "user_facing_reason_code": "manual_hold_safety_review",
            "triggered_by_kind": "system",
            "source_fact_kind": "review_case",
            "source_fact_id": review_case_id,
            "source_snapshot_json": {},
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restore-stage-sealed"
        }),
    )
    .await;
    assert_eq!(sealed.status, StatusCode::OK);

    let decision = operator_post_json(
        &app,
        &format!("/api/internal/operator/review-cases/{review_case_id}/decisions"),
        &approver_id,
        json!({
            "decision_kind": "restore",
            "user_facing_reason_code": "restored_after_review",
            "operator_note_internal": "restore rationale is internal",
            "decision_payload_json": {
                "resolution": "restore"
            },
            "decision_idempotency_key": "room-restore-stage-decision"
        }),
    )
    .await;
    assert_eq!(decision.status, StatusCode::OK);
    let decision_fact_id = decision.body["operator_decision_fact_id"]
        .as_str()
        .expect("operator decision fact id must exist")
        .to_owned();

    let restored = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "restore",
            "to_stage": "relationship",
            "user_facing_reason_code": "restored_after_review",
            "triggered_by_kind": "operator",
            "triggered_by_account_id": approver_id,
            "source_fact_kind": "operator_decision",
            "source_fact_id": decision_fact_id,
            "source_snapshot_json": {
                "resolution": "restore"
            },
            "review_case_id": review_case_id,
            "fact_idempotency_key": "room-restore-stage-transition"
        }),
    )
    .await;
    assert_eq!(restored.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn room_progression_create_replay_survives_participant_deactivation() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-replay-active-a", "room-replay-active-a").await;
    let counterparty = sign_in(&app, "pi-user-room-replay-active-b", "room-replay-active-b").await;
    let client = test_db_client().await;

    let created = internal_post_json(
        &app,
        "/api/internal/room-progressions",
        json!({
            "realm_id": "realm-room-replay-active",
            "participant_account_ids": [
                subject.account_id,
                counterparty.account_id
            ],
            "user_facing_reason_code": "room_created",
            "source_fact_kind": "intent_room_request",
            "source_fact_id": "room-replay-active-source",
            "source_snapshot_json": {},
            "request_idempotency_key": "room-replay-active-create"
        }),
    )
    .await;
    assert_eq!(created.status, StatusCode::OK);
    let room_progression_id = created.body["room_progression_id"]
        .as_str()
        .expect("room progression id must exist")
        .to_owned();

    client
        .execute(
            "
            UPDATE core.accounts
            SET account_state = 'suspended',
                updated_at = CURRENT_TIMESTAMP
            WHERE account_id::text = $1
            ",
            &[&counterparty.account_id],
        )
        .await
        .expect("account state must update");

    let replayed = internal_post_json(
        &app,
        "/api/internal/room-progressions",
        json!({
            "realm_id": "realm-room-replay-active",
            "participant_account_ids": [
                subject.account_id,
                counterparty.account_id
            ],
            "user_facing_reason_code": "room_created",
            "source_fact_kind": "intent_room_request",
            "source_fact_id": "room-replay-active-source",
            "source_snapshot_json": {},
            "request_idempotency_key": "room-replay-active-create"
        }),
    )
    .await;
    assert_eq!(replayed.status, StatusCode::OK);
    assert_eq!(replayed.body["room_progression_id"], room_progression_id);
}

#[tokio::test]
async fn participant_triggered_transitions_require_active_account() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-actor-active-a", "room-actor-active-a").await;
    let counterparty = sign_in(&app, "pi-user-room-actor-active-b", "room-actor-active-b").await;
    let client = test_db_client().await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    client
        .execute(
            "
            UPDATE core.accounts
            SET account_state = 'suspended',
                updated_at = CURRENT_TIMESTAMP
            WHERE account_id::text = $1
            ",
            &[&subject.account_id],
        )
        .await
        .expect("account state must update");

    let response = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "mutual_intent_acknowledged",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-actor-active-coordinate",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-actor-active-coordinate"
        }),
    )
    .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn room_projection_rebuild_is_idempotent() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-rebuild-a", "room-rebuild-a").await;
    let counterparty = sign_in(&app, "pi-user-room-rebuild-b", "room-rebuild-b").await;
    let room_progression_id =
        create_room(&app, &subject.account_id, &counterparty.account_id).await;

    let first = internal_post_json(
        &app,
        "/api/internal/projection/room-progressions/rebuild",
        json!({}),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.body["rebuilt_count"], 1);

    let first_view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(first_view.status, StatusCode::OK);
    assert!(first_view.body["source_watermark_at"].as_str().is_some());
    assert!(first_view.body["last_projected_at"].as_str().is_some());
    assert!(first_view.body["projection_lag_ms"].as_i64().unwrap_or(-1) >= 0);
    let first_generation = first_view.body["rebuild_generation"]
        .as_i64()
        .expect("rebuild_generation must be numeric");

    let second = internal_post_json(
        &app,
        "/api/internal/projection/room-progressions/rebuild",
        json!({}),
    )
    .await;
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(second.body["rebuilt_count"], 1);

    let view = get_json(
        &app,
        &format!("/api/projection/room-progression-views/{room_progression_id}"),
        Some(subject.token.as_str()),
    )
    .await;
    assert_eq!(view.status, StatusCode::OK);
    assert_eq!(view.body["source_fact_count"], 1);
    let second_generation = view.body["rebuild_generation"]
        .as_i64()
        .expect("rebuild_generation must be numeric");
    assert!(second_generation > first_generation);
}

#[tokio::test]
async fn room_progression_idempotency_uses_canonical_payload_hashes() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-user-room-replay-a", "room-replay-a").await;
    let counterparty = sign_in(&app, "pi-user-room-replay-b", "room-replay-b").await;

    let create_body = format!(
        r#"{{
            "realm_id": "realm-room-replay",
            "participant_account_ids": ["{subject_id}", "{counterparty_id}"],
            "user_facing_reason_code": "room_created",
            "source_fact_kind": "intent_room_request",
            "source_fact_id": "room-replay-source",
            "source_snapshot_json": {{
                "outer": {{ "b": 2, "a": 1 }},
                "array": [{{ "z": 3, "y": 2 }}]
            }},
            "request_idempotency_key": "room-replay-create"
        }}"#,
        subject_id = subject.account_id,
        counterparty_id = counterparty.account_id
    );
    let created =
        internal_post_raw_json(&app, "/api/internal/room-progressions", &create_body).await;
    assert_eq!(created.status, StatusCode::OK);
    let room_progression_id = created.body["room_progression_id"]
        .as_str()
        .expect("room progression id must exist")
        .to_owned();

    let replay_body = format!(
        r#"{{
            "request_idempotency_key": "room-replay-create",
            "source_snapshot_json": {{
                "array": [{{ "y": 2, "z": 3 }}],
                "outer": {{ "a": 1, "b": 2 }}
            }},
            "source_fact_id": "room-replay-source",
            "source_fact_kind": "intent_room_request",
            "user_facing_reason_code": "room_created",
            "participant_account_ids": ["{counterparty_id}", "{subject_id}"],
            "realm_id": "realm-room-replay"
        }}"#,
        subject_id = subject.account_id,
        counterparty_id = counterparty.account_id
    );
    let replayed =
        internal_post_raw_json(&app, "/api/internal/room-progressions", &replay_body).await;
    assert_eq!(replayed.status, StatusCode::OK);
    assert_eq!(replayed.body["room_progression_id"], room_progression_id);

    let omitted_snapshot_create_body = format!(
        r#"{{
            "realm_id": "realm-room-replay-missing",
            "participant_account_ids": ["{subject_id}", "{counterparty_id}"],
            "user_facing_reason_code": "room_created",
            "source_fact_kind": "intent_room_request",
            "source_fact_id": "room-replay-missing-source",
            "request_idempotency_key": "room-replay-missing-create"
        }}"#,
        subject_id = subject.account_id,
        counterparty_id = counterparty.account_id
    );
    let omitted_created = internal_post_raw_json(
        &app,
        "/api/internal/room-progressions",
        &omitted_snapshot_create_body,
    )
    .await;
    assert_eq!(omitted_created.status, StatusCode::OK);
    let omitted_room_progression_id = omitted_created.body["room_progression_id"]
        .as_str()
        .expect("room progression id must exist")
        .to_owned();

    let explicit_empty_create = internal_post_json(
        &app,
        "/api/internal/room-progressions",
        json!({
            "realm_id": "realm-room-replay-missing",
            "participant_account_ids": [
                subject.account_id,
                counterparty.account_id
            ],
            "user_facing_reason_code": "room_created",
            "source_fact_kind": "intent_room_request",
            "source_fact_id": "room-replay-missing-source",
            "source_snapshot_json": {},
            "request_idempotency_key": "room-replay-missing-create"
        }),
    )
    .await;
    assert_eq!(explicit_empty_create.status, StatusCode::OK);
    assert_eq!(
        explicit_empty_create.body["room_progression_id"],
        omitted_room_progression_id
    );

    let mismatched = internal_post_json(
        &app,
        "/api/internal/room-progressions",
        json!({
            "realm_id": "realm-room-replay",
            "participant_account_ids": [
                subject.account_id,
                counterparty.account_id
            ],
            "user_facing_reason_code": "policy_review",
            "source_fact_kind": "intent_room_request",
            "source_fact_id": "room-replay-source",
            "source_snapshot_json": {},
            "request_idempotency_key": "room-replay-create"
        }),
    )
    .await;
    assert_eq!(mismatched.status, StatusCode::BAD_REQUEST);

    let fact = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "mutual_intent_acknowledged",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-replay-coordinate",
            "source_snapshot_json": {
                "outer": { "b": 2, "a": 1 }
            },
            "fact_idempotency_key": "room-replay-coordinate"
        }),
    )
    .await;
    assert_eq!(fact.status, StatusCode::OK);
    let fact_id = fact.body["room_progression_fact_id"]
        .as_str()
        .expect("fact id must exist")
        .to_owned();

    let replay_fact = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "mutual_intent_acknowledged",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-replay-coordinate",
            "source_snapshot_json": {
                "outer": { "a": 1, "b": 2 }
            },
            "fact_idempotency_key": "room-replay-coordinate"
        }),
    )
    .await;
    assert_eq!(replay_fact.status, StatusCode::OK);
    assert_eq!(replay_fact.body["room_progression_fact_id"], fact_id);

    let omitted_snapshot_fact_body = format!(
        r#"{{
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "mutual_intent_acknowledged",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": "{subject_id}",
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-replay-coordinate-missing",
            "fact_idempotency_key": "room-replay-coordinate-missing"
        }}"#,
        subject_id = subject.account_id
    );
    let omitted_fact = internal_post_raw_json(
        &app,
        &format!("/api/internal/room-progressions/{omitted_room_progression_id}/facts"),
        &omitted_snapshot_fact_body,
    )
    .await;
    assert_eq!(omitted_fact.status, StatusCode::OK);
    let omitted_fact_id = omitted_fact.body["room_progression_fact_id"]
        .as_str()
        .expect("fact id must exist")
        .to_owned();

    let explicit_empty_fact = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{omitted_room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "mutual_intent_acknowledged",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-replay-coordinate-missing",
            "source_snapshot_json": {},
            "fact_idempotency_key": "room-replay-coordinate-missing"
        }),
    )
    .await;
    assert_eq!(explicit_empty_fact.status, StatusCode::OK);
    assert_eq!(
        explicit_empty_fact.body["room_progression_fact_id"],
        omitted_fact_id
    );

    let mismatched_fact = internal_post_json(
        &app,
        &format!("/api/internal/room-progressions/{room_progression_id}/facts"),
        json!({
            "transition_kind": "advance_to_coordination",
            "to_stage": "coordination",
            "user_facing_reason_code": "policy_review",
            "triggered_by_kind": "participant",
            "triggered_by_account_id": subject.account_id,
            "source_fact_kind": "mutual_intent_acknowledgment",
            "source_fact_id": "room-replay-coordinate",
            "source_snapshot_json": {
                "outer": { "a": 1, "b": 999 }
            },
            "fact_idempotency_key": "room-replay-coordinate"
        }),
    )
    .await;
    assert_eq!(mismatched_fact.status, StatusCode::BAD_REQUEST);
}

struct SignedInUser {
    token: String,
    account_id: String,
}

struct JsonResponse {
    status: StatusCode,
    body: Value,
}

async fn create_room(app: &Router, participant_a: &str, participant_b: &str) -> String {
    let response = internal_post_json(
        app,
        "/api/internal/room-progressions",
        json!({
            "realm_id": "realm-room-default",
            "participant_account_ids": [participant_a, participant_b],
            "user_facing_reason_code": "room_created",
            "source_fact_kind": "intent_room_request",
            "source_fact_id": format!("room-source-{}", Uuid::new_v4()),
            "source_snapshot_json": {},
            "request_idempotency_key": format!("room-create-{}", Uuid::new_v4())
        }),
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    response.body["room_progression_id"]
        .as_str()
        .expect("room progression id must exist")
        .to_owned()
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
            VALUES ($1, $2, $3, 'room progression test role')
            ",
            &[&Uuid::new_v4(), &account_id, &role],
        )
        .await
        .expect("operator role assignment must insert");
    account_id.to_string()
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

async fn operator_post_json(
    app: &Router,
    path: &str,
    operator_id: &str,
    body: Value,
) -> JsonResponse {
    request_json(app, "POST", path, None, Some(operator_id), Some(body)).await
}

async fn internal_post_json(app: &Router, path: &str, body: Value) -> JsonResponse {
    request_json(app, "POST", path, None, None, Some(body)).await
}

async fn internal_post_raw_json(app: &Router, path: &str, body: &str) -> JsonResponse {
    request_raw_json(app, "POST", path, None, None, Some(body)).await
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
