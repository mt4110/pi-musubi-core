use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use musubi_backend::{
    build_app, new_state,
    services::proof::{StartProofChallengeInput, start_proof_challenge},
};
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn public_proof_challenge_rejects_operator_pin_fallback() {
    let app = build_app(new_state());
    let user = sign_in(&app, "pi-user-proof-a", "proof-a").await;

    let response = post_json(
        &app,
        "/api/proof/challenges",
        Some(user.token.as_str()),
        json!({
            "venue_id": "venue-proof-a",
            "realm_id": "realm-proof",
            "fallback_mode": "operator_pin",
            "operator_id": "operator-proof-a"
        }),
    )
    .await;

    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        response.body["error"],
        "operator_pin fallback is not available from the public proof challenge endpoint"
    );
    assert!(response.body.get("operator_pin").is_none());
    assert!(response.body.get("operator_delivery").is_none());
}

#[tokio::test]
async fn public_operator_pin_rejections_do_not_consume_operator_budget() {
    let state = new_state();
    let app = build_app(state.clone());
    let user = sign_in(&app, "pi-user-proof-budget", "proof-budget").await;

    for _ in 0..3 {
        let response = post_json(
            &app,
            "/api/proof/challenges",
            Some(user.token.as_str()),
            json!({
                "venue_id": "venue-proof-budget",
                "realm_id": "realm-proof",
                "fallback_mode": "operator_pin",
                "operator_id": "operator-budget-a"
            }),
        )
        .await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST);
    }

    for index in 0..3 {
        let internal = start_proof_challenge(
            &state,
            StartProofChallengeInput {
                subject_account_id: format!("internal-proof-subject-{index}"),
                venue_id: "venue-proof-budget".to_owned(),
                realm_id: "realm-proof".to_owned(),
                fallback_mode: "operator_pin".to_owned(),
                operator_id: Some("operator-budget-a".to_owned()),
            },
        )
        .await
        .expect("public rejections must not burn internal operator fallback budget");
        assert!(internal.client.operator_pin_issued);
        assert!(internal.operator_delivery.is_some());
    }
}

#[tokio::test]
async fn public_challenge_ignores_operator_id_when_normal_flow_is_requested() {
    let app = build_app(new_state());
    let user = sign_in(&app, "pi-user-proof-normal", "proof-normal").await;

    let response = post_json(
        &app,
        "/api/proof/challenges",
        Some(user.token.as_str()),
        json!({
            "venue_id": "venue-proof-normal",
            "realm_id": "realm-proof",
            "fallback_mode": "none",
            "operator_id": "operator-should-not-be-principal"
        }),
    )
    .await;

    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body["challenge_id"].is_string());
    assert!(response.body["client_nonce"].is_string());
    assert_eq!(response.body["allowed_fallback_mode"], "none");
    assert_eq!(response.body["operator_pin_issued"], false);
    assert!(response.body.get("operator_pin").is_none());
    assert!(response.body.get("operator_delivery").is_none());
}

#[tokio::test]
async fn invalid_proof_envelope_is_recorded_as_rejected_evidence() {
    let app = build_app(new_state());
    let user = sign_in(&app, "pi-user-proof-b", "proof-b").await;

    let challenge = post_json(
        &app,
        "/api/proof/challenges",
        Some(user.token.as_str()),
        json!({
            "venue_id": "venue-proof-b",
            "realm_id": "realm-proof",
            "fallback_mode": "none"
        }),
    )
    .await;
    assert_eq!(challenge.status, StatusCode::OK);

    let submission = post_json(
        &app,
        "/api/proof/submissions",
        Some(user.token.as_str()),
        json!({
            "challenge_id": challenge.body["challenge_id"],
            "venue_id": "venue-proof-b",
            "display_code": "BAD123",
            "key_version": challenge.body["venue_key_version"],
            "client_nonce": challenge.body["client_nonce"],
            "observed_at_ms": chrono::Utc::now().timestamp_millis(),
            "coarse_location_bucket": "tokyo-shibuya",
            "device_session_id": "ephemeral-device-session",
            "fallback_mode": "none"
        }),
    )
    .await;

    assert_eq!(submission.status, StatusCode::OK);
    assert_eq!(submission.body["accepted"], false);
    assert_eq!(submission.body["verification_status"], "rejected");
    assert_eq!(submission.body["reason_code"], "invalid_code");
    assert!(submission.body["proof_submission_id"].is_string());
    assert!(submission.body["proof_verification_id"].is_string());
}

struct SignedInUser {
    token: String,
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
    }
}

struct JsonResponse {
    status: StatusCode,
    body: Value,
}

async fn post_json(
    app: &Router,
    path: &str,
    bearer_token: Option<&str>,
    body: Value,
) -> JsonResponse {
    let mut builder = Request::builder().method("POST").uri(path);
    if let Some(token) = bearer_token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }

    let request = builder
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
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
