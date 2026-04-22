use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use chrono::{DateTime, Duration, Utc};
use musubi_backend::{build_app, new_state_from_config, new_test_state};
use musubi_db_runtime::DbConfig;
use serde_json::{Value, json};
use tokio_postgres::error::SqlState;
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
    assert!(request.body.get("requested_by_account_id").is_none());
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
    assert!(requester_view.body.get("requested_by_account_id").is_none());
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
            .get("requested_by_account_id")
            .is_none()
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
async fn realm_request_body_size_is_bounded() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-large-body", "realm-large-body").await;

    let oversized = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": "Large body Realm",
            "slug_candidate": "large-body-realm",
            "purpose_text": "x".repeat(17 * 1024),
            "venue_context_json": {
                "city": "Tokyo",
                "venue_type": "cafe"
            },
            "expected_member_shape_json": {
                "pace": "quiet"
            },
            "bootstrap_rationale_text": "The route must reject oversized JSON before deserialization.",
            "request_idempotency_key": "large-body-realm"
        }),
    )
    .await;

    assert_eq!(oversized.status, StatusCode::PAYLOAD_TOO_LARGE);
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
async fn operator_realm_request_list_is_bounded_and_cursor_paged() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let operator_id = insert_operator_account(&client, "reviewer").await;

    for index in 0..3 {
        let requester = sign_in(
            &app,
            &format!("pi-user-realm-list-{index}"),
            &format!("realm-list-{index}"),
        )
        .await;
        let request = post_json(
            &app,
            "/api/realms/requests",
            Some(requester.token.as_str()),
            json!({
                "display_name": format!("Realm list {index}"),
                "slug_candidate": format!("realm-list-{index}"),
                "purpose_text": "Operator list pagination coverage.",
                "venue_context_json": {
                    "city": "Tokyo",
                    "venue": format!("list-{index}")
                },
                "expected_member_shape_json": {
                    "pace": "calm",
                    "index": index
                },
                "bootstrap_rationale_text": "Keep the operator list bounded.",
                "request_idempotency_key": format!("realm-list-{index}")
            }),
        )
        .await;
        assert_eq!(request.status, StatusCode::OK);
    }

    let first_page = operator_get_json(
        &app,
        "/api/internal/operator/realms/requests?limit=2",
        &operator_id,
    )
    .await;
    assert_eq!(first_page.status, StatusCode::OK);
    let first_items = first_page
        .body
        .as_array()
        .expect("first page must be an array");
    assert_eq!(first_items.len(), 2);
    let cursor_item = &first_items[1];
    let cursor_created_at = cursor_item["created_at"]
        .as_str()
        .expect("cursor created_at must exist");
    let cursor_request_id = cursor_item["realm_request_id"]
        .as_str()
        .expect("cursor realm_request_id must exist");

    let second_page = operator_get_json(
        &app,
        &format!(
            "/api/internal/operator/realms/requests?limit=2&before_created_at={cursor_created_at}&before_realm_request_id={cursor_request_id}"
        ),
        &operator_id,
    )
    .await;
    assert_eq!(second_page.status, StatusCode::OK);
    let second_items = second_page
        .body
        .as_array()
        .expect("second page must be an array");
    assert_eq!(second_items.len(), 1);
    assert_ne!(
        second_items[0]["realm_request_id"],
        first_items[0]["realm_request_id"]
    );
    assert_ne!(
        second_items[0]["realm_request_id"],
        first_items[1]["realm_request_id"]
    );

    let invalid_limit = operator_get_json(
        &app,
        "/api/internal/operator/realms/requests?limit=0",
        &operator_id,
    )
    .await;
    assert_eq!(invalid_limit.status, StatusCode::BAD_REQUEST);
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
    let third_duplicate_request_id = third_duplicate.body["realm_request_id"]
        .as_str()
        .expect("third duplicate request id must exist");
    let duplicate_operator_view = operator_get_json(
        &app,
        &format!("/api/internal/operator/realms/requests/{third_duplicate_request_id}"),
        &approver_id,
    )
    .await;
    assert_eq!(duplicate_operator_view.status, StatusCode::OK);
    assert_eq!(
        duplicate_operator_view.body["open_review_triggers"][0]["trigger_kind"],
        "duplicate_venue_context"
    );
    let duplicate_trigger = client
        .query_one(
            "
            SELECT related_realm_request_id::text AS related_realm_request_id,
                   context_json
            FROM dao.realm_review_triggers
            WHERE trigger_kind = 'duplicate_venue_context'
              AND trigger_state = 'open'
            ",
            &[],
        )
        .await
        .expect("duplicate trigger must query");
    assert_eq!(
        duplicate_trigger.get::<_, String>("related_realm_request_id"),
        third_duplicate_request_id
    );
    let duplicate_context: Value = duplicate_trigger.get("context_json");
    assert_eq!(duplicate_context["matching_request_count"], 2);

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
        if index == 1 {
            assert_eq!(
                rejected.body["open_review_triggers"][0]["trigger_kind"],
                "repeated_rejected_requests"
            );
        }
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
    let repeated_request_id = repeated_request.body["realm_request_id"]
        .as_str()
        .expect("repeated request id must exist");
    let repeated_operator_view = operator_get_json(
        &app,
        &format!("/api/internal/operator/realms/requests/{repeated_request_id}"),
        &approver_id,
    )
    .await;
    assert_eq!(repeated_operator_view.status, StatusCode::OK);
    assert_eq!(
        repeated_operator_view.body["open_review_triggers"][0]["trigger_kind"],
        "repeated_rejected_requests"
    );
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
async fn realm_bootstrap_internal_json_posts_reject_oversized_bodies() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());

    for path in [
        "/api/internal/operator/realms/requests/request-id/approve",
        "/api/internal/operator/realms/requests/request-id/reject",
        "/api/internal/realms/realm-id/sponsor-records",
        "/api/internal/realms/realm-id/admissions",
    ] {
        let request = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(vec![b'a'; 20_000]))
            .expect("request must build");

        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("app should respond");

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE, "{path}");
    }
}

#[tokio::test]
async fn realm_request_rejects_unknown_candidate_account_without_enumeration() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-candidate-validation",
        "realm-candidate-validation",
    )
    .await;

    let response = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": "候補検証Realm",
            "slug_candidate": "candidate-validation-realm",
            "purpose_text": "候補アカウント検証",
            "venue_context_json": {"area": "tokyo"},
            "expected_member_shape_json": {"shape": "small"},
            "bootstrap_rationale_text": "未知の候補は公開面で列挙させない",
            "proposed_sponsor_account_id": Uuid::new_v4(),
            "request_idempotency_key": "candidate-validation-request"
        }),
    )
    .await;

    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert!(
        response
            .body
            .to_string()
            .contains("provided sponsor/steward account id is invalid")
    );
    assert!(!response.body.to_string().contains("active account"));
    assert!(!response.body.to_string().contains("not found"));
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
async fn bootstrap_summary_authorizes_against_latest_admission_status() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-summary-latest-a",
        "realm-summary-latest-a",
    )
    .await;
    let member = sign_in(
        &app,
        "pi-user-realm-summary-latest-b",
        "realm-summary-latest-b",
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
        "active",
        "realm-summary-latest",
    )
    .await;
    let admission = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": member.account_id,
            "source_fact_kind": "realm_admin_invite",
            "source_fact_id": "realm-summary-latest-admission",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-summary-latest-admission"
        }),
    )
    .await;
    assert_eq!(admission.status, StatusCode::OK);
    assert_eq!(admission.body["admission_status"], "admitted");

    let admitted_summary = get_json(
        &app,
        &format!("/api/projection/realms/{realm_id}/bootstrap-summary"),
        Some(member.token.as_str()),
    )
    .await;
    assert_eq!(admitted_summary.status, StatusCode::OK);

    append_admission_status_for_test(
        &client,
        &realm_id,
        &member.account_id,
        &approver_id,
        "revoked",
        "operator_restriction",
        "realm-summary-latest-revoked",
    )
    .await;

    let revoked_summary = get_json(
        &app,
        &format!("/api/projection/realms/{realm_id}/bootstrap-summary"),
        Some(member.token.as_str()),
    )
    .await;
    assert_eq!(revoked_summary.status, StatusCode::NOT_FOUND);
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
async fn approval_sponsor_record_uses_normalized_review_idempotency_key() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-normalized-review-a",
        "realm-normalized-review-a",
    )
    .await;
    let sponsor = sign_in(
        &app,
        "pi-user-realm-normalized-review-b",
        "realm-normalized-review-b",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let request = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": "Normalized review key realm",
            "slug_candidate": "normalized-review-key-realm",
            "purpose_text": "Sponsor lineage should use normalized review keys.",
            "venue_context_json": {
                "city": "Tokyo",
                "venue_type": "community_space"
            },
            "expected_member_shape_json": {
                "pace": "quiet"
            },
            "bootstrap_rationale_text": "Keep approval replay keys stable.",
            "proposed_sponsor_account_id": sponsor.account_id,
            "request_idempotency_key": "realm-normalized-review-request"
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
            "sponsor_quota_total": 1,
            "review_decision_idempotency_key": "  realm-normalized-review-approve  "
        }),
    )
    .await;
    assert_eq!(approval.status, StatusCode::OK);
    let realm_id = approval.body["realm_id"]
        .as_str()
        .expect("realm id must exist");

    let row = client
        .query_one(
            "
            SELECT
                request.review_decision_idempotency_key,
                sponsor.request_idempotency_key AS sponsor_request_idempotency_key
            FROM dao.realm_requests request
            JOIN dao.realms realm
              ON realm.created_from_realm_request_id = request.realm_request_id
            JOIN dao.realm_sponsor_records sponsor
              ON sponsor.realm_id = realm.realm_id
            WHERE request.realm_request_id::text = $1
              AND realm.realm_id = $2
            ",
            &[&realm_request_id, &realm_id],
        )
        .await
        .expect("approval sponsor record must query");
    assert_eq!(
        row.get::<_, Option<String>>("review_decision_idempotency_key")
            .as_deref(),
        Some("realm-normalized-review-approve")
    );
    assert_eq!(
        row.get::<_, String>("sponsor_request_idempotency_key"),
        "realm-normalized-review-approve"
    );
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
async fn pending_admission_can_be_superseded_by_operator_admission_after_review() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-supersede-a", "realm-supersede-a").await;
    let sponsor = sign_in(&app, "pi-user-realm-supersede-b", "realm-supersede-b").await;
    let first_member = sign_in(&app, "pi-user-realm-supersede-c", "realm-supersede-c").await;
    let reviewed_member = sign_in(&app, "pi-user-realm-supersede-d", "realm-supersede-d").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        Some(&sponsor),
        None,
        &approver_id,
        "active",
        "realm-supersede-001",
    )
    .await;
    let sponsor_record_id = sponsor_record_id_for_realm(&client, &realm_id).await;

    let first = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": first_member.account_id,
            "sponsor_record_id": sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-supersede-first",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-supersede-first"
        }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.body["admission_status"], "admitted");

    let pending = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": reviewed_member.account_id,
            "sponsor_record_id": sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-supersede-pending",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-supersede-pending"
        }),
    )
    .await;
    assert_eq!(pending.status, StatusCode::OK);
    assert_eq!(pending.body["admission_kind"], "review_required");
    assert_eq!(pending.body["admission_status"], "pending");

    let approved_after_review = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": reviewed_member.account_id,
            "source_fact_kind": "operator_review",
            "source_fact_id": "realm-supersede-approved",
            "source_snapshot_json": {
                "review_outcome": "approved"
            },
            "request_idempotency_key": "realm-supersede-approved"
        }),
    )
    .await;
    assert_eq!(approved_after_review.status, StatusCode::OK);
    assert_eq!(approved_after_review.body["admission_kind"], "normal");
    assert_eq!(approved_after_review.body["admission_status"], "admitted");

    let member_summary = get_json(
        &app,
        &format!("/api/projection/realms/{realm_id}/bootstrap-summary"),
        Some(reviewed_member.token.as_str()),
    )
    .await;
    assert_eq!(member_summary.status, StatusCode::OK);
    assert_eq!(
        member_summary.body["admission_view"]["admission_status"],
        "admitted"
    );
}

#[tokio::test]
async fn admitted_member_is_not_downgraded_by_later_admission_request_after_corridor_expiry() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-preserve-a", "realm-preserve-a").await;
    let member = sign_in(&app, "pi-user-realm-preserve-b", "realm-preserve-b").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        None,
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-preserve-admitted-001",
    )
    .await;

    let first = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": member.account_id,
            "source_fact_kind": "operator_review",
            "source_fact_id": "realm-preserve-first",
            "source_snapshot_json": {
                "review_outcome": "approved"
            },
            "request_idempotency_key": "realm-preserve-first"
        }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.body["admission_kind"], "corridor");
    assert_eq!(first.body["admission_status"], "admitted");

    expire_corridor_without_rebuild(&client, &realm_id).await;

    let later_body = json!({
            "account_id": member.account_id,
            "source_fact_kind": "operator_review",
            "source_fact_id": "realm-preserve-after-expiry",
            "source_snapshot_json": {
                "review_outcome": "still_admitted"
            },
            "request_idempotency_key": "realm-preserve-after-expiry"
    });
    let later = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        later_body.clone(),
    )
    .await;
    assert_eq!(later.status, StatusCode::OK);
    assert_eq!(later.body["admission_kind"], "corridor");
    assert_eq!(later.body["admission_status"], "admitted");
    let later_admission_id = later.body["realm_admission_id"]
        .as_str()
        .expect("admission id must exist")
        .to_owned();
    assert_eq!(
        admission_count_for_account(&client, &realm_id, &member.account_id).await,
        1
    );

    let drifted_replay = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": member.account_id,
            "source_fact_kind": "operator_review",
            "source_fact_id": "realm-preserve-after-expiry-drift",
            "source_snapshot_json": {
                "review_outcome": "still_admitted"
            },
            "request_idempotency_key": "realm-preserve-after-expiry"
        }),
    )
    .await;
    assert_eq!(drifted_replay.status, StatusCode::BAD_REQUEST);

    let replayed_later = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        later_body,
    )
    .await;
    assert_eq!(replayed_later.status, StatusCode::OK);
    assert_eq!(
        replayed_later.body["realm_admission_id"],
        later_admission_id
    );
    assert_eq!(
        admission_idempotency_key_count(&client, &realm_id, "realm-preserve-after-expiry").await,
        1
    );

    let member_summary = get_json(
        &app,
        &format!("/api/projection/realms/{realm_id}/bootstrap-summary"),
        Some(member.token.as_str()),
    )
    .await;
    assert_eq!(member_summary.status, StatusCode::OK);
    assert_eq!(
        member_summary.body["admission_view"]["admission_status"],
        "admitted"
    );
}

#[tokio::test]
async fn corridor_member_cap_counts_distinct_accounts_when_pending_rows_repeat() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-corridor-distinct-a",
        "realm-corridor-distinct-a",
    )
    .await;
    let repeated_member = sign_in(
        &app,
        "pi-user-realm-corridor-distinct-b",
        "realm-corridor-distinct-b",
    )
    .await;
    let next_member = sign_in(
        &app,
        "pi-user-realm-corridor-distinct-c",
        "realm-corridor-distinct-c",
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
        "realm-corridor-distinct-001",
    )
    .await;
    let corridor_id: Uuid = client
        .query_one(
            "
            SELECT bootstrap_corridor_id
            FROM dao.bootstrap_corridors
            WHERE realm_id = $1
            ",
            &[&realm_id],
        )
        .await
        .expect("bootstrap corridor must query")
        .get("bootstrap_corridor_id");
    let repeated_member_id =
        Uuid::parse_str(&repeated_member.account_id).expect("member id must be uuid");
    let approver_uuid = Uuid::parse_str(&approver_id).expect("operator id must be uuid");

    for index in 0..2 {
        let source_fact_id = format!("realm-corridor-distinct-pending-{index}");
        let request_idempotency_key = format!("realm-corridor-distinct-pending-{index}");
        client
            .execute(
                "
                INSERT INTO dao.realm_admissions (
                    realm_admission_id,
                    realm_id,
                    account_id,
                    admission_kind,
                    admission_status,
                    sponsor_record_id,
                    bootstrap_corridor_id,
                    granted_by_actor_kind,
                    granted_by_actor_id,
                    review_reason_code,
                    source_fact_kind,
                    source_fact_id,
                    source_snapshot_json,
                    request_idempotency_key,
                    request_payload_hash
                )
                VALUES (
                    $1,
                    $2,
                    $3,
                    'review_required',
                    'pending',
                    NULL,
                    $4,
                    'operator',
                    $5,
                    'review_required',
                    'operator_review',
                    $6,
                    '{}'::jsonb,
                    $7,
                    repeat('7', 64)
                )
                ",
                &[
                    &Uuid::new_v4(),
                    &realm_id,
                    &repeated_member_id,
                    &corridor_id,
                    &approver_uuid,
                    &source_fact_id,
                    &request_idempotency_key,
                ],
            )
            .await
            .expect("pending corridor admission must insert");
    }

    let next = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": next_member.account_id,
            "source_fact_kind": "operator_review",
            "source_fact_id": "realm-corridor-distinct-next",
            "source_snapshot_json": {
                "review_outcome": "approved"
            },
            "request_idempotency_key": "realm-corridor-distinct-next"
        }),
    )
    .await;
    assert_eq!(next.status, StatusCode::OK);
    assert_eq!(next.body["admission_kind"], "corridor");
    assert_eq!(next.body["admission_status"], "admitted");
}

#[tokio::test]
async fn sponsor_quota_counts_distinct_accounts_when_pending_rows_repeat() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-sponsor-distinct-a",
        "realm-sponsor-distinct-a",
    )
    .await;
    let sponsor = sign_in(
        &app,
        "pi-user-realm-sponsor-distinct-b",
        "realm-sponsor-distinct-b",
    )
    .await;
    let repeated_member = sign_in(
        &app,
        "pi-user-realm-sponsor-distinct-c",
        "realm-sponsor-distinct-c",
    )
    .await;
    let next_member = sign_in(
        &app,
        "pi-user-realm-sponsor-distinct-d",
        "realm-sponsor-distinct-d",
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
        "realm-sponsor-distinct-001",
    )
    .await;
    let sponsor_record_id = sponsor_record_id_for_realm(&client, &realm_id).await;
    client
        .execute(
            "
            UPDATE dao.realm_sponsor_records
            SET quota_total = 2
            WHERE realm_sponsor_record_id::text = $1
            ",
            &[&sponsor_record_id],
        )
        .await
        .expect("sponsor quota must update");

    let sponsor_record_uuid =
        Uuid::parse_str(&sponsor_record_id).expect("sponsor record id must be uuid");
    let repeated_member_id =
        Uuid::parse_str(&repeated_member.account_id).expect("member id must be uuid");
    let approver_uuid = Uuid::parse_str(&approver_id).expect("operator id must be uuid");

    for index in 0..2 {
        let source_fact_id = format!("realm-sponsor-distinct-pending-{index}");
        let request_idempotency_key = format!("realm-sponsor-distinct-pending-{index}");
        client
            .execute(
                "
                INSERT INTO dao.realm_admissions (
                    realm_admission_id,
                    realm_id,
                    account_id,
                    admission_kind,
                    admission_status,
                    sponsor_record_id,
                    bootstrap_corridor_id,
                    granted_by_actor_kind,
                    granted_by_actor_id,
                    review_reason_code,
                    source_fact_kind,
                    source_fact_id,
                    source_snapshot_json,
                    request_idempotency_key,
                    request_payload_hash
                )
                VALUES (
                    $1,
                    $2,
                    $3,
                    'sponsor_backed',
                    'pending',
                    $4,
                    NULL,
                    'operator',
                    $5,
                    'active_after_review',
                    'operator_review',
                    $6,
                    '{}'::jsonb,
                    $7,
                    repeat('8', 64)
                )
                ",
                &[
                    &Uuid::new_v4(),
                    &realm_id,
                    &repeated_member_id,
                    &sponsor_record_uuid,
                    &approver_uuid,
                    &source_fact_id,
                    &request_idempotency_key,
                ],
            )
            .await
            .expect("pending sponsor admission must insert");
    }

    let next = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": next_member.account_id,
            "sponsor_record_id": sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-sponsor-distinct-next",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-sponsor-distinct-next"
        }),
    )
    .await;
    assert_eq!(next.status, StatusCode::OK);
    assert_eq!(next.body["admission_kind"], "sponsor_backed");
    assert_eq!(next.body["admission_status"], "admitted");
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
async fn sponsor_lineage_churn_counts_sponsor_accounts_for_caps_and_summary() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-sponsor-lineage-a",
        "realm-sponsor-lineage-a",
    )
    .await;
    let sponsor = sign_in(
        &app,
        "pi-user-realm-sponsor-lineage-b",
        "realm-sponsor-lineage-b",
    )
    .await;
    let first_member = sign_in(
        &app,
        "pi-user-realm-sponsor-lineage-c",
        "realm-sponsor-lineage-c",
    )
    .await;
    let second_member = sign_in(
        &app,
        "pi-user-realm-sponsor-lineage-d",
        "realm-sponsor-lineage-d",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let request = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        json!({
            "display_name": "Sponsor lineage realm",
            "slug_candidate": "sponsor-lineage-realm",
            "purpose_text": "Sponsor status churn must not consume extra sponsor slots.",
            "venue_context_json": {
                "city": "Tokyo",
                "venue_type": "gallery"
            },
            "expected_member_shape_json": {
                "size": "small"
            },
            "bootstrap_rationale_text": "One sponsor account may have multiple records over time.",
            "proposed_sponsor_account_id": sponsor.account_id,
            "request_idempotency_key": "realm-sponsor-lineage-request"
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
            "sponsor_quota_total": 3,
            "corridor_starts_at": (Utc::now() - Duration::minutes(5)).to_rfc3339(),
            "corridor_ends_at": (Utc::now() + Duration::days(3)).to_rfc3339(),
            "corridor_member_cap": 3,
            "corridor_sponsor_cap": 1,
            "review_threshold_json": {},
            "review_decision_idempotency_key": "realm-sponsor-lineage-approve"
        }),
    )
    .await;
    assert_eq!(approval.status, StatusCode::OK);
    let realm_id = approval.body["realm_id"]
        .as_str()
        .expect("realm id must exist")
        .to_owned();
    let initial_sponsor_record_id = sponsor_record_id_for_realm(&client, &realm_id).await;

    let first = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": first_member.account_id,
            "sponsor_record_id": initial_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-sponsor-lineage-first",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-sponsor-lineage-first"
        }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.body["admission_kind"], "sponsor_backed");
    assert_eq!(first.body["admission_status"], "admitted");

    set_sponsor_status(
        &client,
        &initial_sponsor_record_id,
        "rate_limited",
        "sponsor_rate_limited",
    )
    .await;
    let reactivated_once = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "active",
            "quota_total": 3,
            "status_reason_code": "limited_bootstrap_active",
            "request_idempotency_key": "realm-sponsor-lineage-reactivate-once"
        }),
    )
    .await;
    assert_eq!(reactivated_once.status, StatusCode::OK);
    let reactivated_once_id = reactivated_once.body["realm_sponsor_record_id"]
        .as_str()
        .expect("reactivated sponsor record id must exist");

    set_sponsor_status(
        &client,
        reactivated_once_id,
        "rate_limited",
        "sponsor_rate_limited",
    )
    .await;
    let reactivated_twice = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "active",
            "quota_total": 3,
            "status_reason_code": "limited_bootstrap_active",
            "request_idempotency_key": "realm-sponsor-lineage-reactivate-twice"
        }),
    )
    .await;
    assert_eq!(reactivated_twice.status, StatusCode::OK);
    let current_sponsor_record_id = reactivated_twice.body["realm_sponsor_record_id"]
        .as_str()
        .expect("current sponsor record id must exist");

    let review_summary = operator_get_json(
        &app,
        &format!("/api/internal/operator/realms/{realm_id}/review-summary"),
        &approver_id,
    )
    .await;
    assert_eq!(review_summary.status, StatusCode::OK);
    assert_eq!(review_summary.body["active_sponsor_count"], 1);
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
    assert!(!trigger_kinds.contains(&"sponsor_concentration"));

    let second = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": second_member.account_id,
            "sponsor_record_id": initial_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-sponsor-lineage-second",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-sponsor-lineage-second"
        }),
    )
    .await;
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(second.body["admission_kind"], "sponsor_backed");
    assert_eq!(second.body["admission_status"], "admitted");
    assert_eq!(second.body["sponsor_record_id"], current_sponsor_record_id);
}

#[tokio::test]
async fn sponsor_quota_counts_reactivated_sponsor_lineage() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-sponsor-quota-lineage-a",
        "realm-sponsor-quota-lineage-a",
    )
    .await;
    let sponsor = sign_in(
        &app,
        "pi-user-realm-sponsor-quota-lineage-b",
        "realm-sponsor-quota-lineage-b",
    )
    .await;
    let first_member = sign_in(
        &app,
        "pi-user-realm-sponsor-quota-lineage-c",
        "realm-sponsor-quota-lineage-c",
    )
    .await;
    let second_member = sign_in(
        &app,
        "pi-user-realm-sponsor-quota-lineage-d",
        "realm-sponsor-quota-lineage-d",
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
        "limited_bootstrap",
        "realm-sponsor-quota-lineage",
    )
    .await;
    let initial_sponsor_record_id = sponsor_record_id_for_realm(&client, &realm_id).await;

    let first = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": first_member.account_id,
            "sponsor_record_id": initial_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-sponsor-quota-lineage-first",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-sponsor-quota-lineage-first"
        }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.body["admission_kind"], "sponsor_backed");
    assert_eq!(first.body["admission_status"], "admitted");

    let rate_limited = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "rate_limited",
            "quota_total": 1,
            "status_reason_code": "sponsor_rate_limited",
            "request_idempotency_key": "realm-sponsor-quota-lineage-rate-limited"
        }),
    )
    .await;
    assert_eq!(rate_limited.status, StatusCode::OK);

    let reactivated = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "active",
            "quota_total": 1,
            "status_reason_code": "limited_bootstrap_active",
            "request_idempotency_key": "realm-sponsor-quota-lineage-reactivated"
        }),
    )
    .await;
    assert_eq!(reactivated.status, StatusCode::OK);
    let current_sponsor_record_id = reactivated.body["realm_sponsor_record_id"]
        .as_str()
        .expect("current sponsor record id must exist");

    let second = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": second_member.account_id,
            "sponsor_record_id": initial_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-sponsor-quota-lineage-second",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-sponsor-quota-lineage-second"
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
    assert_eq!(second.body["sponsor_record_id"], current_sponsor_record_id);
}

#[tokio::test]
async fn restrictive_sponsor_status_requires_existing_lineage() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-sponsor-restrictive-a",
        "realm-sponsor-restrictive-a",
    )
    .await;
    let sponsor = sign_in(
        &app,
        "pi-user-realm-sponsor-restrictive-b",
        "realm-sponsor-restrictive-b",
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
        "realm-sponsor-restrictive",
    )
    .await;

    let rate_limited = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "rate_limited",
            "quota_total": 1,
            "status_reason_code": "sponsor_rate_limited",
            "request_idempotency_key": "realm-sponsor-restrictive-rate-limited"
        }),
    )
    .await;
    assert_eq!(rate_limited.status, StatusCode::BAD_REQUEST);

    let revoked = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "revoked",
            "quota_total": 1,
            "status_reason_code": "sponsor_revoked",
            "request_idempotency_key": "realm-sponsor-restrictive-revoked"
        }),
    )
    .await;
    assert_eq!(revoked.status, StatusCode::BAD_REQUEST);
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
    let third_member = sign_in(&app, "pi-user-realm-sponsor-e", "realm-sponsor-e").await;
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

    let active_record = operator_post_json(
        &app,
        &format!("/api/internal/realms/{rate_limited_realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "active",
            "quota_total": 2,
            "status_reason_code": "limited_bootstrap_active",
            "request_idempotency_key": "realm-sponsor-reactivated-record"
        }),
    )
    .await;
    assert_eq!(active_record.status, StatusCode::OK);
    let active_sponsor_record_id = active_record.body["realm_sponsor_record_id"]
        .as_str()
        .expect("active sponsor record id must exist");

    let reactivated = operator_post_json(
        &app,
        &format!("/api/internal/realms/{rate_limited_realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": third_member.account_id,
            "sponsor_record_id": rate_limited_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-reactivated-admission",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-reactivated-admission"
        }),
    )
    .await;
    assert_eq!(reactivated.status, StatusCode::OK);
    assert_eq!(reactivated.body["admission_kind"], "sponsor_backed");
    assert_eq!(reactivated.body["admission_status"], "admitted");
    assert_eq!(
        reactivated.body["sponsor_record_id"],
        active_sponsor_record_id
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
async fn sponsor_lineage_can_progress_from_proposed_or_approved_to_active() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-sponsor-progress-a",
        "realm-sponsor-progress-a",
    )
    .await;
    let sponsor = sign_in(
        &app,
        "pi-user-realm-sponsor-progress-b",
        "realm-sponsor-progress-b",
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
        "active",
        "realm-sponsor-progress",
    )
    .await;

    let proposed = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "proposed",
            "quota_total": 2,
            "status_reason_code": "request_received",
            "request_idempotency_key": "realm-sponsor-progress-proposed"
        }),
    )
    .await;
    assert_eq!(proposed.status, StatusCode::OK);

    let approved = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "approved",
            "quota_total": 2,
            "status_reason_code": "limited_bootstrap_active",
            "request_idempotency_key": "realm-sponsor-progress-approved"
        }),
    )
    .await;
    assert_eq!(approved.status, StatusCode::OK);

    let active = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "active",
            "quota_total": 2,
            "status_reason_code": "active_after_review",
            "request_idempotency_key": "realm-sponsor-progress-active"
        }),
    )
    .await;
    assert_eq!(active.status, StatusCode::OK);
    assert_eq!(active.body["sponsor_status"], "active");
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
async fn inactive_sponsor_account_blocks_auto_admission_and_can_be_revoked() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-inactive-sponsor-a",
        "realm-inactive-sponsor-a",
    )
    .await;
    let sponsor = sign_in(
        &app,
        "pi-user-realm-inactive-sponsor-b",
        "realm-inactive-sponsor-b",
    )
    .await;
    let first_member = sign_in(
        &app,
        "pi-user-realm-inactive-sponsor-c",
        "realm-inactive-sponsor-c",
    )
    .await;
    let second_member = sign_in(
        &app,
        "pi-user-realm-inactive-sponsor-d",
        "realm-inactive-sponsor-d",
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
        "realm-inactive-sponsor",
    )
    .await;
    let stale_active_sponsor_record_id = sponsor_record_id_for_realm(&client, &realm_id).await;
    set_account_state(&client, &sponsor.account_id, "suspended").await;

    let replayed_existing_sponsor = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "active",
            "quota_total": 1,
            "status_reason_code": "active_after_review",
            "request_idempotency_key": "realm-inactive-sponsor-approve"
        }),
    )
    .await;
    assert_eq!(replayed_existing_sponsor.status, StatusCode::OK);
    assert_eq!(
        replayed_existing_sponsor.body["realm_sponsor_record_id"],
        stale_active_sponsor_record_id
    );

    let blocked = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": first_member.account_id,
            "sponsor_record_id": stale_active_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-inactive-sponsor-blocked",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-inactive-sponsor-blocked"
        }),
    )
    .await;
    assert_eq!(blocked.status, StatusCode::OK);
    assert_eq!(blocked.body["admission_kind"], "review_required");
    assert_eq!(blocked.body["admission_status"], "pending");
    assert_eq!(blocked.body["review_reason_code"], "sponsor_revoked");

    let revoked_sponsor_record = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/sponsor-records"),
        &approver_id,
        json!({
            "sponsor_account_id": sponsor.account_id,
            "sponsor_status": "revoked",
            "quota_total": 1,
            "status_reason_code": "sponsor_revoked",
            "request_idempotency_key": "realm-inactive-sponsor-revoked"
        }),
    )
    .await;
    assert_eq!(revoked_sponsor_record.status, StatusCode::OK);
    let revoked_sponsor_record_id = revoked_sponsor_record.body["realm_sponsor_record_id"]
        .as_str()
        .expect("revoked sponsor record id must exist");

    let revoked = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": second_member.account_id,
            "sponsor_record_id": stale_active_sponsor_record_id,
            "source_fact_kind": "sponsor_invite",
            "source_fact_id": "realm-inactive-sponsor-revoked-admission",
            "source_snapshot_json": {},
            "request_idempotency_key": "realm-inactive-sponsor-revoked-admission"
        }),
    )
    .await;
    assert_eq!(revoked.status, StatusCode::OK);
    assert_eq!(revoked.body["admission_kind"], "review_required");
    assert_eq!(revoked.body["admission_status"], "pending");
    assert_eq!(revoked.body["review_reason_code"], "sponsor_revoked");
    assert_eq!(revoked.body["sponsor_record_id"], revoked_sponsor_record_id);
}

#[tokio::test]
async fn sponsor_backed_admission_waits_for_sponsor_lineage_lock() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-sponsor-lock-a", "realm-sponsor-lock-a").await;
    let sponsor = sign_in(&app, "pi-user-realm-sponsor-lock-b", "realm-sponsor-lock-b").await;
    let member = sign_in(&app, "pi-user-realm-sponsor-lock-c", "realm-sponsor-lock-c").await;
    let mut lock_client = test_db_client().await;
    let approver_id = insert_operator_account(&lock_client, "approver").await;

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        Some(&sponsor),
        None,
        &approver_id,
        "active",
        "realm-sponsor-lock",
    )
    .await;
    let active_sponsor_record_id = sponsor_record_id_for_realm(&lock_client, &realm_id).await;
    let sponsor_account_id =
        Uuid::parse_str(&sponsor.account_id).expect("sponsor account id must be a uuid");

    let lock_tx = lock_client
        .transaction()
        .await
        .expect("lock transaction must start");
    lock_sponsor_lineage_for_test(&lock_tx, &realm_id, &sponsor_account_id).await;

    let admission_app = app.clone();
    let admission_path = format!("/api/internal/realms/{realm_id}/admissions");
    let admission_operator_id = approver_id.clone();
    let admission_member_id = member.account_id.clone();
    let admission_sponsor_record_id = active_sponsor_record_id.clone();
    let mut admission_task = tokio::spawn(async move {
        operator_post_json(
            &admission_app,
            &admission_path,
            &admission_operator_id,
            json!({
                "account_id": admission_member_id,
                "sponsor_record_id": admission_sponsor_record_id,
                "source_fact_kind": "sponsor_invite",
                "source_fact_id": "realm-sponsor-lock-admission",
                "source_snapshot_json": {},
                "request_idempotency_key": "realm-sponsor-lock-admission"
            }),
        )
        .await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(200), &mut admission_task)
            .await
            .is_err()
    );

    lock_tx
        .commit()
        .await
        .expect("lock transaction must commit");

    let admission = admission_task.await.expect("admission task must complete");
    assert_eq!(admission.status, StatusCode::OK);
    assert_eq!(admission.body["admission_kind"], "sponsor_backed");
    assert_eq!(admission.body["admission_status"], "admitted");
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

    let disabled_review_summary = operator_get_json(
        &app,
        &format!("/api/internal/operator/realms/{disabled_realm_id}/review-summary"),
        &approver_id,
    )
    .await;
    assert_eq!(disabled_review_summary.status, StatusCode::OK);
    assert_eq!(
        disabled_review_summary.body["corridor_status"],
        "disabled_by_operator"
    );
    assert_eq!(
        disabled_review_summary.body["corridor_remaining_seconds"],
        0
    );
}

#[tokio::test]
async fn summary_reads_derive_expired_corridor_without_unrelated_write() {
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
    assert_eq!(current_corridor_status(&client, &realm_id).await, "active");
    assert_eq!(
        current_projection_corridor_status(&client, &realm_id).await,
        "active"
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
async fn rebuild_realm_bootstrap_views_accepts_empty_json_body() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let rebuild = operator_post_json(
        &app,
        "/api/internal/projection/realms/rebuild",
        &approver_id,
        json!({}),
    )
    .await;
    assert_eq!(rebuild.status, StatusCode::OK);
    assert!(rebuild.body["bootstrap_view_count"].as_i64().is_some());
    assert!(rebuild.body["admission_view_count"].as_i64().is_some());
    assert!(rebuild.body["review_summary_count"].as_i64().is_some());
}

#[tokio::test]
async fn participant_summary_read_skips_noop_operator_projection_refresh() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-summary-noop-a", "realm-summary-noop-a").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        None,
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-summary-noop",
    )
    .await;
    let bootstrap_projected_at = current_bootstrap_last_projected_at(&client, &realm_id).await;
    let review_projected_at = current_review_summary_last_projected_at(&client, &realm_id).await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let summary = get_json(
        &app,
        &format!("/api/projection/realms/{realm_id}/bootstrap-summary"),
        Some(requester.token.as_str()),
    )
    .await;
    assert_eq!(summary.status, StatusCode::OK);
    assert_eq!(
        current_bootstrap_last_projected_at(&client, &realm_id).await,
        bootstrap_projected_at
    );
    assert_eq!(
        current_review_summary_last_projected_at(&client, &realm_id).await,
        review_projected_at
    );
}

#[tokio::test]
async fn operator_review_summary_read_refreshes_lag_metadata() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-summary-lag-a", "realm-summary-lag-a").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        None,
        None,
        &approver_id,
        "limited_bootstrap",
        "realm-summary-lag",
    )
    .await;
    client
        .execute(
            "
            UPDATE projection.realm_review_summaries
            SET projection_lag_ms = 999999,
                last_projected_at = last_projected_at - interval '1 second'
            WHERE realm_id = $1
            ",
            &[&realm_id],
        )
        .await
        .expect("review summary lag fixture must update");
    let projected_at = current_review_summary_last_projected_at(&client, &realm_id).await;

    let review_summary = operator_get_json(
        &app,
        &format!("/api/internal/operator/realms/{realm_id}/review-summary"),
        &approver_id,
    )
    .await;
    assert_eq!(review_summary.status, StatusCode::OK);
    assert!(
        review_summary.body["projection_lag_ms"]
            .as_i64()
            .unwrap_or(0)
            >= 0
    );
    assert!(current_review_summary_last_projected_at(&client, &realm_id).await > projected_at);
}

#[tokio::test]
async fn unauthorized_summary_read_does_not_expire_corridor_or_refresh_projection() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-summary-unauthorized-a",
        "realm-summary-unauthorized-a",
    )
    .await;
    let outsider = sign_in(
        &app,
        "pi-user-realm-summary-unauthorized-b",
        "realm-summary-unauthorized-b",
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
        "realm-summary-unauthorized",
    )
    .await;
    expire_corridor_without_rebuild(&client, &realm_id).await;
    assert_eq!(current_corridor_status(&client, &realm_id).await, "active");
    assert_eq!(
        current_projection_corridor_status(&client, &realm_id).await,
        "active"
    );

    let unauthorized_summary = get_json(
        &app,
        &format!("/api/projection/realms/{realm_id}/bootstrap-summary"),
        Some(outsider.token.as_str()),
    )
    .await;
    assert_eq!(unauthorized_summary.status, StatusCode::NOT_FOUND);
    assert_eq!(current_corridor_status(&client, &realm_id).await, "active");
    assert_eq!(
        current_projection_corridor_status(&client, &realm_id).await,
        "active"
    );

    let requester_summary = get_json(
        &app,
        &format!("/api/projection/realms/{realm_id}/bootstrap-summary"),
        Some(requester.token.as_str()),
    )
    .await;
    assert_eq!(requester_summary.status, StatusCode::OK);
    assert_eq!(current_corridor_status(&client, &realm_id).await, "active");
    assert_eq!(
        current_projection_corridor_status(&client, &realm_id).await,
        "active"
    );
    assert_eq!(
        requester_summary.body["bootstrap_view"]["corridor_status"],
        "expired"
    );
}

#[tokio::test]
async fn realm_bootstrap_rebuild_requires_operator_role() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let ordinary = sign_in(
        &app,
        "pi-user-realm-rebuild-ordinary",
        "realm-rebuild-ordinary",
    )
    .await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let missing_operator = request_json(
        &app,
        "POST",
        "/api/internal/projection/realms/rebuild",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(missing_operator.status, StatusCode::BAD_REQUEST);

    let ordinary_operator = request_json(
        &app,
        "POST",
        "/api/internal/projection/realms/rebuild",
        None,
        Some(&ordinary.account_id),
        None,
    )
    .await;
    assert_eq!(ordinary_operator.status, StatusCode::UNAUTHORIZED);

    let rebuild = request_json(
        &app,
        "POST",
        "/api/internal/projection/realms/rebuild",
        None,
        Some(&approver_id),
        None,
    )
    .await;
    assert_eq!(rebuild.status, StatusCode::OK);
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
async fn concurrent_sponsor_and_admission_replays_return_existing_rows() {
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
    let requester = sign_in(&app, "pi-user-realm-idem-race-a", "realm-idem-race-a").await;
    let sponsor = sign_in(&app, "pi-user-realm-idem-race-b", "realm-idem-race-b").await;
    let member = sign_in(&app, "pi-user-realm-idem-race-c", "realm-idem-race-c").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        None,
        None,
        &approver_id,
        "active",
        "realm-idem-race",
    )
    .await;

    let sponsor_path = format!("/api/internal/realms/{realm_id}/sponsor-records");
    let sponsor_body = json!({
        "sponsor_account_id": sponsor.account_id,
        "sponsor_status": "active",
        "quota_total": 2,
        "status_reason_code": "active_after_review",
        "request_idempotency_key": "realm-idem-race-sponsor"
    });
    let (first_sponsor, second_sponsor) = tokio::join!(
        operator_post_json(&app, &sponsor_path, &approver_id, sponsor_body.clone()),
        operator_post_json(&second_app, &sponsor_path, &approver_id, sponsor_body)
    );
    assert_eq!(first_sponsor.status, StatusCode::OK);
    assert_eq!(second_sponsor.status, StatusCode::OK);
    assert_eq!(
        first_sponsor.body["realm_sponsor_record_id"],
        second_sponsor.body["realm_sponsor_record_id"]
    );

    let admission_path = format!("/api/internal/realms/{realm_id}/admissions");
    let admission_body = json!({
        "account_id": member.account_id,
        "source_fact_kind": "realm_admin_invite",
        "source_fact_id": "realm-idem-race-admission",
        "source_snapshot_json": {},
        "request_idempotency_key": "realm-idem-race-admission"
    });
    let (first_admission, second_admission) = tokio::join!(
        operator_post_json(&app, &admission_path, &approver_id, admission_body.clone()),
        operator_post_json(&second_app, &admission_path, &approver_id, admission_body)
    );
    assert_eq!(first_admission.status, StatusCode::OK);
    assert_eq!(second_admission.status, StatusCode::OK);
    assert_eq!(
        first_admission.body["realm_admission_id"],
        second_admission.body["realm_admission_id"]
    );
}

#[tokio::test]
async fn request_and_admission_idempotency_replay_mismatch_is_rejected() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-idem-a", "realm-idem-a").await;
    let member = sign_in(&app, "pi-user-realm-idem-b", "realm-idem-b").await;
    let proposed_sponsor = sign_in(&app, "pi-user-realm-idem-c", "realm-idem-c").await;
    let proposed_steward = sign_in(&app, "pi-user-realm-idem-d", "realm-idem-d").await;
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
        "proposed_sponsor_account_id": proposed_sponsor.account_id,
        "proposed_steward_account_id": proposed_steward.account_id,
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
        request_body.clone(),
    )
    .await;
    assert_eq!(replayed_request.status, StatusCode::OK);
    assert_eq!(replayed_request.body["realm_request_id"], realm_request_id);

    set_account_state(&client, &proposed_sponsor.account_id, "suspended").await;
    set_account_state(&client, &proposed_steward.account_id, "suspended").await;
    let inactive_candidate_replay = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        request_body.clone(),
    )
    .await;
    assert_eq!(inactive_candidate_replay.status, StatusCode::OK);
    assert_eq!(
        inactive_candidate_replay.body["realm_request_id"],
        realm_request_id
    );

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
            "proposed_sponsor_account_id": proposed_sponsor.account_id,
            "proposed_steward_account_id": proposed_steward.account_id,
            "request_idempotency_key": "realm-idem-request"
        }),
    )
    .await;
    assert_eq!(mismatched_request.status, StatusCode::BAD_REQUEST);

    set_account_state(&client, &proposed_sponsor.account_id, "active").await;
    set_account_state(&client, &proposed_steward.account_id, "active").await;
    let approval = operator_post_json(
        &app,
        &format!("/api/internal/operator/realms/requests/{realm_request_id}/approve"),
        &approver_id,
        json!({
            "target_realm_status": "active",
            "review_reason_code": "active_after_review",
            "sponsor_quota_total": 1,
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
        admission_body.clone(),
    )
    .await;
    assert_eq!(replayed_admission.status, StatusCode::OK);
    assert_eq!(
        replayed_admission.body["realm_admission_id"],
        realm_admission_id
    );

    set_account_state(&client, &member.account_id, "suspended").await;
    let inactive_account_replay = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        admission_body.clone(),
    )
    .await;
    assert_eq!(inactive_account_replay.status, StatusCode::OK);
    assert_eq!(
        inactive_account_replay.body["realm_admission_id"],
        realm_admission_id
    );

    grant_operator_role(&client, &approver_id, "steward").await;
    let role_changed_replay = operator_post_json(
        &app,
        &format!("/api/internal/realms/{realm_id}/admissions"),
        &approver_id,
        json!({
            "account_id": member.account_id,
            "source_fact_kind": "realm_admin_invite",
            "source_fact_id": "realm-idem-admission",
            "source_snapshot_json": {
                "safe_summary": "first admission"
            },
            "request_idempotency_key": "realm-idem-admission"
        }),
    )
    .await;
    assert_eq!(role_changed_replay.status, StatusCode::OK);
    assert_eq!(
        role_changed_replay.body["realm_admission_id"],
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

#[tokio::test]
async fn realm_bootstrap_idempotency_keys_reject_blank_values_at_db_layer() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-db-key-a", "realm-db-key-a").await;
    let sponsor = sign_in(&app, "pi-user-realm-db-key-b", "realm-db-key-b").await;
    let member = sign_in(&app, "pi-user-realm-db-key-c", "realm-db-key-c").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;

    let requester_id = Uuid::parse_str(&requester.account_id).expect("requester id must be uuid");
    let sponsor_id = Uuid::parse_str(&sponsor.account_id).expect("sponsor id must be uuid");
    let member_id = Uuid::parse_str(&member.account_id).expect("member id must be uuid");
    let approver_uuid = Uuid::parse_str(&approver_id).expect("approver id must be uuid");
    let blank_request_id = Uuid::new_v4();
    let blank_review_request_id = Uuid::new_v4();
    let blank_sponsor_record_id = Uuid::new_v4();
    let blank_admission_id = Uuid::new_v4();
    let valid_admission_id = Uuid::new_v4();

    assert_check_violation(
        client
            .execute(
                "
                INSERT INTO dao.realm_requests (
                    realm_request_id,
                    requested_by_account_id,
                    display_name,
                    slug_candidate,
                    purpose_text,
                    venue_context_json,
                    expected_member_shape_json,
                    bootstrap_rationale_text,
                    request_state,
                    review_reason_code,
                    request_idempotency_key,
                    request_payload_hash
                )
                VALUES (
                    $1,
                    $2,
                    'Blank key realm',
                    'blank-key-realm-request',
                    'A blank request key must fail.',
                    '{\"city\":\"Tokyo\"}'::jsonb,
                    '{\"size\":\"small\"}'::jsonb,
                    'The database owns the idempotency contract.',
                    'requested',
                    'request_received',
                    '   ',
                    repeat('0', 64)
                )
                ",
                &[&blank_request_id, &requester_id],
            )
            .await,
    );

    assert_check_violation(
        client
            .execute(
                "
                INSERT INTO dao.realm_requests (
                    realm_request_id,
                    requested_by_account_id,
                    display_name,
                    slug_candidate,
                    purpose_text,
                    venue_context_json,
                    expected_member_shape_json,
                    bootstrap_rationale_text,
                    request_state,
                    review_reason_code,
                    request_idempotency_key,
                    request_payload_hash,
                    reviewed_by_operator_id,
                    review_decision_idempotency_key,
                    review_decision_payload_hash,
                    reviewed_at
                )
                VALUES (
                    $1,
                    $2,
                    'Blank review key realm',
                    'blank-review-key-realm-request',
                    'A blank review key must fail.',
                    '{\"city\":\"Tokyo\"}'::jsonb,
                    '{\"size\":\"small\"}'::jsonb,
                    'The database owns review idempotency too.',
                    'approved',
                    'active_after_review',
                    'db-valid-request-key',
                    repeat('1', 64),
                    $3,
                    '   ',
                    repeat('2', 64),
                    CURRENT_TIMESTAMP
                )
                ",
                &[&blank_review_request_id, &requester_id, &approver_uuid],
            )
            .await,
    );

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        None,
        None,
        &approver_id,
        "active",
        "db-key-blank",
    )
    .await;

    assert_check_violation(
        client
            .execute(
                "
                INSERT INTO dao.realm_sponsor_records (
                    realm_sponsor_record_id,
                    realm_id,
                    sponsor_account_id,
                    sponsor_status,
                    quota_total,
                    status_reason_code,
                    approved_by_operator_id,
                    request_idempotency_key,
                    request_payload_hash
                )
                VALUES (
                    $1,
                    $2,
                    $3,
                    'approved',
                    1,
                    'active_after_review',
                    $4,
                    '   ',
                    repeat('3', 64)
                )
                ",
                &[
                    &blank_sponsor_record_id,
                    &realm_id,
                    &sponsor_id,
                    &approver_uuid,
                ],
            )
            .await,
    );

    assert_check_violation(
        client
            .execute(
                "
                INSERT INTO dao.realm_admissions (
                    realm_admission_id,
                    realm_id,
                    account_id,
                    admission_kind,
                    admission_status,
                    granted_by_actor_kind,
                    granted_by_actor_id,
                    review_reason_code,
                    source_fact_kind,
                    source_fact_id,
                    request_idempotency_key,
                    request_payload_hash
                )
                VALUES (
                    $1,
                    $2,
                    $3,
                    'normal',
                    'admitted',
                    'operator',
                    $4,
                    'active_after_review',
                    'db_test',
                    'blank-admission-key',
                    '   ',
                    repeat('4', 64)
                )
                ",
                &[&blank_admission_id, &realm_id, &member_id, &approver_uuid],
            )
            .await,
    );

    client
        .execute(
            "
            INSERT INTO dao.realm_admissions (
                realm_admission_id,
                realm_id,
                account_id,
                admission_kind,
                admission_status,
                granted_by_actor_kind,
                granted_by_actor_id,
                review_reason_code,
                source_fact_kind,
                source_fact_id,
                request_idempotency_key,
                request_payload_hash
            )
            VALUES (
                $1,
                $2,
                $3,
                'normal',
                'admitted',
                'operator',
                $4,
                'active_after_review',
                'db_test',
                'valid-admission-key-source',
                'valid-admission-key',
                repeat('5', 64)
            )
            ",
            &[&valid_admission_id, &realm_id, &member_id, &approver_uuid],
        )
        .await
        .expect("valid admission fixture");

    assert_check_violation(
        client
            .execute(
                "
                INSERT INTO dao.realm_admission_idempotency_keys (
                    realm_id,
                    granted_by_actor_id,
                    request_idempotency_key,
                    realm_admission_id,
                    request_payload_hash
                )
                VALUES (
                    $1,
                    $2,
                    '   ',
                    $3,
                    repeat('6', 64)
                )
                ",
                &[&realm_id, &approver_uuid, &valid_admission_id],
            )
            .await,
    );
}

#[tokio::test]
async fn realm_admission_kind_requires_matching_lineage_at_db_layer() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-user-realm-db-lineage-a", "realm-db-lineage-a").await;
    let sponsor = sign_in(&app, "pi-user-realm-db-lineage-b", "realm-db-lineage-b").await;
    let member = sign_in(&app, "pi-user-realm-db-lineage-c", "realm-db-lineage-c").await;
    let client = test_db_client().await;
    let approver_id = insert_operator_account(&client, "approver").await;
    let member_id = Uuid::parse_str(&member.account_id).expect("member id must be uuid");
    let approver_uuid = Uuid::parse_str(&approver_id).expect("approver id must be uuid");

    let (realm_id, _) = create_realm(
        &app,
        &requester,
        Some(&sponsor),
        None,
        &approver_id,
        "limited_bootstrap",
        "db-admission-lineage",
    )
    .await;
    let sponsor_record_id = Uuid::parse_str(&sponsor_record_id_for_realm(&client, &realm_id).await)
        .expect("sponsor record id must be uuid");
    let corridor_id: Uuid = client
        .query_one(
            "
            SELECT bootstrap_corridor_id
            FROM dao.bootstrap_corridors
            WHERE realm_id = $1
            LIMIT 1
            ",
            &[&realm_id],
        )
        .await
        .expect("bootstrap corridor must query")
        .get("bootstrap_corridor_id");

    assert_check_violation(
        client
            .execute(
                "
                INSERT INTO dao.realm_admissions (
                    realm_admission_id,
                    realm_id,
                    account_id,
                    admission_kind,
                    admission_status,
                    sponsor_record_id,
                    bootstrap_corridor_id,
                    granted_by_actor_kind,
                    granted_by_actor_id,
                    review_reason_code,
                    source_fact_kind,
                    source_fact_id,
                    request_idempotency_key,
                    request_payload_hash
                )
                VALUES (
                    $1,
                    $2,
                    $3,
                    'sponsor_backed',
                    'pending',
                    NULL,
                    $4,
                    'operator',
                    $5,
                    'limited_bootstrap_active',
                    'db_test',
                    'sponsor-backed-without-sponsor',
                    'sponsor-backed-without-sponsor',
                    repeat('5', 64)
                )
                ",
                &[
                    &Uuid::new_v4(),
                    &realm_id,
                    &member_id,
                    &corridor_id,
                    &approver_uuid,
                ],
            )
            .await,
    );

    assert_check_violation(
        client
            .execute(
                "
                INSERT INTO dao.realm_admissions (
                    realm_admission_id,
                    realm_id,
                    account_id,
                    admission_kind,
                    admission_status,
                    sponsor_record_id,
                    bootstrap_corridor_id,
                    granted_by_actor_kind,
                    granted_by_actor_id,
                    review_reason_code,
                    source_fact_kind,
                    source_fact_id,
                    request_idempotency_key,
                    request_payload_hash
                )
                VALUES (
                    $1,
                    $2,
                    $3,
                    'corridor',
                    'pending',
                    NULL,
                    NULL,
                    'operator',
                    $4,
                    'limited_bootstrap_active',
                    'db_test',
                    'corridor-without-corridor',
                    'corridor-without-corridor',
                    repeat('6', 64)
                )
                ",
                &[&Uuid::new_v4(), &realm_id, &member_id, &approver_uuid],
            )
            .await,
    );

    assert_check_violation(
        client
            .execute(
                "
                INSERT INTO dao.realm_admissions (
                    realm_admission_id,
                    realm_id,
                    account_id,
                    admission_kind,
                    admission_status,
                    sponsor_record_id,
                    bootstrap_corridor_id,
                    granted_by_actor_kind,
                    granted_by_actor_id,
                    review_reason_code,
                    source_fact_kind,
                    source_fact_id,
                    request_idempotency_key,
                    request_payload_hash
                )
                VALUES (
                    $1,
                    $2,
                    $3,
                    'corridor',
                    'pending',
                    $4,
                    $5,
                    'operator',
                    $6,
                    'limited_bootstrap_active',
                    'db_test',
                    'corridor-with-sponsor',
                    'corridor-with-sponsor',
                    repeat('7', 64)
                )
                ",
                &[
                    &Uuid::new_v4(),
                    &realm_id,
                    &member_id,
                    &sponsor_record_id,
                    &corridor_id,
                    &approver_uuid,
                ],
            )
            .await,
    );

    assert_check_violation(
        client
            .execute(
                "
                INSERT INTO dao.realm_admissions (
                    realm_admission_id,
                    realm_id,
                    account_id,
                    admission_kind,
                    admission_status,
                    sponsor_record_id,
                    bootstrap_corridor_id,
                    granted_by_actor_kind,
                    granted_by_actor_id,
                    review_reason_code,
                    source_fact_kind,
                    source_fact_id,
                    request_idempotency_key,
                    request_payload_hash
                )
                VALUES (
                    $1,
                    $2,
                    $3,
                    'normal',
                    'pending',
                    $4,
                    NULL,
                    'operator',
                    $5,
                    'active_after_review',
                    'db_test',
                    'normal-with-sponsor',
                    'normal-with-sponsor',
                    repeat('8', 64)
                )
                ",
                &[
                    &Uuid::new_v4(),
                    &realm_id,
                    &member_id,
                    &sponsor_record_id,
                    &approver_uuid,
                ],
            )
            .await,
    );
}

#[tokio::test]
async fn realm_request_review_states_require_review_audit_fields_at_db_layer() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(
        &app,
        "pi-user-realm-db-review-audit",
        "realm-db-review-audit",
    )
    .await;
    let client = test_db_client().await;

    let requester_id = Uuid::parse_str(&requester.account_id).expect("requester id must be uuid");
    let unaudited_review_request_id = Uuid::new_v4();

    assert_check_violation(
        client
            .execute(
                "
                INSERT INTO dao.realm_requests (
                    realm_request_id,
                    requested_by_account_id,
                    display_name,
                    slug_candidate,
                    purpose_text,
                    venue_context_json,
                    expected_member_shape_json,
                    bootstrap_rationale_text,
                    request_state,
                    review_reason_code,
                    request_idempotency_key,
                    request_payload_hash,
                    review_decision_idempotency_key,
                    review_decision_payload_hash
                )
                VALUES (
                    $1,
                    $2,
                    'Unaudited approved realm',
                    'unaudited-approved-realm',
                    'Approved and rejected rows require review actor/time.',
                    '{\"city\":\"Tokyo\"}'::jsonb,
                    '{\"size\":\"small\"}'::jsonb,
                    'The database owns review audit completeness.',
                    'approved',
                    'active_after_review',
                    'unaudited-approved-request',
                    repeat('5', 64),
                    'unaudited-approved-review',
                    repeat('6', 64)
                )
                ",
                &[&unaudited_review_request_id, &requester_id],
            )
            .await,
    );
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

fn assert_check_violation(result: Result<u64, tokio_postgres::Error>) {
    let error = result.expect_err("database check must reject invalid row");
    assert_eq!(error.code(), Some(&SqlState::CHECK_VIOLATION));
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

async fn admission_idempotency_key_count(
    client: &tokio_postgres::Client,
    realm_id: &str,
    request_idempotency_key: &str,
) -> i64 {
    client
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM dao.realm_admission_idempotency_keys
            WHERE realm_id = $1
              AND request_idempotency_key = $2
            ",
            &[&realm_id, &request_idempotency_key],
        )
        .await
        .expect("admission idempotency key count must query")
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

async fn set_account_state(client: &tokio_postgres::Client, account_id: &str, account_state: &str) {
    let account_id = Uuid::parse_str(account_id).expect("account id must be uuid");
    client
        .execute(
            "
            UPDATE core.accounts
            SET account_state = $2
            WHERE account_id = $1
            ",
            &[&account_id, &account_state],
        )
        .await
        .expect("account state must update");
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

async fn current_corridor_status(client: &tokio_postgres::Client, realm_id: &str) -> String {
    client
        .query_one(
            "
            SELECT corridor_status
            FROM dao.bootstrap_corridors
            WHERE realm_id = $1
            ORDER BY updated_at DESC, bootstrap_corridor_id DESC
            LIMIT 1
            ",
            &[&realm_id],
        )
        .await
        .expect("corridor row must exist")
        .get("corridor_status")
}

async fn append_admission_status_for_test(
    client: &tokio_postgres::Client,
    realm_id: &str,
    account_id: &str,
    operator_id: &str,
    admission_status: &str,
    review_reason_code: &str,
    request_idempotency_key: &str,
) {
    let account_uuid = Uuid::parse_str(account_id).expect("account id must be a uuid");
    let operator_uuid = Uuid::parse_str(operator_id).expect("operator id must be a uuid");
    client
        .execute(
            "
            INSERT INTO dao.realm_admissions (
                realm_admission_id,
                realm_id,
                account_id,
                admission_kind,
                admission_status,
                granted_by_actor_kind,
                granted_by_actor_id,
                review_reason_code,
                source_fact_kind,
                source_fact_id,
                source_snapshot_json,
                request_idempotency_key,
                request_payload_hash,
                created_at,
                updated_at
            )
            VALUES (
                $1, $2, $3, 'normal', $4, 'operator', $5, $6,
                'realm_admin_invite', $7, '{}'::jsonb, $8, repeat('0', 64),
                CURRENT_TIMESTAMP + interval '1 minute',
                CURRENT_TIMESTAMP + interval '1 minute'
            )
            ",
            &[
                &Uuid::new_v4(),
                &realm_id,
                &account_uuid,
                &admission_status,
                &operator_uuid,
                &review_reason_code,
                &request_idempotency_key,
                &request_idempotency_key,
            ],
        )
        .await
        .expect("admission status row must insert");
}

async fn lock_sponsor_lineage_for_test(
    tx: &tokio_postgres::Transaction<'_>,
    realm_id: &str,
    sponsor_account_id: &Uuid,
) {
    let sponsor_account_id_text = sponsor_account_id.to_string();
    tx.query_one(
        "
        SELECT pg_advisory_xact_lock(
            hashtext('realm_bootstrap.sponsor_lineage'),
            hashtext($1 || ':' || $2::text)
        )
        ",
        &[&realm_id, &sponsor_account_id_text],
    )
    .await
    .expect("sponsor lineage lock must be acquired");
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

async fn current_bootstrap_last_projected_at(
    client: &tokio_postgres::Client,
    realm_id: &str,
) -> DateTime<Utc> {
    client
        .query_one(
            "
            SELECT last_projected_at
            FROM projection.realm_bootstrap_views
            WHERE realm_id = $1
            ",
            &[&realm_id],
        )
        .await
        .expect("bootstrap projection row must exist")
        .get("last_projected_at")
}

async fn current_review_summary_last_projected_at(
    client: &tokio_postgres::Client,
    realm_id: &str,
) -> DateTime<Utc> {
    client
        .query_one(
            "
            SELECT last_projected_at
            FROM projection.realm_review_summaries
            WHERE realm_id = $1
            ",
            &[&realm_id],
        )
        .await
        .expect("review summary projection row must exist")
        .get("last_projected_at")
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

async fn grant_operator_role(client: &tokio_postgres::Client, operator_id: &str, role: &str) {
    let operator_id = Uuid::parse_str(operator_id).expect("operator id must be a uuid");
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
            &[&Uuid::new_v4(), &operator_id, &role],
        )
        .await
        .expect("operator role assignment must insert");
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
