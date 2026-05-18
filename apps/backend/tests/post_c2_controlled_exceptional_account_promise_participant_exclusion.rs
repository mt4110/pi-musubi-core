use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use musubi_backend::{build_app, new_test_state};
use serde_json::{Value, json};
use tokio_postgres::NoTls;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn controlled_exceptional_account_cannot_create_promise_as_initiating_participant() {
    let (_test_state, app, client) = test_context().await;
    let controlled = sign_in(
        &app,
        "pi-user-post-c2-promise-participant-controlled-initiator",
        "post-c2-controlled-initiator",
    )
    .await;
    let ordinary = sign_in(
        &app,
        "pi-user-post-c2-promise-participant-ordinary-counterparty",
        "post-c2-ordinary-counterparty",
    )
    .await;
    let controlled_account_id =
        Uuid::parse_str(&controlled.account_id).expect("controlled account id must be a UUID");

    set_account_class(
        &client,
        &controlled_account_id,
        "Controlled Exceptional Account",
    )
    .await;
    assert_account_class_and_state(
        &client,
        &controlled_account_id,
        "Controlled Exceptional Account",
        "active",
    )
    .await;

    let before = boundary_counts(&client).await;
    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(controlled.token.as_str()),
        json!({
            "internal_idempotency_key": "post-c2-controlled-initiator-promise",
            "realm_id": "realm-post-c2-controlled-initiator-promise",
            "counterparty_account_id": ordinary.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;

    assert_fail_closed(
        &create_promise,
        "initiator account must be an Ordinary Account",
    );
    assert_eq!(boundary_counts(&client).await, before);
    assert_account_class_and_state(
        &client,
        &controlled_account_id,
        "Controlled Exceptional Account",
        "active",
    )
    .await;
    assert_public_trust_projection_not_visible(
        &app,
        &controlled,
        "realm-post-c2-controlled-initiator-promise",
    )
    .await;
    assert_no_score_display_or_relationship_depth_columns(&client).await;
}

#[tokio::test]
async fn controlled_exceptional_account_cannot_be_promise_counterparty_participant() {
    let (_test_state, app, client) = test_context().await;
    let ordinary = sign_in(
        &app,
        "pi-user-post-c2-promise-participant-ordinary-initiator",
        "post-c2-ordinary-initiator",
    )
    .await;
    let controlled = sign_in(
        &app,
        "pi-user-post-c2-promise-participant-controlled-counterparty",
        "post-c2-controlled-counterparty",
    )
    .await;
    let controlled_account_id =
        Uuid::parse_str(&controlled.account_id).expect("controlled account id must be a UUID");

    set_account_class(
        &client,
        &controlled_account_id,
        "Controlled Exceptional Account",
    )
    .await;
    assert_account_class_and_state(
        &client,
        &controlled_account_id,
        "Controlled Exceptional Account",
        "active",
    )
    .await;

    let before = boundary_counts(&client).await;
    let create_promise = post_json(
        &app,
        "/api/promise/intents",
        Some(ordinary.token.as_str()),
        json!({
            "internal_idempotency_key": "post-c2-controlled-counterparty-promise",
            "realm_id": "realm-post-c2-controlled-counterparty-promise",
            "counterparty_account_id": controlled.account_id,
            "deposit_amount_minor_units": 10000,
            "currency_code": "PI"
        }),
    )
    .await;

    assert_fail_closed(
        &create_promise,
        "counterparty account must be an Ordinary Account",
    );
    assert_eq!(boundary_counts(&client).await, before);
    assert_account_class_and_state(
        &client,
        &controlled_account_id,
        "Controlled Exceptional Account",
        "active",
    )
    .await;
    assert_public_trust_projection_not_visible(
        &app,
        &controlled,
        "realm-post-c2-controlled-counterparty-promise",
    )
    .await;
    assert_no_score_display_or_relationship_depth_columns(&client).await;
}

async fn test_context() -> (musubi_backend::TestState, Router, tokio_postgres::Client) {
    let test_state = new_test_state()
        .await
        .expect("test database state should initialize");
    let app = build_app(test_state.state.clone());
    let database_url = std::env::var("MUSUBI_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("integration tests require MUSUBI_TEST_DATABASE_URL or DATABASE_URL to be set");
    let (client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to test database");
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("test database connection error: {error}");
        }
    });

    (test_state, app, client)
}

async fn set_account_class(
    client: &tokio_postgres::Client,
    account_id: &Uuid,
    account_class: &str,
) {
    client
        .execute(
            "
            UPDATE core.accounts
            SET account_class = $2
            WHERE account_id = $1
            ",
            &[account_id, &account_class],
        )
        .await
        .expect("account class fixture should update");
}

async fn assert_account_class_and_state(
    client: &tokio_postgres::Client,
    account_id: &Uuid,
    expected_class: &str,
    expected_state: &str,
) {
    let row = client
        .query_one(
            "
            SELECT account_class, account_state
            FROM core.accounts
            WHERE account_id = $1
            ",
            &[account_id],
        )
        .await
        .expect("account classification should load");

    assert_eq!(row.get::<_, String>("account_class"), expected_class);
    assert_eq!(row.get::<_, String>("account_state"), expected_state);
}

#[derive(Debug, PartialEq, Eq)]
struct BoundaryCounts {
    promise_intents: i64,
    promise_intent_idempotency_keys: i64,
    settlement_cases: i64,
    settlement_intents: i64,
    settlement_submissions: i64,
    provider_attempts: i64,
    settlement_observations: i64,
    journal_entries: i64,
    account_postings: i64,
    promise_views: i64,
    settlement_views: i64,
    trust_snapshots: i64,
    realm_trust_snapshots: i64,
    room_progression_tracks: i64,
    room_progression_facts: i64,
    room_progression_views: i64,
    proposed_mutation_attempts: i64,
    intake_decisions: i64,
    categorical_source_references: i64,
    categorical_mutation_facts: i64,
    outbox_events: i64,
    outbox_attempts: i64,
    command_inbox: i64,
    recovery_runs: i64,
    outbox_event_archive: i64,
    outbox_attempt_archive: i64,
    command_inbox_archive: i64,
    review_cases: i64,
    evidence_bundles: i64,
    evidence_access_grants: i64,
    operator_decision_facts: i64,
    appeal_cases: i64,
    review_status_views: i64,
    realm_requests: i64,
    realms: i64,
    realm_sponsor_records: i64,
    bootstrap_corridors: i64,
    realm_admissions: i64,
    realm_admission_idempotency_keys: i64,
    realm_review_triggers: i64,
    realm_bootstrap_views: i64,
    realm_admission_views: i64,
    realm_review_summaries: i64,
}

async fn boundary_counts(client: &tokio_postgres::Client) -> BoundaryCounts {
    let row = client
        .query_one(
            "
            SELECT
                (SELECT COUNT(*)::bigint FROM dao.promise_intents) AS promise_intents,
                (SELECT COUNT(*)::bigint FROM dao.promise_intent_idempotency_keys) AS promise_intent_idempotency_keys,
                (SELECT COUNT(*)::bigint FROM dao.settlement_cases) AS settlement_cases,
                (SELECT COUNT(*)::bigint FROM dao.settlement_intents) AS settlement_intents,
                (SELECT COUNT(*)::bigint FROM dao.settlement_submissions) AS settlement_submissions,
                (SELECT COUNT(*)::bigint FROM dao.provider_attempts) AS provider_attempts,
                (SELECT COUNT(*)::bigint FROM dao.settlement_observations) AS settlement_observations,
                (SELECT COUNT(*)::bigint FROM ledger.journal_entries) AS journal_entries,
                (SELECT COUNT(*)::bigint FROM ledger.account_postings) AS account_postings,
                (SELECT COUNT(*)::bigint FROM projection.promise_views) AS promise_views,
                (SELECT COUNT(*)::bigint FROM projection.settlement_views) AS settlement_views,
                (SELECT COUNT(*)::bigint FROM projection.trust_snapshots) AS trust_snapshots,
                (SELECT COUNT(*)::bigint FROM projection.realm_trust_snapshots) AS realm_trust_snapshots,
                (SELECT COUNT(*)::bigint FROM dao.room_progression_tracks) AS room_progression_tracks,
                (SELECT COUNT(*)::bigint FROM dao.room_progression_facts) AS room_progression_facts,
                (SELECT COUNT(*)::bigint FROM projection.room_progression_views) AS room_progression_views,
                (SELECT COUNT(*)::bigint FROM social_trust.proposed_mutation_attempts) AS proposed_mutation_attempts,
                (SELECT COUNT(*)::bigint FROM social_trust.intake_decisions) AS intake_decisions,
                (SELECT COUNT(*)::bigint FROM social_trust.categorical_source_references) AS categorical_source_references,
                (SELECT COUNT(*)::bigint FROM social_trust.categorical_mutation_facts) AS categorical_mutation_facts,
                (SELECT COUNT(*)::bigint FROM outbox.events) AS outbox_events,
                (SELECT COUNT(*)::bigint FROM outbox.outbox_attempts) AS outbox_attempts,
                (SELECT COUNT(*)::bigint FROM outbox.command_inbox) AS command_inbox,
                (SELECT COUNT(*)::bigint FROM outbox.recovery_runs) AS recovery_runs,
                (SELECT COUNT(*)::bigint FROM outbox.outbox_event_archive) AS outbox_event_archive,
                (SELECT COUNT(*)::bigint FROM outbox.outbox_attempt_archive) AS outbox_attempt_archive,
                (SELECT COUNT(*)::bigint FROM outbox.command_inbox_archive) AS command_inbox_archive,
                (SELECT COUNT(*)::bigint FROM dao.review_cases) AS review_cases,
                (SELECT COUNT(*)::bigint FROM dao.evidence_bundles) AS evidence_bundles,
                (SELECT COUNT(*)::bigint FROM dao.evidence_access_grants) AS evidence_access_grants,
                (SELECT COUNT(*)::bigint FROM dao.operator_decision_facts) AS operator_decision_facts,
                (SELECT COUNT(*)::bigint FROM dao.appeal_cases) AS appeal_cases,
                (SELECT COUNT(*)::bigint FROM projection.review_status_views) AS review_status_views,
                (SELECT COUNT(*)::bigint FROM dao.realm_requests) AS realm_requests,
                (SELECT COUNT(*)::bigint FROM dao.realms) AS realms,
                (SELECT COUNT(*)::bigint FROM dao.realm_sponsor_records) AS realm_sponsor_records,
                (SELECT COUNT(*)::bigint FROM dao.bootstrap_corridors) AS bootstrap_corridors,
                (SELECT COUNT(*)::bigint FROM dao.realm_admissions) AS realm_admissions,
                (SELECT COUNT(*)::bigint FROM dao.realm_admission_idempotency_keys) AS realm_admission_idempotency_keys,
                (SELECT COUNT(*)::bigint FROM dao.realm_review_triggers) AS realm_review_triggers,
                (SELECT COUNT(*)::bigint FROM projection.realm_bootstrap_views) AS realm_bootstrap_views,
                (SELECT COUNT(*)::bigint FROM projection.realm_admission_views) AS realm_admission_views,
                (SELECT COUNT(*)::bigint FROM projection.realm_review_summaries) AS realm_review_summaries
            ",
            &[],
        )
        .await
        .expect("boundary counts should load");

    BoundaryCounts {
        promise_intents: row.get("promise_intents"),
        promise_intent_idempotency_keys: row.get("promise_intent_idempotency_keys"),
        settlement_cases: row.get("settlement_cases"),
        settlement_intents: row.get("settlement_intents"),
        settlement_submissions: row.get("settlement_submissions"),
        provider_attempts: row.get("provider_attempts"),
        settlement_observations: row.get("settlement_observations"),
        journal_entries: row.get("journal_entries"),
        account_postings: row.get("account_postings"),
        promise_views: row.get("promise_views"),
        settlement_views: row.get("settlement_views"),
        trust_snapshots: row.get("trust_snapshots"),
        realm_trust_snapshots: row.get("realm_trust_snapshots"),
        room_progression_tracks: row.get("room_progression_tracks"),
        room_progression_facts: row.get("room_progression_facts"),
        room_progression_views: row.get("room_progression_views"),
        proposed_mutation_attempts: row.get("proposed_mutation_attempts"),
        intake_decisions: row.get("intake_decisions"),
        categorical_source_references: row.get("categorical_source_references"),
        categorical_mutation_facts: row.get("categorical_mutation_facts"),
        outbox_events: row.get("outbox_events"),
        outbox_attempts: row.get("outbox_attempts"),
        command_inbox: row.get("command_inbox"),
        recovery_runs: row.get("recovery_runs"),
        outbox_event_archive: row.get("outbox_event_archive"),
        outbox_attempt_archive: row.get("outbox_attempt_archive"),
        command_inbox_archive: row.get("command_inbox_archive"),
        review_cases: row.get("review_cases"),
        evidence_bundles: row.get("evidence_bundles"),
        evidence_access_grants: row.get("evidence_access_grants"),
        operator_decision_facts: row.get("operator_decision_facts"),
        appeal_cases: row.get("appeal_cases"),
        review_status_views: row.get("review_status_views"),
        realm_requests: row.get("realm_requests"),
        realms: row.get("realms"),
        realm_sponsor_records: row.get("realm_sponsor_records"),
        bootstrap_corridors: row.get("bootstrap_corridors"),
        realm_admissions: row.get("realm_admissions"),
        realm_admission_idempotency_keys: row.get("realm_admission_idempotency_keys"),
        realm_review_triggers: row.get("realm_review_triggers"),
        realm_bootstrap_views: row.get("realm_bootstrap_views"),
        realm_admission_views: row.get("realm_admission_views"),
        realm_review_summaries: row.get("realm_review_summaries"),
    }
}

fn assert_fail_closed(response: &JsonResponse, expected_error: &str) {
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert_eq!(response.body["error"], expected_error);
    assert_no_participant_authority_fields(&response.body);
}

async fn assert_public_trust_projection_not_visible(
    app: &Router,
    subject: &SignedInUser,
    realm_id: &str,
) {
    let global = get_json(
        app,
        &format!("/api/projection/trust-snapshots/{}", subject.account_id),
        Some(subject.token.as_str()),
    )
    .await;
    assert_ne!(global.status, StatusCode::OK);
    assert_no_participant_authority_fields(&global.body);

    let realm = get_json(
        app,
        &format!(
            "/api/projection/realm-trust-snapshots/{realm_id}/{}",
            subject.account_id
        ),
        Some(subject.token.as_str()),
    )
    .await;
    assert_ne!(realm.status, StatusCode::OK);
    assert_no_participant_authority_fields(&realm.body);
}

async fn assert_no_score_display_or_relationship_depth_columns(client: &tokio_postgres::Client) {
    let rows = client
        .query(
            "
            SELECT table_name, column_name
            FROM information_schema.columns
            WHERE table_schema = 'social_trust'
              AND table_name IN (
                  'categorical_source_references',
                  'categorical_mutation_facts'
              )
              AND (
                  column_name LIKE '%score%'
                  OR column_name LIKE '%weight%'
                  OR column_name LIKE '%rank%'
                  OR column_name LIKE '%display%'
                  OR column_name LIKE '%relationship_depth%'
                  OR column_name LIKE '%projection%'
                  OR column_name LIKE '%discovery%'
                  OR column_name LIKE '%recommendation%'
                  OR column_name LIKE '%ordinary_cohort%'
              )
            ",
            &[],
        )
        .await
        .expect("social trust column metadata should load");

    assert!(
        rows.is_empty(),
        "Controlled Exceptional Account Promise participant exclusion must not expose score/display/Relationship Depth/discovery/recommendation columns: {:?}",
        rows.iter()
            .map(|row| format!(
                "{}.{}",
                row.get::<_, String>("table_name"),
                row.get::<_, String>("column_name")
            ))
            .collect::<Vec<_>>()
    );
}

fn assert_no_participant_authority_fields(body: &Value) {
    for field in [
        "promise_intent_id",
        "settlement_case_id",
        "initiator_account_id",
        "counterparty_account_id",
        "participant_account_id",
        "participant_state",
        "current_intent_status",
        "case_status",
        "outbox_event_ids",
        "replayed_intent",
        "trust_posture",
        "reason_codes",
        "trust_score",
        "score",
        "rank",
        "trust_rank",
        "trust_tier",
        "display_level",
        "public_level",
        "recovery_ceiling",
        "discovery_priority",
        "recommendation_boost",
        "recommendation_input",
        "ordinary_cohort_evidence",
        "contact_unlock",
        "room_transition",
        "settlement_progression",
        "promise_runtime_outcome",
        "proof_runtime_outcome",
        "relationship_depth",
        "mobile_ui_state",
        "retention_action",
        "pruning_action",
        "archive_action",
        "deletion_action",
        "legal_hold_action",
        "key_lifecycle_action",
        "retry_action",
        "queue_action",
        "outbox_action",
        "inbox_action",
    ] {
        assert!(
            body.get(field).is_none(),
            "Controlled Exceptional Account Promise participant exclusion must not expose {field} in public API response"
        );
    }
}

struct SignedInUser {
    token: String,
    account_id: String,
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
    request_json(app, "POST", path, bearer_token, Some(body)).await
}

async fn get_json(app: &Router, path: &str, bearer_token: Option<&str>) -> JsonResponse {
    request_json(app, "GET", path, bearer_token, None).await
}

async fn request_json(
    app: &Router,
    method: &str,
    path: &str,
    bearer_token: Option<&str>,
    body: Option<Value>,
) -> JsonResponse {
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
    let body = if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&bytes).expect("response body must be valid json")
    };

    JsonResponse { status, body }
}
