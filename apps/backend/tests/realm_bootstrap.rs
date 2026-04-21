use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use chrono::{Duration, Utc};
use musubi_backend::{build_app, new_state_from_config, new_test_state};
use musubi_db_runtime::DbConfig;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn realm_request_can_be_approved_into_limited_bootstrap_and_participant_summary_is_redacted()
{
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-request-a", "realm-request-a").await;
    let sponsor = sign_in(&app, "pi-user-realm-request-b", "realm-request-b").await;
    let steward = sign_in(&app, "pi-user-realm-request-c", "realm-request-c").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let request = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": "東京コーヒー散歩",
            "slug_candidate": "tokyo-coffee-walks",
            "purpose_text": "落ち着いて会える小さな集まりを始めます。",
            "venue_context_json": {
                "city": "Tokyo",
                "venue_type": "cafe"
            },
            "expected_member_shape_json": {
                "pace": "slow",
                "size": "small"
            },
            "bootstrap_rationale_text": "立ち上がりだけ段階的に進めます。",
            "proposed_sponsor_account_id": sponsor.account_id,
            "proposed_steward_account_id": steward.account_id,
            "request_idempotency_key": "realm-request-approve-001"
        }),
    )
    .await;
    assert_eq!(request.status, StatusCode::OK);
    assert_eq!(request.body["request_state"], "requested");
    assert!(request.body.get("reviewed_by_operator_id").is_none());
    let realm_request_id = request.body["realm_request_id"]
        .as_str()
        .expect("realm request id must exist")
        .to_owned();

    let approval = operator_post_json(
        &app,
        &format!("/api/internal/operator/realms/requests/{realm_request_id}/approve"),
        &approver_id,
        json!({
            "target_realm_status": "limited_bootstrap",
            "approved_slug": "tokyo-coffee-walks",
            "approved_display_name": "東京コーヒー散歩",
            "review_reason_code": "limited_bootstrap_active",
            "steward_account_id": steward.account_id,
            "sponsor_quota_total": 2,
            "corridor_starts_at": (Utc::now() - Duration::minutes(5)).to_rfc3339(),
            "corridor_ends_at": (Utc::now() + Duration::days(7)).to_rfc3339(),
            "corridor_member_cap": 5,
            "corridor_sponsor_cap": 2,
            "review_threshold_json": {
                "quota_review_threshold": 2
            },
            "review_decision_idempotency_key": "realm-request-approve-001"
        }),
    )
    .await;
    assert_eq!(approval.status, StatusCode::OK);
    assert_eq!(approval.body["realm_status"], "limited_bootstrap");
    let realm_id = approval.body["realm_id"]
        .as_str()
        .expect("realm id must exist")
        .to_owned();

    let requester_view = get_json(
        &app,
        &format!("/api/realms/requests/{realm_request_id}"),
        Some(requester.token.as_str()),
    )
    .await;
    assert_eq!(requester_view.status, StatusCode::OK);
    assert_eq!(requester_view.body["request_state"], "approved");
    assert_eq!(requester_view.body["created_realm_id"], realm_id);
    assert!(requester_view.body.get("reviewed_by_operator_id").is_none());

    let summary = get_json(
        &app,
        &format!("/api/projection/realms/{realm_id}/bootstrap-summary"),
        Some(requester.token.as_str()),
    )
    .await;
    assert_eq!(summary.status, StatusCode::OK);
    assert_eq!(
        summary.body["bootstrap_view"]["realm_status"],
        "limited_bootstrap"
    );
    assert_eq!(
        summary.body["bootstrap_view"]["admission_posture"],
        "limited"
    );
    assert_eq!(
        summary.body["bootstrap_view"]["sponsor_display_state"],
        "sponsor_and_steward"
    );
    assert!(
        summary.body["realm_request"]
            .get("reviewed_by_operator_id")
            .is_none()
    );
    assert!(
        summary.body["bootstrap_view"]
            .get("source_fact_count")
            .is_none()
    );
    assert!(
        summary.body["bootstrap_view"]
            .get("projection_lag_ms")
            .is_none()
    );
    assert!(
        summary.body["bootstrap_view"]
            .get("rebuild_generation")
            .is_none()
    );
}

#[tokio::test]
async fn duplicate_venue_request_enters_pending_review_and_rejected_request_creates_no_realm() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester_a = sign_in(&app, "pi-user-realm-duplicate-a", "realm-duplicate-a").await;
    let requester_b = sign_in(&app, "pi-user-realm-duplicate-b", "realm-duplicate-b").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let venue_context = json!({
        "city": "Kyoto",
        "venue_type": "tea_house"
    });
    let first = post_json(
        &app,
        "/api/realms/requests",
        Some(requester_a.token.as_str()),
        json!({
            "display_name": "京都茶会",
            "slug_candidate": "kyoto-tea-circle",
            "purpose_text": "静かな対話を大切にします。",
            "venue_context_json": venue_context,
            "expected_member_shape_json": { "size": "small" },
            "bootstrap_rationale_text": "まずは少人数で始めます。",
            "request_idempotency_key": "realm-duplicate-first"
        }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);

    let second = post_json(
        &app,
        "/api/realms/requests",
        Some(requester_b.token.as_str()),
        json!({
            "display_name": "京都茶会 別口",
            "slug_candidate": "kyoto-tea-circle-b",
            "purpose_text": "同じ文脈の別申請です。",
            "venue_context_json": {
                "city": "Kyoto",
                "venue_type": "tea_house"
            },
            "expected_member_shape_json": { "size": "small" },
            "bootstrap_rationale_text": "確認が必要なケースです。",
            "request_idempotency_key": "realm-duplicate-second"
        }),
    )
    .await;
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(second.body["request_state"], "pending_review");
    assert_eq!(second.body["review_reason_code"], "review_required");
    let second_request_id = second.body["realm_request_id"]
        .as_str()
        .expect("second request id must exist")
        .to_owned();

    let rejected = operator_post_json(
        &app,
        &format!("/api/internal/operator/realms/requests/{second_request_id}/reject"),
        &approver_id,
        json!({
            "review_reason_code": "duplicate_or_invalid",
            "review_decision_idempotency_key": "realm-duplicate-reject"
        }),
    )
    .await;
    assert_eq!(rejected.status, StatusCode::OK);
    assert_eq!(rejected.body["request_state"], "rejected");
    assert_eq!(rejected.body["review_reason_code"], "duplicate_or_invalid");
    assert_eq!(rejected.body["reviewed_by_operator_id"], approver_id);
    assert!(rejected.body["created_realm_id"].is_null());

    let participant_request = get_json(
        &app,
        &format!("/api/realms/requests/{second_request_id}"),
        Some(requester_b.token.as_str()),
    )
    .await;
    assert_eq!(participant_request.status, StatusCode::OK);
    assert_eq!(participant_request.body["request_state"], "rejected");
    assert!(
        participant_request
            .body
            .get("reviewed_by_operator_id")
            .is_none()
    );
    assert!(participant_request.body["created_realm_id"].is_null());
}

#[tokio::test]
async fn approval_with_changed_slug_releases_original_slug_candidate() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester_a = sign_in(
        &app,
        "pi-user-realm-approved-slug-a",
        "realm-approved-slug-a",
    )
    .await;
    let requester_b = sign_in(
        &app,
        "pi-user-realm-approved-slug-b",
        "realm-approved-slug-b",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let first_request = post_json(
        &app,
        "/api/realms/requests",
        Some(requester_a.token.as_str()),
        json!({
            "display_name": "Original slug realm",
            "slug_candidate": "original-slug-candidate",
            "purpose_text": "The operator will approve a different slug.",
            "venue_context_json": {
                "city": "Tokyo",
                "venue_type": "gallery"
            },
            "expected_member_shape_json": {
                "pace": "slow"
            },
            "bootstrap_rationale_text": "Slug release regression coverage.",
            "request_idempotency_key": "realm-approved-slug-first"
        }),
    )
    .await;
    assert_eq!(first_request.status, StatusCode::OK);
    let realm_request_id = first_request.body["realm_request_id"]
        .as_str()
        .expect("realm request id must exist")
        .to_owned();

    let approval = operator_post_json(
        &app,
        &format!("/api/internal/operator/realms/requests/{realm_request_id}/approve"),
        &approver_id,
        json!({
            "target_realm_status": "active",
            "approved_slug": "approved-slug-candidate",
            "approved_display_name": "Approved display realm",
            "review_reason_code": "active_after_review",
            "review_decision_idempotency_key": "realm-approved-slug-approve"
        }),
    )
    .await;
    assert_eq!(approval.status, StatusCode::OK);
    assert_eq!(approval.body["slug"], "approved-slug-candidate");
    assert_eq!(approval.body["display_name"], "Approved display realm");

    let requester_view = get_json(
        &app,
        &format!("/api/realms/requests/{realm_request_id}"),
        Some(requester_a.token.as_str()),
    )
    .await;
    assert_eq!(requester_view.status, StatusCode::OK);
    assert_eq!(
        requester_view.body["slug_candidate"],
        "approved-slug-candidate"
    );
    assert_eq!(
        requester_view.body["display_name"],
        "Approved display realm"
    );

    let second_request = post_json(
        &app,
        "/api/realms/requests",
        Some(requester_b.token.as_str()),
        json!({
            "display_name": "Reused original slug realm",
            "slug_candidate": "original-slug-candidate",
            "purpose_text": "The original candidate should be available again.",
            "venue_context_json": {
                "city": "Osaka",
                "venue_type": "bookstore"
            },
            "expected_member_shape_json": {
                "pace": "quiet"
            },
            "bootstrap_rationale_text": "The first request no longer reserves this slug.",
            "request_idempotency_key": "realm-approved-slug-second"
        }),
    )
    .await;
    assert_eq!(second_request.status, StatusCode::OK);
}

#[tokio::test]
async fn concurrent_realm_request_replay_returns_existing_request() {
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
    let requester = sign_in(&app, "pi-user-realm-request-race", "realm-request-race").await;
    let client = test_db_client().await;

    let request_body = json!({
        "display_name": "Concurrent request realm",
        "slug_candidate": "concurrent-request-realm",
        "purpose_text": "Concurrent duplicate delivery should replay.",
        "venue_context_json": {
            "city": "Tokyo",
            "venue_type": "library"
        },
        "expected_member_shape_json": {
            "pace": "quiet"
        },
        "bootstrap_rationale_text": "Idempotency race regression coverage.",
        "request_idempotency_key": "realm-request-race"
    });
    let (first, second) = tokio::join!(
        post_json(
            &app,
            "/api/realms/requests",
            Some(requester.token.as_str()),
            request_body.clone()
        ),
        post_json(
            &second_app,
            "/api/realms/requests",
            Some(requester.token.as_str()),
            request_body
        )
    );
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(
        first.body["realm_request_id"],
        second.body["realm_request_id"]
    );

    let request_count: i64 = client
        .query_one(
            "
            SELECT COUNT(*) AS request_count
            FROM dao.realm_requests
            WHERE requested_by_account_id::text = $1
              AND request_idempotency_key = 'realm-request-race'
            ",
            &[&requester.account_id],
        )
        .await
        .expect("realm request count must query")
        .get("request_count");
    assert_eq!(request_count, 1);
}

#[tokio::test]
async fn suspicious_requests_enter_review_even_when_trigger_is_already_open() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester_a = sign_in(&app, "pi-user-realm-trigger-a", "realm-trigger-a").await;
    let requester_b = sign_in(&app, "pi-user-realm-trigger-b", "realm-trigger-b").await;
    let requester_c = sign_in(&app, "pi-user-realm-trigger-c", "realm-trigger-c").await;
    let rejected_requester = sign_in(&app, "pi-user-realm-trigger-d", "realm-trigger-d").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let shared_venue = json!({
        "city": "Nara",
        "venue_type": "gallery"
    });
    let first_duplicate = post_json(
        &app,
        "/api/realms/requests",
        Some(requester_a.token.as_str()),
        json!({
            "display_name": "奈良ギャラリー散歩",
            "slug_candidate": "nara-gallery-walks-a",
            "purpose_text": "静かな展示を一緒に見ます。",
            "venue_context_json": shared_venue,
            "expected_member_shape_json": { "size": "small" },
            "bootstrap_rationale_text": "少人数から始めます。",
            "request_idempotency_key": "realm-trigger-duplicate-a"
        }),
    )
    .await;
    assert_eq!(first_duplicate.status, StatusCode::OK);

    let second_duplicate = post_json(
        &app,
        "/api/realms/requests",
        Some(requester_b.token.as_str()),
        json!({
            "display_name": "奈良ギャラリー散歩 別口",
            "slug_candidate": "nara-gallery-walks-b",
            "purpose_text": "同じ文脈の申請です。",
            "venue_context_json": {
                "city": "Nara",
                "venue_type": "gallery"
            },
            "expected_member_shape_json": { "size": "small" },
            "bootstrap_rationale_text": "確認が必要です。",
            "request_idempotency_key": "realm-trigger-duplicate-b"
        }),
    )
    .await;
    assert_eq!(second_duplicate.status, StatusCode::OK);
    assert_eq!(second_duplicate.body["request_state"], "pending_review");

    let third_duplicate = post_json(
        &app,
        "/api/realms/requests",
        Some(requester_c.token.as_str()),
        json!({
            "display_name": "奈良ギャラリー散歩 追加",
            "slug_candidate": "nara-gallery-walks-c",
            "purpose_text": "同じ venue context の追加申請です。",
            "venue_context_json": {
                "city": "Nara",
                "venue_type": "gallery"
            },
            "expected_member_shape_json": { "size": "small" },
            "bootstrap_rationale_text": "既存 trigger があっても review が必要です。",
            "request_idempotency_key": "realm-trigger-duplicate-c"
        }),
    )
    .await;
    assert_eq!(third_duplicate.status, StatusCode::OK);
    assert_eq!(third_duplicate.body["request_state"], "pending_review");

    for index in 0..2 {
        let request = post_json(
            &app,
            "/api/realms/requests",
            Some(rejected_requester.token.as_str()),
            json!({
                "display_name": format!("Repeated rejected {index}"),
                "slug_candidate": format!("repeated-rejected-{index}"),
                "purpose_text": "反復 rejected path の確認です。",
                "venue_context_json": {
                    "city": "Nagoya",
                    "sequence": index
                },
                "expected_member_shape_json": { "size": "small" },
                "bootstrap_rationale_text": "審査の結果 rejected にします。",
                "request_idempotency_key": format!("realm-trigger-rejected-{index}")
            }),
        )
        .await;
        assert_eq!(request.status, StatusCode::OK);
        let request_id = request.body["realm_request_id"]
            .as_str()
            .expect("realm request id must exist");
        let rejected = operator_post_json(
            &app,
            &format!("/api/internal/operator/realms/requests/{request_id}/reject"),
            &approver_id,
            json!({
                "review_reason_code": "duplicate_or_invalid",
                "review_decision_idempotency_key": format!("realm-trigger-reject-{index}")
            }),
        )
        .await;
        assert_eq!(rejected.status, StatusCode::OK);
    }

    let repeated_request = post_json(
        &app,
        "/api/realms/requests",
        Some(rejected_requester.token.as_str()),
        json!({
            "display_name": "Repeated requester third",
            "slug_candidate": "repeated-rejected-third",
            "purpose_text": "既存 repeated trigger があっても review が必要です。",
            "venue_context_json": {
                "city": "Nagoya",
                "sequence": 3
            },
            "expected_member_shape_json": { "size": "small" },
            "bootstrap_rationale_text": "反復 rejected requester の追加申請です。",
            "request_idempotency_key": "realm-trigger-rejected-third"
        }),
    )
    .await;
    assert_eq!(repeated_request.status, StatusCode::OK);
    assert_eq!(repeated_request.body["request_state"], "pending_review");
}

#[tokio::test]
async fn realm_review_decision_reason_codes_are_path_specific() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-review-reason-a",
        "realm-review-reason-a",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let request = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": "Review reason realm",
            "slug_candidate": "review-reason-realm",
            "purpose_text": "Decision reason validation.",
            "venue_context_json": {
                "city": "Tokyo",
                "venue_type": "gallery"
            },
            "expected_member_shape_json": {
                "pace": "slow"
            },
            "bootstrap_rationale_text": "Keep decision paths semantically narrow.",
            "request_idempotency_key": "realm-review-reason-request"
        }),
    )
    .await;
    assert_eq!(request.status, StatusCode::OK);
    let realm_request_id = request.body["realm_request_id"]
        .as_str()
        .expect("realm request id must exist")
        .to_owned();

    let mismatched_approval_reason = operator_post_json(
        &app,
        &format!("/api/internal/operator/realms/requests/{realm_request_id}/approve"),
        &approver_id,
        json!({
            "target_realm_status": "active",
            "review_reason_code": "limited_bootstrap_active",
            "review_decision_idempotency_key": "realm-review-reason-bad-approve"
        }),
    )
    .await;
    assert_eq!(mismatched_approval_reason.status, StatusCode::BAD_REQUEST);

    let rejected_with_activation_reason = operator_post_json(
        &app,
        &format!("/api/internal/operator/realms/requests/{realm_request_id}/reject"),
        &approver_id,
        json!({
            "review_reason_code": "active_after_review",
            "review_decision_idempotency_key": "realm-review-reason-bad-reject"
        }),
    )
    .await;
    assert_eq!(
        rejected_with_activation_reason.status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn realm_request_requires_venue_context_and_expected_member_shape() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-required-a", "realm-required-a").await;

    let missing_venue = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": "文脈なし申請",
            "slug_candidate": "missing-venue-context",
            "purpose_text": "文脈がない申請です。",
            "expected_member_shape_json": { "size": "small" },
            "bootstrap_rationale_text": "確認します。",
            "request_idempotency_key": "missing-venue-context"
        }),
    )
    .await;
    assert!(missing_venue.status.is_client_error());

    let empty_shape = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": "形なし申請",
            "slug_candidate": "empty-member-shape",
            "purpose_text": "member shape が空の申請です。",
            "venue_context_json": {
                "city": "Tokyo"
            },
            "expected_member_shape_json": {},
            "bootstrap_rationale_text": "確認します。",
            "request_idempotency_key": "empty-member-shape"
        }),
    )
    .await;
    assert_eq!(empty_shape.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn active_realm_uses_normal_admission_kind_and_open_posture() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-active-a", "realm-active-a").await;
    let member = sign_in(&app, "pi-user-realm-active-b", "realm-active-b").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        None,
        None,
        &approver_id,
        "active",
        "realm-active-001",
    )
    .await;

    let summary = get_json(
        &app,
        &format!("/api/projection/realms/{realm_id}/bootstrap-summary"),
        Some(requester.token.as_str()),
    )
    .await;
    assert_eq!(summary.status, StatusCode::OK);
    assert_eq!(summary.body["bootstrap_view"]["realm_status"], "active");
    assert_eq!(summary.body["bootstrap_view"]["admission_posture"], "open");

    let admission = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": member.account_id,
            "source_fact_kind": "realm_admin_invite",
            "source_fact_id": "realm-active-admission",
            "source_snapshot_json": {
                "private_note": "must stay internal"
            },
            "request_idempotency_key": "realm-active-admission"
        }),
    )
    .await;
    assert_eq!(admission.status, StatusCode::OK);
    assert_eq!(admission.body["admission_kind"], "normal");
    assert_eq!(admission.body["admission_status"], "admitted");
    assert_eq!(admission.body["review_reason_code"], "active_after_review");

    let member_summary = get_json(
        &app,
        &format!("/api/projection/realms/{realm_id}/bootstrap-summary"),
        Some(member.token.as_str()),
    )
    .await;
    assert_eq!(member_summary.status, StatusCode::OK);
    assert_eq!(
        member_summary.body["admission_view"]["admission_kind"],
        "normal"
    );
    assert_eq!(
        member_summary.body["admission_view"]["admission_status"],
        "admitted"
    );
    assert!(
        member_summary
            .body
            .to_string()
            .contains("active_after_review")
    );
    assert!(!member_summary.body.to_string().contains("private_note"));
}

#[tokio::test]
async fn approval_rejects_already_expired_corridor() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-expired-approval-a",
        "realm-expired-approval-a",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let request = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": "期限切れ corridor",
            "slug_candidate": "expired-corridor-approval",
            "purpose_text": "期限切れ corridor は作れません。",
            "venue_context_json": {
                "city": "Tokyo",
                "venue_type": "community_space"
            },
            "expected_member_shape_json": {
                "size": "small"
            },
            "bootstrap_rationale_text": "期限の検証です。",
            "request_idempotency_key": "expired-corridor-approval-request"
        }),
    )
    .await;
    assert_eq!(request.status, StatusCode::OK);
    let realm_request_id = request.body["realm_request_id"]
        .as_str()
        .expect("realm request id must exist");

    let approval = operator_post_json(
        &app,
        &format!("/api/internal/operator/realms/requests/{realm_request_id}/approve"),
        &approver_id,
        json!({
            "target_realm_status": "limited_bootstrap",
            "review_reason_code": "limited_bootstrap_active",
            "corridor_starts_at": (Utc::now() - Duration::days(2)).to_rfc3339(),
            "corridor_ends_at": (Utc::now() - Duration::hours(1)).to_rfc3339(),
            "corridor_member_cap": 2,
            "corridor_sponsor_cap": 1,
            "review_threshold_json": {},
            "review_decision_idempotency_key": "expired-corridor-approval"
        }),
    )
    .await;
    assert_eq!(approval.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn approval_rejects_non_positive_sponsor_quota_total() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-sponsor-quota-approval-a",
        "realm-sponsor-quota-approval-a",
    )
    .await;
    let sponsor = sign_in(
        &app,
        "pi-user-realm-sponsor-quota-approval-b",
        "realm-sponsor-quota-approval-b",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let request = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": "Sponsor quota approval",
            "slug_candidate": "sponsor-quota-approval",
            "purpose_text": "Sponsor quota must be validated before insert.",
            "venue_context_json": {
                "city": "Tokyo",
                "venue_type": "community_space"
            },
            "expected_member_shape_json": {
                "size": "small"
            },
            "bootstrap_rationale_text": "Non-positive quota should be rejected.",
            "proposed_sponsor_account_id": sponsor.account_id,
            "request_idempotency_key": "realm-sponsor-quota-approval-request"
        }),
    )
    .await;
    assert_eq!(request.status, StatusCode::OK);
    let realm_request_id = request.body["realm_request_id"]
        .as_str()
        .expect("realm request id must exist");

    let approval = operator_post_json(
        &app,
        &format!("/api/internal/operator/realms/requests/{realm_request_id}/approve"),
        &approver_id,
        json!({
            "target_realm_status": "active",
            "review_reason_code": "active_after_review",
            "sponsor_quota_total": 0,
            "review_decision_idempotency_key": "realm-sponsor-quota-approval"
        }),
    )
    .await;
    assert_eq!(approval.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sponsor_backed_admission_respects_quota_and_review_summary_is_redacted() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-quota-a", "realm-quota-a").await;
    let sponsor = sign_in(&app, "pi-user-realm-quota-b", "realm-quota-b").await;
    let first_member = sign_in(&app, "pi-user-realm-quota-c", "realm-quota-c").await;
    let second_member = sign_in(&app, "pi-user-realm-quota-d", "realm-quota-d").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        Some(&sponsor),
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-quota-001",
    )
    .await;
    let sponsor_record_id = sponsor_record_id_for_realm(&client, &realm_id).await;

    let review_case = operator_post_json(
        &app,
        "/api/internal/operator/review-cases",
        &approver_id,
        json!({
            "case_type": "realm_admission_review",
            "severity": "sev2",
            "subject_account_id": requester.account_id,
            "related_realm_id": realm_id,
            "opened_reason_code": "policy_review",
            "source_fact_kind": "realm_request",
            "source_fact_id": "realm-quota-review",
            "source_snapshot_json": {
                "safe_summary": "bootstrap health review"
            },
            "request_idempotency_key": "realm-quota-review"
        }),
    )
    .await;
    assert_eq!(review_case.status, StatusCode::OK);

    let first = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": first_member.account_id,
            "sponsor_record_id": sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-quota-first",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-quota-first"
        }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.body["admission_kind"], "sponsor_backed");
    assert_eq!(first.body["admission_status"], "admitted");

    let second = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": second_member.account_id,
            "sponsor_record_id": sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-quota-second",
            "source_snapshot_json": {
                "internal_trigger_context": "must not leak"
            },
            "request_idempotency_key": "realm-quota-second"
        }),
    )
    .await;
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(second.body["admission_kind"], "review_required");
    assert_eq!(second.body["admission_status"], "pending");
    assert_eq!(
        second.body["review_reason_code"],
        "bootstrap_capacity_reached"
    );

    let pending_member_summary = get_json(
        &app,
        &format!("/api/projection/realms/{realm_id}/bootstrap-summary"),
        Some(second_member.token.as_str()),
    )
    .await;
    assert_eq!(pending_member_summary.status, StatusCode::NOT_FOUND);

    let review_summary = operator_get_json(
        &app,
        &format!("/api/internal/operator/realms/{realm_id}/review-summary"),
        &approver_id,
    )
    .await;
    assert_eq!(review_summary.status, StatusCode::OK);
    assert_eq!(review_summary.body["open_review_case_count"], 1);
    assert!(
        review_summary.body["open_review_trigger_count"]
            .as_i64()
            .expect("trigger count must be numeric")
            >= 1
    );
    assert_eq!(
        review_summary.body["latest_redacted_reason_code"],
        "bootstrap_capacity_reached"
    );
    assert!(
        review_summary.body["open_review_triggers"][0]
            .get("context_json")
            .is_none()
    );
    assert!(
        !review_summary
            .body
            .to_string()
            .contains("internal_trigger_context")
    );
}

#[tokio::test]
async fn sponsor_backed_admission_respects_corridor_member_cap() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-member-cap-a", "realm-member-cap-a").await;
    let sponsor = sign_in(&app, "pi-user-realm-member-cap-b", "realm-member-cap-b").await;
    let first_member = sign_in(&app, "pi-user-realm-member-cap-c", "realm-member-cap-c").await;
    let second_member = sign_in(&app, "pi-user-realm-member-cap-d", "realm-member-cap-d").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let request = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": "Corridor member cap realm",
            "slug_candidate": "corridor-member-cap-realm",
            "purpose_text": "member cap を writer path で守ります。",
            "venue_context_json": {
                "city": "Tokyo",
                "venue_type": "library"
            },
            "expected_member_shape_json": {
                "size": "tiny"
            },
            "bootstrap_rationale_text": "1人だけ corridor admit します。",
            "proposed_sponsor_account_id": sponsor.account_id,
            "request_idempotency_key": "realm-member-cap-request"
        }),
    )
    .await;
    assert_eq!(request.status, StatusCode::OK);
    let realm_request_id = request.body["realm_request_id"]
        .as_str()
        .expect("realm request id must exist");

    let approval = operator_post_json(
        &app,
        &format!("/api/internal/operator/realms/requests/{realm_request_id}/approve"),
        &approver_id,
        json!({
            "target_realm_status": "limited_bootstrap",
            "review_reason_code": "limited_bootstrap_active",
            "sponsor_quota_total": 2,
            "corridor_starts_at": (Utc::now() - Duration::minutes(5)).to_rfc3339(),
            "corridor_ends_at": (Utc::now() + Duration::days(3)).to_rfc3339(),
            "corridor_member_cap": 1,
            "corridor_sponsor_cap": 1,
            "review_threshold_json": {},
            "review_decision_idempotency_key": "realm-member-cap-approve"
        }),
    )
    .await;
    assert_eq!(approval.status, StatusCode::OK);
    let realm_id = approval.body["realm_id"]
        .as_str()
        .expect("realm id must exist")
        .to_owned();
    let sponsor_record_id = sponsor_record_id_for_realm(&client, &realm_id).await;

    let first = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": first_member.account_id,
            "sponsor_record_id": sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-member-cap-first",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-member-cap-first"
        }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.body["admission_kind"], "sponsor_backed");
    assert_eq!(first.body["admission_status"], "admitted");

    let second = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": second_member.account_id,
            "sponsor_record_id": sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-member-cap-second",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-member-cap-second"
        }),
    )
    .await;
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(second.body["admission_kind"], "review_required");
    assert_eq!(second.body["admission_status"], "pending");
    assert_eq!(
        second.body["review_reason_code"],
        "bootstrap_capacity_reached"
    );

    let review_summary = operator_get_json(
        &app,
        &format!("/api/internal/operator/realms/{realm_id}/review-summary"),
        &approver_id,
    )
    .await;
    assert_eq!(review_summary.status, StatusCode::OK);
    assert_eq!(
        review_summary.body["latest_redacted_reason_code"],
        "bootstrap_capacity_reached"
    );
    assert_eq!(
        review_summary.body["open_review_triggers"][0]["trigger_kind"],
        "corridor_cap_pressure"
    );
}

#[tokio::test]
async fn concurrent_corridor_admissions_do_not_exceed_member_cap() {
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
    let requester = sign_in(
        &app,
        "pi-user-realm-corridor-race-requester",
        "realm-corridor-race-requester",
    )
    .await;
    let first_member = sign_in(
        &app,
        "pi-user-realm-corridor-race-first",
        "realm-corridor-race-first",
    )
    .await;
    let second_member = sign_in(
        &app,
        "pi-user-realm-corridor-race-second",
        "realm-corridor-race-second",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        None,
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-corridor-race",
    )
    .await;
    client
        .execute(
            "
            UPDATE dao.bootstrap_corridors
            SET member_cap = 1,
                updated_at = CURRENT_TIMESTAMP
            WHERE realm_id = $1
            ",
            &[&realm_id],
        )
        .await
        .expect("corridor member cap must update");

    let path = format!("/api/internal/realms/{realm_id}/admissions");
    let first_body = json!({
        "account_id": first_member.account_id,
        "source_fact_kind": "manual_review",
        "source_fact_id": "realm-corridor-race-first",
        "source_snapshot_json": {},
        "request_idempotency_key": "realm-corridor-race-first"
    });
    let second_body = json!({
        "account_id": second_member.account_id,
        "source_fact_kind": "manual_review",
        "source_fact_id": "realm-corridor-race-second",
        "source_snapshot_json": {},
        "request_idempotency_key": "realm-corridor-race-second"
    });

    let (first, second) = tokio::join!(
        operator_post_json(&app, &path, &approver_id, first_body),
        operator_post_json(&second_app, &path, &approver_id, second_body)
    );
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(second.status, StatusCode::OK);
    let responses = [&first, &second];
    assert_eq!(
        responses
            .iter()
            .filter(|response| response.body["admission_kind"] == "corridor")
            .count(),
        1
    );
    assert_eq!(
        responses
            .iter()
            .filter(|response| response.body["review_reason_code"] == "bootstrap_capacity_reached")
            .count(),
        1
    );
    let admitted_count: i64 = client
        .query_one(
            "
            SELECT COUNT(*) AS admitted_count
            FROM dao.realm_admissions
            WHERE realm_id = $1
              AND bootstrap_corridor_id IS NOT NULL
              AND admission_status = 'admitted'
            ",
            &[&realm_id],
        )
        .await
        .expect("admission count must query")
        .get("admitted_count");
    assert_eq!(admitted_count, 1);
}

#[tokio::test]
async fn realm_scoped_review_trigger_fingerprints_do_not_collapse_across_realms() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-scoped-trigger-requester",
        "realm-scoped-trigger-requester",
    )
    .await;
    let sponsor = sign_in(
        &app,
        "pi-user-realm-scoped-trigger-sponsor",
        "realm-scoped-trigger-sponsor",
    )
    .await;
    let member = sign_in(
        &app,
        "pi-user-realm-scoped-trigger-member",
        "realm-scoped-trigger-member",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let mut realm_ids = Vec::new();
    for index in 1..=4 {
        let (realm_id, _) = create_realm(
            &app,
            &requester,
            None,
            None,
            &approver_id,
            "active",
            &format!("realm-scoped-trigger-{index}"),
        )
        .await;
        let sponsor_record = operator_post_json(
            &app,
            &format!("/api/internal/realms/{realm_id}/sponsor-records"),
            &approver_id,
            json!({
                "sponsor_account_id": sponsor.account_id,
                "sponsor_status": "active",
                "quota_total": 4,
                "status_reason_code": "active_after_review",
                "request_idempotency_key": format!("realm-scoped-trigger-sponsor-{index}")
            }),
        )
        .await;
        assert_eq!(sponsor_record.status, StatusCode::OK);
        let admission = operator_post_json(
            &app,
            &format!("/api/internal/realms/{realm_id}/admissions"),
            &approver_id,
            json!({
                "account_id": member.account_id,
                "source_fact_kind": "manual_review",
                "source_fact_id": format!("realm-scoped-trigger-admission-{index}"),
                "source_snapshot_json": {},
                "request_idempotency_key": format!("realm-scoped-trigger-admission-{index}")
            }),
        )
        .await;
        assert_eq!(admission.status, StatusCode::OK);
        realm_ids.push(realm_id);
    }

    for realm_id in &realm_ids[2..] {
        let review_summary = operator_get_json(
            &app,
            &format!("/api/internal/operator/realms/{realm_id}/review-summary"),
            &approver_id,
        )
        .await;
        assert_eq!(review_summary.status, StatusCode::OK);
        assert_eq!(review_summary.body["open_review_trigger_count"], 2);
        let trigger_kinds = review_summary.body["open_review_triggers"]
            .as_array()
            .expect("open review triggers must be an array")
            .iter()
            .map(|value| {
                value["trigger_kind"]
                    .as_str()
                    .expect("trigger kind must be present")
            })
            .collect::<Vec<_>>();
        assert!(trigger_kinds.contains(&"sponsor_concentration"));
        assert!(trigger_kinds.contains(&"suspicious_member_overlap"));
    }
}

#[tokio::test]
async fn cross_realm_sponsor_record_cannot_grant_admission() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester_a = sign_in(
        &app,
        "pi-user-realm-cross-sponsor-a",
        "realm-cross-sponsor-a",
    )
    .await;
    let requester_b = sign_in(
        &app,
        "pi-user-realm-cross-sponsor-b",
        "realm-cross-sponsor-b",
    )
    .await;
    let sponsor = sign_in(
        &app,
        "pi-user-realm-cross-sponsor-c",
        "realm-cross-sponsor-c",
    )
    .await;
    let member = sign_in(
        &app,
        "pi-user-realm-cross-sponsor-d",
        "realm-cross-sponsor-d",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (realm_a_id, _) = create_realm(
        &app,
        &requester_a,
        Some(&sponsor),
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-cross-sponsor-a",
    )
    .await;
    let realm_a_sponsor_record_id = sponsor_record_id_for_realm(&client, &realm_a_id).await;
    let (realm_b_id, _) = create_realm(
        &app,
        &requester_b,
        None,
        None,
        &approver_id,
        "active",
        "realm-cross-sponsor-b",
    )
    .await;

    let admission = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_b_id}/admissions"),
        &approver_id,
        json!({
            "account_id": member.account_id,
            "sponsor_record_id": realm_a_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-cross-sponsor-admission",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-cross-sponsor-admission"
        }),
    )
    .await;
    assert_eq!(admission.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        admission_count_for_account(&client, &realm_b_id, &member.account_id).await,
        0
    );
}

#[tokio::test]
async fn rate_limited_and_revoked_sponsor_do_not_auto_admit() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-sponsor-a", "realm-sponsor-a").await;
    let sponsor = sign_in(&app, "pi-user-realm-sponsor-b", "realm-sponsor-b").await;
    let first_member = sign_in(&app, "pi-user-realm-sponsor-c", "realm-sponsor-c").await;
    let second_member = sign_in(&app, "pi-user-realm-sponsor-d", "realm-sponsor-d").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (rate_limited_realm_id, _) = create_realm(
        &app,
        &requester,
        Some(&sponsor),
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-sponsor-rate-limited",
    )
    .await;
    let rate_limited_sponsor_record_id =
        sponsor_record_id_for_realm(&client, &rate_limited_realm_id).await;
    let rate_limit_record = operator_post_json(
        &app,
        &format!("/api/internal/realms/{rate_limited_realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "rate_limited",
            "quota_total": 1,
            "status_reason_code": "sponsor_rate_limited",
            "request_idempotency_key": "realm-sponsor-rate-limited-record"
        }),
    )
    .await;
    assert_eq!(rate_limit_record.status, StatusCode::OK);
    let latest_rate_limited_sponsor_record_id = rate_limit_record.body["realm_sponsor_record_id"]
        .as_str()
        .expect("rate-limited sponsor record id must exist");

    let rate_limited = operator_post_json(
        &app,
        &format!("/api/internal/realms/{rate_limited_realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": first_member.account_id,
            "sponsor_record_id": rate_limited_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-rate-limited-admission",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-rate-limited-admission"
        }),
    )
    .await;
    assert_eq!(rate_limited.status, StatusCode::OK);
    assert_eq!(rate_limited.body["admission_kind"], "review_required");
    assert_eq!(rate_limited.body["admission_status"], "pending");
    assert_eq!(
        rate_limited.body["review_reason_code"],
        "sponsor_rate_limited"
    );
    assert_eq!(
        rate_limited.body["sponsor_record_id"],
        latest_rate_limited_sponsor_record_id
    );

    let (revoked_realm_id, _) = create_realm(
        &app,
        &requester,
        Some(&sponsor),
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-sponsor-revoked",
    )
    .await;
    let revoked_sponsor_record_id = sponsor_record_id_for_realm(&client, &revoked_realm_id).await;
    set_sponsor_status(
        &client,
        &revoked_sponsor_record_id,
        "revoked",
        "sponsor_revoked",
    )
    .await;

    let revoked = operator_post_json(
        &app,
        &format!("/api/internal/realms/{revoked_realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": second_member.account_id,
            "sponsor_record_id": revoked_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-revoked-admission",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-revoked-admission"
        }),
    )
    .await;
    assert_eq!(revoked.status, StatusCode::OK);
    assert_eq!(revoked.body["admission_kind"], "review_required");
    assert_eq!(revoked.body["admission_status"], "pending");
    assert_eq!(revoked.body["review_reason_code"], "sponsor_revoked");
}

#[tokio::test]
async fn stale_sponsor_record_id_uses_latest_sponsor_status() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-stale-sponsor-a",
        "realm-stale-sponsor-a",
    )
    .await;
    let sponsor = sign_in(
        &app,
        "pi-user-realm-stale-sponsor-b",
        "realm-stale-sponsor-b",
    )
    .await;
    let member = sign_in(
        &app,
        "pi-user-realm-stale-sponsor-c",
        "realm-stale-sponsor-c",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        Some(&sponsor),
        None,
        &approver_id,
        "active",
        "realm-stale-sponsor",
    )
    .await;
    let stale_active_sponsor_record_id = sponsor_record_id_for_realm(&client, &realm_id).await;
    let revoked_sponsor_record = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "revoked",
            "quota_total": 1,
            "status_reason_code": "sponsor_revoked",
            "request_idempotency_key": "realm-stale-sponsor-revoked"
        }),
    )
    .await;
    assert_eq!(revoked_sponsor_record.status, StatusCode::OK);
    let revoked_sponsor_record_id = revoked_sponsor_record.body["realm_sponsor_record_id"]
        .as_str()
        .expect("revoked sponsor record id must exist");

    let admission = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": member.account_id,
            "sponsor_record_id": stale_active_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-stale-sponsor-admission",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-stale-sponsor-admission"
        }),
    )
    .await;
    assert_eq!(admission.status, StatusCode::OK);
    assert_eq!(admission.body["admission_kind"], "review_required");
    assert_eq!(admission.body["admission_status"], "pending");
    assert_eq!(admission.body["review_reason_code"], "sponsor_revoked");
    assert_eq!(
        admission.body["sponsor_record_id"],
        revoked_sponsor_record_id
    );
}

#[tokio::test]
async fn expired_and_disabled_corridor_do_not_grant_corridor_benefits_even_if_projection_is_stale()
{
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-corridor-a", "realm-corridor-a").await;
    let expired_member = sign_in(&app, "pi-user-realm-corridor-b", "realm-corridor-b").await;
    let disabled_member = sign_in(&app, "pi-user-realm-corridor-c", "realm-corridor-c").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (expired_realm_id, _) = create_realm(
        &app,
        &requester,
        None,
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-corridor-expired",
    )
    .await;
    assert_eq!(
        current_projection_corridor_status(&client, &expired_realm_id).await,
        "active"
    );
    expire_corridor_without_rebuild(&client, &expired_realm_id).await;

    let expired = operator_post_json(
        &app,
        &format!("/api/internal/realms/{expired_realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": expired_member.account_id,
            "source_fact_kind": "realm_admin_invite",
            "source_fact_id": "realm-corridor-expired",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-corridor-expired"
        }),
    )
    .await;
    assert_eq!(expired.status, StatusCode::OK);
    assert_eq!(expired.body["admission_kind"], "review_required");
    assert_eq!(expired.body["admission_status"], "pending");
    assert_eq!(expired.body["review_reason_code"], "bootstrap_expired");

    let expired_summary = get_json(
        &app,
        &format!("/api/projection/realms/{expired_realm_id}/bootstrap-summary"),
        Some(requester.token.as_str()),
    )
    .await;
    assert_eq!(expired_summary.status, StatusCode::OK);
    assert_eq!(
        expired_summary.body["bootstrap_view"]["corridor_status"],
        "expired"
    );
    assert_eq!(
        expired_summary.body["bootstrap_view"]["admission_posture"],
        "review_required"
    );

    let (disabled_realm_id, _) = create_realm(
        &app,
        &requester,
        None,
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-corridor-disabled",
    )
    .await;
    assert_eq!(
        current_projection_corridor_status(&client, &disabled_realm_id).await,
        "active"
    );
    disable_corridor_without_rebuild(&client, &disabled_realm_id).await;

    let disabled = operator_post_json(
        &app,
        &format!("/api/internal/realms/{disabled_realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": disabled_member.account_id,
            "source_fact_kind": "realm_admin_invite",
            "source_fact_id": "realm-corridor-disabled",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-corridor-disabled"
        }),
    )
    .await;
    assert_eq!(disabled.status, StatusCode::OK);
    assert_eq!(disabled.body["admission_kind"], "review_required");
    assert_eq!(disabled.body["admission_status"], "pending");

    let disabled_summary = get_json(
        &app,
        &format!("/api/projection/realms/{disabled_realm_id}/bootstrap-summary"),
        Some(requester.token.as_str()),
    )
    .await;
    assert_eq!(disabled_summary.status, StatusCode::OK);
    assert_eq!(
        disabled_summary.body["bootstrap_view"]["corridor_status"],
        "disabled_by_operator"
    );
    assert_eq!(
        disabled_summary.body["bootstrap_view"]["admission_posture"],
        "review_required"
    );
}

#[tokio::test]
async fn summary_reads_refresh_expired_corridor_without_unrelated_write() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-summary-expiry-a",
        "realm-summary-expiry-a",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        None,
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-summary-expiry",
    )
    .await;
    let rebuild_generation = current_projection_rebuild_generation(&client, &realm_id).await;
    assert_eq!(
        current_projection_corridor_status(&client, &realm_id).await,
        "active"
    );
    expire_corridor_without_rebuild(&client, &realm_id).await;

    let participant_summary = get_json(
        &app,
        &format!("/api/projection/realms/{realm_id}/bootstrap-summary"),
        Some(requester.token.as_str()),
    )
    .await;
    assert_eq!(participant_summary.status, StatusCode::OK);
    assert_eq!(
        participant_summary.body["bootstrap_view"]["corridor_status"],
        "expired"
    );
    assert_eq!(
        participant_summary.body["bootstrap_view"]["admission_posture"],
        "review_required"
    );
    assert_eq!(
        current_projection_rebuild_generation(&client, &realm_id).await,
        rebuild_generation
    );

    let review_summary = operator_get_json(
        &app,
        &format!("/api/internal/operator/realms/{realm_id}/review-summary"),
        &approver_id,
    )
    .await;
    assert_eq!(review_summary.status, StatusCode::OK);
    assert_eq!(review_summary.body["corridor_status"], "expired");
    assert_eq!(review_summary.body["corridor_remaining_seconds"], 0);
}

#[tokio::test]
async fn expired_or_disabled_corridor_blocks_sponsor_backed_auto_admission() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-corridor-sponsor-a",
        "realm-corridor-sponsor-a",
    )
    .await;
    let sponsor = sign_in(
        &app,
        "pi-user-realm-corridor-sponsor-b",
        "realm-corridor-sponsor-b",
    )
    .await;
    let expired_member = sign_in(
        &app,
        "pi-user-realm-corridor-sponsor-c",
        "realm-corridor-sponsor-c",
    )
    .await;
    let disabled_member = sign_in(
        &app,
        "pi-user-realm-corridor-sponsor-d",
        "realm-corridor-sponsor-d",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (expired_realm_id, _) = create_realm(
        &app,
        &requester,
        Some(&sponsor),
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-corridor-sponsor-expired",
    )
    .await;
    let expired_sponsor_record_id = sponsor_record_id_for_realm(&client, &expired_realm_id).await;
    expire_corridor_without_rebuild(&client, &expired_realm_id).await;
    let expired = operator_post_json(
        &app,
        &format!("/api/internal/realms/{expired_realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": expired_member.account_id,
            "sponsor_record_id": expired_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-corridor-sponsor-expired",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-corridor-sponsor-expired"
        }),
    )
    .await;
    assert_eq!(expired.status, StatusCode::OK);
    assert_eq!(expired.body["admission_kind"], "review_required");
    assert_eq!(expired.body["admission_status"], "pending");
    assert_eq!(expired.body["review_reason_code"], "bootstrap_expired");

    let (disabled_realm_id, _) = create_realm(
        &app,
        &requester,
        Some(&sponsor),
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-corridor-sponsor-disabled",
    )
    .await;
    let disabled_sponsor_record_id = sponsor_record_id_for_realm(&client, &disabled_realm_id).await;
    disable_corridor_without_rebuild(&client, &disabled_realm_id).await;
    let disabled = operator_post_json(
        &app,
        &format!("/api/internal/realms/{disabled_realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": disabled_member.account_id,
            "sponsor_record_id": disabled_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-corridor-sponsor-disabled",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-corridor-sponsor-disabled"
        }),
    )
    .await;
    assert_eq!(disabled.status, StatusCode::OK);
    assert_eq!(disabled.body["admission_kind"], "review_required");
    assert_eq!(disabled.body["admission_status"], "pending");
    assert_eq!(disabled.body["review_reason_code"], "operator_restriction");
}

#[tokio::test]
async fn restricted_and_suspended_realms_block_new_admissions() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-block-a", "realm-block-a").await;
    let restricted_member = sign_in(&app, "pi-user-realm-block-b", "realm-block-b").await;
    let suspended_member = sign_in(&app, "pi-user-realm-block-c", "realm-block-c").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (restricted_realm_id, _) = create_realm(
        &app,
        &requester,
        None,
        None,
        &approver_id,
        "active",
        "realm-restricted",
    )
    .await;
    set_realm_status(
        &client,
        &restricted_realm_id,
        "restricted",
        "restricted_after_review",
    )
    .await;
    let restricted = operator_post_json(
        &app,
        &format!("/api/internal/realms/{restricted_realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": restricted_member.account_id,
            "source_fact_kind": "realm_admin_invite",
            "source_fact_id": "realm-restricted-admission",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-restricted-admission"
        }),
    )
    .await;
    assert_eq!(restricted.status, StatusCode::BAD_REQUEST);

    let (suspended_realm_id, _) = create_realm(
        &app,
        &requester,
        None,
        None,
        &approver_id,
        "active",
        "realm-suspended",
    )
    .await;
    set_realm_status(
        &client,
        &suspended_realm_id,
        "suspended",
        "suspended_after_review",
    )
    .await;
    let suspended = operator_post_json(
        &app,
        &format!("/api/internal/realms/{suspended_realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": suspended_member.account_id,
            "source_fact_kind": "realm_admin_invite",
            "source_fact_id": "realm-suspended-admission",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-suspended-admission"
        }),
    )
    .await;
    assert_eq!(suspended.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn duplicate_sponsor_record_conflict_is_bad_request() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-sponsor-dup-a", "realm-sponsor-dup-a").await;
    let sponsor = sign_in(&app, "pi-user-realm-sponsor-dup-b", "realm-sponsor-dup-b").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        Some(&sponsor),
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-sponsor-duplicate",
    )
    .await;

    let duplicate = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "active",
            "quota_total": 1,
            "status_reason_code": "limited_bootstrap_active",
            "request_idempotency_key": "realm-sponsor-duplicate-second-key"
        }),
    )
    .await;
    assert_eq!(duplicate.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn request_and_admission_idempotency_replay_mismatch_is_rejected() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-idem-a", "realm-idem-a").await;
    let member = sign_in(&app, "pi-user-realm-idem-b", "realm-idem-b").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let request_body = json!({
        "display_name": "神戸読書会",
        "slug_candidate": "kobe-reading-room",
        "purpose_text": "静かな読書と対話です。",
        "venue_context_json": {
            "city": "Kobe",
            "venue_type": "bookstore"
        },
        "expected_member_shape_json": {
            "size": "small"
        },
        "bootstrap_rationale_text": "まずは小さく始めます。",
        "request_idempotency_key": "realm-idem-request"
    });
    let first_request = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        request_body.clone(),
    )
    .await;
    assert_eq!(first_request.status, StatusCode::OK);
    let realm_request_id = first_request.body["realm_request_id"]
        .as_str()
        .expect("realm request id must exist")
        .to_owned();

    let replayed_request = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        request_body,
    )
    .await;
    assert_eq!(replayed_request.status, StatusCode::OK);
    assert_eq!(replayed_request.body["realm_request_id"], realm_request_id);

    let mismatched_request = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": "神戸読書会",
            "slug_candidate": "kobe-reading-room",
            "purpose_text": "別の目的文です。",
            "venue_context_json": {
                "city": "Kobe",
                "venue_type": "bookstore"
            },
            "expected_member_shape_json": {
                "size": "small"
            },
            "bootstrap_rationale_text": "まずは小さく始めます。",
            "request_idempotency_key": "realm-idem-request"
        }),
    )
    .await;
    assert_eq!(mismatched_request.status, StatusCode::BAD_REQUEST);

    let approval = operator_post_json(
        &app,
        &format!("/api/internal/operator/realms/requests/{realm_request_id}/approve"),
        &approver_id,
        json!({
            "target_realm_status": "active",
            "review_reason_code": "active_after_review",
            "review_decision_idempotency_key": "realm-idem-approve"
        }),
    )
    .await;
    assert_eq!(approval.status, StatusCode::OK);
    let realm_id = approval.body["realm_id"]
        .as_str()
        .expect("realm id must exist")
        .to_owned();

    let admission_body = json!({
        "account_id": member.account_id,
        "source_fact_kind": "realm_admin_invite",
        "source_fact_id": "realm-idem-admission",
        "source_snapshot_json": {
            "safe_summary": "first admission"
        },
        "request_idempotency_key": "realm-idem-admission"
    });
    let first_admission = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        admission_body.clone(),
    )
    .await;
    assert_eq!(first_admission.status, StatusCode::OK);
    let realm_admission_id = first_admission.body["realm_admission_id"]
        .as_str()
        .expect("realm admission id must exist")
        .to_owned();

    let replayed_admission = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        admission_body,
    )
    .await;
    assert_eq!(replayed_admission.status, StatusCode::OK);
    assert_eq!(
        replayed_admission.body["realm_admission_id"],
        realm_admission_id
    );

    let mismatched_admission = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": member.account_id,
            "source_fact_kind": "realm_admin_invite",
            "source_fact_id": "realm-idem-admission-drift",
            "source_snapshot_json": {
                "safe_summary": "first admission"
            },
            "request_idempotency_key": "realm-idem-admission"
        }),
    )
    .await;
    assert_eq!(mismatched_admission.status, StatusCode::BAD_REQUEST);

    let duplicate_admission = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": member.account_id,
            "source_fact_kind": "realm_admin_invite",
            "source_fact_id": "realm-idem-admission-second-key",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-idem-admission-second-key"
        }),
    )
    .await;
    assert_eq!(duplicate_admission.status, StatusCode::BAD_REQUEST);
}

async fn create_realm(
    app: &Router,
    requester: &SignedInUser,
    sponsor: Option<&SignedInUser>,
    steward: Option<&SignedInUser>,
    approver_id: &str,
    target_realm_status: &str,
    prefix: &str,
) -> (String, String) {
    let request = post_json(
        app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": format!("Realm {prefix}"),
            "slug_candidate": format!("slug-{prefix}"),
            "purpose_text": "Calm bootstrap flow.",
            "venue_context_json": {
                "label": prefix,
                "city": "Tokyo"
            },
            "expected_member_shape_json": {
                "pace": "slow"
            },
            "bootstrap_rationale_text": "Bounded early growth only.",
            "proposed_sponsor_account_id": sponsor.map(|value| value.account_id.clone()),
            "proposed_steward_account_id": steward.map(|value| value.account_id.clone()),
            "request_idempotency_key": format!("{prefix}-request")
        }),
    )
    .await;
    assert_eq!(request.status, StatusCode::OK);
    let realm_request_id = request.body["realm_request_id"]
        .as_str()
        .expect("realm request id must exist")
        .to_owned();

    let approval = operator_post_json(
        app,
        &format!("/api/internal/operator/realms/requests/{realm_request_id}/approve"),
        approver_id,
        json!({
            "target_realm_status": target_realm_status,
            "review_reason_code": if target_realm_status == "active" {
                "active_after_review"
            } else {
                "limited_bootstrap_active"
            },
            "steward_account_id": steward.map(|value| value.account_id.clone()),
            "sponsor_quota_total": sponsor.map(|_| 1),
            "corridor_starts_at": if target_realm_status == "limited_bootstrap" {
                Some((Utc::now() - Duration::minutes(5)).to_rfc3339())
            } else {
                None::<String>
            },
            "corridor_ends_at": if target_realm_status == "limited_bootstrap" {
                Some((Utc::now() + Duration::days(3)).to_rfc3339())
            } else {
                None::<String>
            },
            "corridor_member_cap": if target_realm_status == "limited_bootstrap" { Some(2) } else { None::<i64> },
            "corridor_sponsor_cap": if target_realm_status == "limited_bootstrap" { Some(1) } else { None::<i64> },
            "review_threshold_json": {
                "manual_review_after": 2
            },
            "review_decision_idempotency_key": format!("{prefix}-approve")
        }),
    )
    .await;
    assert_eq!(approval.status, StatusCode::OK);
    (
        approval.body["realm_id"]
            .as_str()
            .expect("realm id must exist")
            .to_owned(),
        realm_request_id,
    )
}

async fn sponsor_record_id_for_realm(client: &tokio_postgres::Client, realm_id: &str) -> String {
    client
        .query_one(
            "
            SELECT realm_sponsor_record_id::text AS realm_sponsor_record_id
            FROM dao.realm_sponsor_records
            WHERE realm_id = $1
            ORDER BY created_at DESC, realm_sponsor_record_id DESC
            LIMIT 1
            ",
            &[&realm_id],
        )
        .await
        .expect("realm sponsor record must exist")
        .get("realm_sponsor_record_id")
}

async fn admission_count_for_account(
    client: &tokio_postgres::Client,
    realm_id: &str,
    account_id: &str,
) -> i64 {
    let account_id = Uuid::parse_str(account_id).expect("account id must be uuid");
    client
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM dao.realm_admissions
            WHERE realm_id = $1
              AND account_id = $2
            ",
            &[&realm_id, &account_id],
        )
        .await
        .expect("admission count must query")
        .get("count")
}

async fn set_sponsor_status(
    client: &tokio_postgres::Client,
    sponsor_record_id: &str,
    sponsor_status: &str,
    status_reason_code: &str,
) {
    client
        .execute(
            "
            UPDATE dao.realm_sponsor_records
            SET sponsor_status = $2,
                status_reason_code = $3,
                updated_at = CURRENT_TIMESTAMP
            WHERE realm_sponsor_record_id::text = $1
            ",
            &[&sponsor_record_id, &sponsor_status, &status_reason_code],
        )
        .await
        .expect("sponsor status must update");
}

async fn expire_corridor_without_rebuild(client: &tokio_postgres::Client, realm_id: &str) {
    client
        .execute(
            "
            UPDATE dao.bootstrap_corridors
            SET ends_at = CURRENT_TIMESTAMP - interval '1 minute'
            WHERE realm_id = $1
            ",
            &[&realm_id],
        )
        .await
        .expect("corridor must update");
}

async fn disable_corridor_without_rebuild(client: &tokio_postgres::Client, realm_id: &str) {
    client
        .execute(
            "
            UPDATE dao.bootstrap_corridors
            SET corridor_status = 'disabled_by_operator',
                disabled_reason_code = 'operator_restriction',
                updated_at = CURRENT_TIMESTAMP
            WHERE realm_id = $1
            ",
            &[&realm_id],
        )
        .await
        .expect("corridor must disable");
}

async fn current_projection_corridor_status(
    client: &tokio_postgres::Client,
    realm_id: &str,
) -> String {
    client
        .query_one(
            "
            SELECT corridor_status
            FROM projection.realm_bootstrap_views
            WHERE realm_id = $1
            ",
            &[&realm_id],
        )
        .await
        .expect("projection row must exist")
        .get("corridor_status")
}

async fn current_projection_rebuild_generation(
    client: &tokio_postgres::Client,
    realm_id: &str,
) -> i64 {
    client
        .query_one(
            "
            SELECT rebuild_generation
            FROM projection.realm_bootstrap_views
            WHERE realm_id = $1
            ",
            &[&realm_id],
        )
        .await
        .expect("projection row must exist")
        .get("rebuild_generation")
}

async fn set_realm_status(
    client: &tokio_postgres::Client,
    realm_id: &str,
    realm_status: &str,
    public_reason_code: &str,
) {
    client
        .execute(
            "
            UPDATE dao.realms
            SET realm_status = $2,
                public_reason_code = $3,
                updated_at = CURRENT_TIMESTAMP
            WHERE realm_id = $1
            ",
            &[&realm_id, &realm_status, &public_reason_code],
        )
        .await
        .expect("realm status must update");
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
            VALUES ($1, $2, $3, 'realm bootstrap test role')
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
        serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            json!({
                "raw_body": String::from_utf8_lossy(&bytes).to_string()
            })
        })
    };

    JsonResponse { status, body }
}
