use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use musubi_backend::{
    build_app, new_test_state,
    services::launch_posture::{LaunchAction, LaunchPostureConfig},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn launch_posture_defaults_to_closed_or_pilot_safe_mode() {
    let config = LaunchPostureConfig::from_lookup(|_| None);
    let snapshot = config.public_snapshot();

    assert_eq!(snapshot.launch_mode, "closed");
    assert_eq!(snapshot.participant_posture, "closed");
    assert_eq!(snapshot.message_code, "launch_closed");
}

#[tokio::test]
async fn invalid_launch_mode_fails_closed() {
    let config = LaunchPostureConfig::from_lookup(|name| match name {
        "MUSUBI_LAUNCH_MODE" => Some("public".to_owned()),
        "MUSUBI_KILL_SWITCH_PROMISE_CREATION" => Some("maybe".to_owned()),
        _ => None,
    });
    let public = config.public_snapshot();
    let internal = config.internal_snapshot();

    assert_eq!(public.launch_mode, "closed");
    assert_eq!(public.message_code, "launch_closed");
    assert!(
        internal
            .config_warnings
            .contains(&"invalid_launch_mode".to_owned())
    );
    assert!(
        internal
            .config_warnings
            .contains(&"invalid_boolean_switch:MUSUBI_KILL_SWITCH_PROMISE_CREATION".to_owned())
    );
    assert!(internal.kill_switches.promise_creation);
}

#[tokio::test]
async fn open_preview_env_mode_fails_closed_and_blocks_participant_writes() {
    let config = LaunchPostureConfig::from_lookup(|name| match name {
        "MUSUBI_LAUNCH_MODE" => Some("open_preview".to_owned()),
        _ => None,
    });
    let public = config.public_snapshot();
    let internal = config.internal_snapshot();

    assert_eq!(public.launch_mode, "closed");
    assert_eq!(public.participant_posture, "closed");
    assert_eq!(public.message_code, "launch_closed");
    assert!(
        internal
            .config_warnings
            .contains(&"unsupported_launch_mode:open_preview".to_owned())
    );

    let test_state = new_test_state().await.expect("test database state");
    test_state.replace_launch_config_for_test(config).await;
    let app = build_app(test_state.state.clone());

    let response = post_json(
        &app,
        "/api/auth/pi",
        None,
        json!({
            "pi_uid": "pi-launch-open-preview-env",
            "username": "launch-open-preview-env",
            "wallet_address": "wallet-pi-launch-open-preview-env",
            "access_token": "access-token-pi-launch-open-preview-env"
        }),
    )
    .await;

    assert_eq!(response.status, StatusCode::FORBIDDEN);
    assert_eq!(response.body["message_code"], "launch_closed");
}

#[tokio::test]
async fn launch_public_posture_redacts_internal_allowlist() {
    let test_state = new_test_state().await.expect("test database state");
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::from_lookup(|name| match name {
            "MUSUBI_LAUNCH_MODE" => Some("pilot".to_owned()),
            "MUSUBI_LAUNCH_ALLOWLIST_PI_UIDS" => Some("pi-private-a,pi-private-b".to_owned()),
            "MUSUBI_LAUNCH_ALLOWLIST_ACCOUNT_IDS" => Some(Uuid::new_v4().to_string()),
            "MUSUBI_LAUNCH_SUPPORT_CONTACT_LABEL" => Some("お問い合わせ".to_owned()),
            "MUSUBI_LAUNCH_SUPPORT_CONTACT_URL" => {
                Some("https://example.invalid/support".to_owned())
            }
            _ => None,
        }))
        .await;
    let app = build_app(test_state.state.clone());

    let response = get_json(&app, "/api/launch/posture", None).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["launch_mode"], "pilot");
    assert_eq!(response.body["participant_posture"], "pilot_only");
    assert_eq!(response.body["message_code"], "launch_pilot_not_allowed");
    assert_eq!(response.body["support_contact"]["label"], "お問い合わせ");
    let body = response.body.to_string();
    assert!(!body.contains("pi-private-a"));
    assert!(!body.contains("pi-private-b"));
    assert!(!body.contains("allowlist"));
}

#[tokio::test]
async fn launch_internal_posture_requires_internal_gate() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let participant = sign_in(&app, "pi-launch-internal-gate", "launch-internal-gate").await;

    let response = get_json(
        &app,
        "/api/internal/launch/posture",
        Some(participant.token.as_str()),
    )
    .await;

    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.body["error"],
        "internal authorization bearer token is required"
    );
}

#[tokio::test]
async fn launch_internal_posture_reports_allowlist_counts_not_members() {
    let test_state = new_test_state().await.expect("test database state");
    let account_id = Uuid::new_v4().to_string();
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::from_lookup(|name| match name {
            "MUSUBI_LAUNCH_MODE" => Some("pilot".to_owned()),
            "MUSUBI_LAUNCH_ALLOWLIST_PI_UIDS" => Some("pi-count-a, pi-count-b".to_owned()),
            "MUSUBI_LAUNCH_ALLOWLIST_ACCOUNT_IDS" => Some(account_id.clone()),
            _ => None,
        }))
        .await;
    let app = build_app(test_state.state.clone());

    let response = get_json(&app, "/api/internal/launch/posture", None).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["allowlist"]["source"], "env");
    assert_eq!(response.body["allowlist"]["pi_uid_count"], 2);
    assert_eq!(response.body["allowlist"]["account_id_count"], 1);
    assert_eq!(response.body["allowlist"]["members_visible"], false);
    let body = response.body.to_string();
    assert!(!body.contains("pi-count-a"));
    assert!(!body.contains(&account_id));
}

#[tokio::test]
async fn closed_launch_blocks_non_allowlisted_pi_auth() {
    let test_state = new_test_state().await.expect("test database state");
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::closed_for_test())
        .await;
    let app = build_app(test_state.state.clone());

    let response = post_json(
        &app,
        "/api/auth/pi",
        None,
        json!({
            "pi_uid": "pi-launch-closed",
            "username": "launch-closed",
            "wallet_address": "wallet-pi-launch-closed",
            "access_token": "access-token-pi-launch-closed"
        }),
    )
    .await;

    assert_eq!(response.status, StatusCode::FORBIDDEN);
    assert_eq!(response.body["error"], "launch_closed");
    assert_eq!(response.body["message_code"], "launch_closed");
}

#[tokio::test]
async fn pilot_launch_allows_allowlisted_pi_auth() {
    let test_state = new_test_state().await.expect("test database state");
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::pilot_for_test(
            &["pi-launch-pilot-allowed"],
            &[],
        ))
        .await;
    let app = build_app(test_state.state.clone());

    let response = post_json(
        &app,
        "/api/auth/pi",
        None,
        json!({
            "pi_uid": "pi-launch-pilot-allowed",
            "username": "launch-pilot-allowed",
            "wallet_address": "wallet-pi-launch-pilot-allowed",
            "access_token": "access-token-pi-launch-pilot-allowed"
        }),
    )
    .await;

    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body["token"].as_str().is_some());
}

#[tokio::test]
async fn pilot_launch_allows_account_allowlisted_pi_auth_for_existing_account() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let existing = sign_in(
        &app,
        "pi-launch-pilot-account-allowed",
        "launch-pilot-account-allowed",
    )
    .await;
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::pilot_for_test(
            &[],
            &[existing.account_id.as_str()],
        ))
        .await;

    let reauthenticated = sign_in(
        &app,
        "pi-launch-pilot-account-allowed",
        "launch-pilot-account-allowed-returning",
    )
    .await;

    assert_eq!(reauthenticated.account_id, existing.account_id);
}

#[tokio::test]
async fn pilot_launch_pi_allowlisted_account_can_use_participant_write_after_auth() {
    let test_state = new_test_state().await.expect("test database state");
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::pilot_for_test(
            &["pi-launch-pilot-write"],
            &[],
        ))
        .await;
    let app = build_app(test_state.state.clone());
    let participant = sign_in(&app, "pi-launch-pilot-write", "launch-pilot-write").await;

    let response = post_json(
        &app,
        "/api/realms/requests",
        Some(participant.token.as_str()),
        realm_request_body("pilot-pi-allowlisted-write"),
    )
    .await;

    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["request_state"], "requested");
    assert!(response.body["realm_request_id"].as_str().is_some());
    assert!(response.body.get("requested_by_account_id").is_none());
}

#[tokio::test]
async fn paused_launch_blocks_promise_creation() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let initiator = sign_in(&app, "pi-launch-promise-a", "launch-promise-a").await;
    let counterparty = sign_in(&app, "pi-launch-promise-b", "launch-promise-b").await;
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::paused_for_test())
        .await;

    let response = post_json(
        &app,
        "/api/promise/intents",
        Some(initiator.token.as_str()),
        json!({
            "internal_idempotency_key": "launch-promise-paused",
            "realm_id": "realm-launch-promise",
            "counterparty_account_id": counterparty.account_id,
            "deposit_amount_minor_units": 1000,
            "currency_code": "JPY"
        }),
    )
    .await;

    assert_eq!(
        response.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        response.body
    );
    assert_eq!(response.body["message_code"], "launch_paused");
}

#[tokio::test]
async fn paused_launch_blocks_proof_submission() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let subject = sign_in(&app, "pi-launch-proof", "launch-proof").await;
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::paused_for_test())
        .await;

    let response = post_json(
        &app,
        "/api/proof/submissions",
        Some(subject.token.as_str()),
        json!({
            "challenge_id": null,
            "venue_id": "venue-launch-proof",
            "display_code": "123456"
        }),
    )
    .await;

    assert_eq!(
        response.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        response.body
    );
    assert_eq!(response.body["message_code"], "launch_paused");
}

#[tokio::test]
async fn paused_launch_blocks_realm_request() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let requester = sign_in(&app, "pi-launch-realm", "launch-realm").await;
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::paused_for_test())
        .await;

    let response = post_json(
        &app,
        "/api/realms/requests",
        Some(requester.token.as_str()),
        realm_request_body("launch-realm-paused"),
    )
    .await;

    assert_eq!(
        response.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        response.body
    );
    assert_eq!(response.body["message_code"], "launch_paused");
}

#[tokio::test]
async fn paused_launch_blocks_appeal_creation() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let appellant = sign_in(&app, "pi-launch-appeal", "launch-appeal").await;
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::paused_for_test())
        .await;

    let response = post_json(
        &app,
        &format!("/api/review-cases/{}/appeals", Uuid::new_v4()),
        Some(appellant.token.as_str()),
        appeal_body("paused"),
    )
    .await;

    assert_eq!(
        response.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        response.body
    );
    assert_eq!(response.body["message_code"], "launch_paused");
}

#[tokio::test]
async fn appeal_creation_kill_switch_blocks_new_appeals() {
    let test_state = new_test_state().await.expect("test database state");
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::with_kill_switch_for_test(
            LaunchAction::AppealCreation,
        ))
        .await;
    let app = build_app(test_state.state.clone());
    let appellant = sign_in(&app, "pi-launch-appeal-switch", "launch-appeal-switch").await;

    let response = post_json(
        &app,
        &format!("/api/review-cases/{}/appeals", Uuid::new_v4()),
        Some(appellant.token.as_str()),
        appeal_body("kill-switch"),
    )
    .await;

    assert_eq!(
        response.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        response.body
    );
    assert_eq!(response.body["message_code"], "appeal_creation_paused");
}

#[tokio::test]
async fn realm_admission_kill_switch_blocks_new_admissions() {
    let test_state = new_test_state().await.expect("test database state");
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::with_kill_switch_for_test(
            LaunchAction::RealmAdmission,
        ))
        .await;
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let operator_id = insert_operator_account(&client, "approver").await;

    let response = request_json(
        &app,
        "POST",
        &format!("/api/internal/realms/{}/admissions", Uuid::new_v4()),
        Some("local_dev_internal_api_token"),
        Some(operator_id.as_str()),
        Some(realm_admission_body(
            &Uuid::new_v4().to_string(),
            "kill-switch",
        )),
    )
    .await;

    assert_eq!(
        response.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        response.body
    );
    assert_eq!(response.body["message_code"], "realm_admission_paused");
}

#[tokio::test]
async fn paused_launch_blocks_internal_realm_admission() {
    let test_state = new_test_state().await.expect("test database state");
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::paused_for_test())
        .await;
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let operator_id = insert_operator_account(&client, "approver").await;

    let response = request_json(
        &app,
        "POST",
        &format!("/api/internal/realms/{}/admissions", Uuid::new_v4()),
        Some("local_dev_internal_api_token"),
        Some(operator_id.as_str()),
        Some(realm_admission_body(&Uuid::new_v4().to_string(), "paused")),
    )
    .await;

    assert_eq!(response.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.body["message_code"], "launch_paused");
}

#[tokio::test]
async fn closed_launch_blocks_internal_realm_admission_for_non_allowlisted_account() {
    let test_state = new_test_state().await.expect("test database state");
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::closed_for_test())
        .await;
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let operator_id = insert_operator_account(&client, "approver").await;

    let response = request_json(
        &app,
        "POST",
        &format!("/api/internal/realms/{}/admissions", Uuid::new_v4()),
        Some("local_dev_internal_api_token"),
        Some(operator_id.as_str()),
        Some(realm_admission_body(&Uuid::new_v4().to_string(), "closed")),
    )
    .await;

    assert_eq!(response.status, StatusCode::FORBIDDEN);
    assert_eq!(response.body["message_code"], "launch_closed");
}

#[tokio::test]
async fn pilot_launch_blocks_internal_realm_admission_for_non_allowlisted_account() {
    let test_state = new_test_state().await.expect("test database state");
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::pilot_for_test(&[], &[]))
        .await;
    let app = build_app(test_state.state.clone());
    let client = test_db_client().await;
    let operator_id = insert_operator_account(&client, "approver").await;

    let response = request_json(
        &app,
        "POST",
        &format!("/api/internal/realms/{}/admissions", Uuid::new_v4()),
        Some("local_dev_internal_api_token"),
        Some(operator_id.as_str()),
        Some(realm_admission_body(
            &Uuid::new_v4().to_string(),
            "pilot-blocked",
        )),
    )
    .await;

    assert_eq!(response.status, StatusCode::FORBIDDEN);
    assert_eq!(response.body["message_code"], "launch_pilot_not_allowed");
}

#[tokio::test]
async fn pilot_launch_allows_allowlisted_internal_realm_admission_to_reach_realm_checks() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let member = sign_in(
        &app,
        "pi-launch-realm-admission-allowed",
        "launch-realm-admission-allowed",
    )
    .await;
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::pilot_for_test(
            &[],
            &[member.account_id.as_str()],
        ))
        .await;
    let client = test_db_client().await;
    let operator_id = insert_operator_account(&client, "approver").await;

    let response = request_json(
        &app,
        "POST",
        &format!("/api/internal/realms/{}/admissions", Uuid::new_v4()),
        Some("local_dev_internal_api_token"),
        Some(operator_id.as_str()),
        Some(realm_admission_body(
            &member.account_id,
            "pilot-allowed-next-layer",
        )),
    )
    .await;

    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert_eq!(response.body["error"], "realm was not found");
    assert!(response.body.get("message_code").is_none());
}

#[tokio::test]
async fn internal_realm_admission_authorizes_operator_before_pilot_allowlist_check() {
    let test_state = new_test_state().await.expect("test database state");
    let app = build_app(test_state.state.clone());
    let member = sign_in(
        &app,
        "pi-launch-realm-admission-oracle",
        "launch-realm-admission-oracle",
    )
    .await;
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::pilot_for_test(
            &[],
            &[member.account_id.as_str()],
        ))
        .await;
    let non_operator_id = Uuid::new_v4().to_string();

    let allowlisted_response = request_json(
        &app,
        "POST",
        &format!("/api/internal/realms/{}/admissions", Uuid::new_v4()),
        Some("local_dev_internal_api_token"),
        Some(non_operator_id.as_str()),
        Some(realm_admission_body(
            &member.account_id,
            "operator-before-launch-allowlisted",
        )),
    )
    .await;
    let non_allowlisted_response = request_json(
        &app,
        "POST",
        &format!("/api/internal/realms/{}/admissions", Uuid::new_v4()),
        Some("local_dev_internal_api_token"),
        Some(non_operator_id.as_str()),
        Some(realm_admission_body(
            &Uuid::new_v4().to_string(),
            "operator-before-launch-blocked",
        )),
    )
    .await;

    assert_eq!(allowlisted_response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(non_allowlisted_response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        allowlisted_response.body["error"],
        "operator role is not allowed for realm bootstrap actions"
    );
    assert_eq!(allowlisted_response.body, non_allowlisted_response.body);
    assert!(allowlisted_response.body.get("message_code").is_none());
}

#[tokio::test]
async fn internal_ops_health_still_available_when_launch_paused() {
    let test_state = new_test_state().await.expect("test database state");
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::paused_for_test())
        .await;
    let app = build_app(test_state.state.clone());

    let response = get_json(&app, "/api/internal/ops/health", None).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["status"], "ok");
}

#[tokio::test]
async fn observability_status_does_not_override_launch_mode() {
    let test_state = new_test_state().await.expect("test database state");
    test_state
        .replace_launch_config_for_test(LaunchPostureConfig::paused_for_test())
        .await;
    let app = build_app(test_state.state.clone());

    let ops = get_json(&app, "/api/internal/ops/observability/snapshot", None).await;
    let launch = get_json(&app, "/api/launch/posture", None).await;

    assert_eq!(ops.status, StatusCode::OK);
    assert_eq!(launch.status, StatusCode::OK);
    assert_eq!(launch.body["launch_mode"], "paused");
    assert_eq!(launch.body["participant_posture"], "paused");
    assert_eq!(launch.body["message_code"], "launch_paused");
}

fn realm_request_body(suffix: &str) -> Value {
    json!({
        "display_name": format!("Launch Realm {suffix}"),
        "slug_candidate": format!("launch-realm-{suffix}"),
        "purpose_text": "Day 1 launch posture test realm",
        "venue_context_json": {
            "kind": "test_venue",
            "locality": "Tokyo"
        },
        "expected_member_shape_json": {
            "kind": "bounded_test_group"
        },
        "bootstrap_rationale_text": "Validate launch posture gating",
        "proposed_sponsor_account_id": null,
        "proposed_steward_account_id": null,
        "request_idempotency_key": format!("launch-realm-request-{suffix}")
    })
}

fn realm_admission_body(account_id: &str, suffix: &str) -> Value {
    json!({
        "account_id": account_id,
        "sponsor_record_id": null,
        "source_fact_kind": "launch_posture_test",
        "source_fact_id": format!("launch-admission-{suffix}"),
        "source_snapshot_json": {},
        "request_idempotency_key": format!("launch-admission-{suffix}")
    })
}

fn appeal_body(suffix: &str) -> Value {
    json!({
        "source_decision_fact_id": null,
        "submitted_reason_code": "appeal_received",
        "appellant_statement": format!("Launch posture appeal {suffix}"),
        "new_evidence_summary_json": {
            "safe_summary": format!("appeal {suffix}")
        },
        "appeal_idempotency_key": format!("launch-appeal-{suffix}")
    })
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
            VALUES ($1, $2, $3, 'launch posture test role')
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
