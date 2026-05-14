use std::path::PathBuf;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use musubi_backend::{
    build_app, new_test_state,
    services::social_trust_mutation::{
        RecordC2BoundedPromiseReliabilityMutationFactInput, SocialTrustMutationPersistenceOutcome,
        SocialTrustMutationStore,
    },
};
use musubi_db_runtime::DbConfig;
use musubi_social_trust_domain::{
    AccountLifecycleReference, AgeAssuranceStateReference, AntiAbuseContinuityReference,
    AuditReference, BlockWithdrawalStateReference, C2BoundedPromiseReliabilityBoundaryIntersection,
    C2BoundedPromiseReliabilityBoundaryPosture, C2BoundedPromiseReliabilityFactIdempotencyKey,
    C2BoundedPromiseReliabilityMutationDecision, C2BoundedPromiseReliabilityMutationFact,
    C2BoundedPromiseReliabilityMutationFactCandidate, C2BoundedPromiseReliabilityRejection,
    C2BoundedPromiseReliabilitySourceFact, C2BoundedPromiseReliabilitySourceFactCandidate,
    ConsentStateReference, CriticalHarmCaseReference, EvidenceLevelReference, EvidencePosture,
    LegalHoldIntersectionReference, PromiseReference, PromiseTermsReference,
    ProposedC2BoundedPromiseReliabilityMutationFact, ReasonFactReference,
    RejectedC2BoundedPromiseReliabilitySourceFact, RetentionClassReference, RetentionPosture,
    ReviewabilityPosture, SafetyCaseReference, SocialTrustAuthorityPosture, WriterSourceReference,
};
use serde_json::{Value, json};
use tokio_postgres::NoTls;
use tower::ServiceExt;
use uuid::Uuid;

const REALM_REFERENCE: &str = "realm-reference-post-c2-rejection-boundary";

#[tokio::test]
async fn rejected_c2_sources_fail_before_persistence_without_side_effects() {
    let (_test_state, app, config, client) = test_context().await;
    let subject = sign_in(
        &app,
        "pi-user-post-c2-rejected-source",
        "post-c2-rejected-source",
    )
    .await;
    let subject_account_id = Uuid::parse_str(&subject.account_id).expect("account id is a UUID");
    let before_coordination = coordination_counts(&client).await;
    let store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("social trust mutation store should connect");

    for (index, rejected_source) in rejected_source_facts().into_iter().enumerate() {
        let idempotency_key = format!("post-c2-rejected-source-{}", rejected_source.as_str());
        let mut proposal = complete_proposal(
            &idempotency_key,
            C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
        );
        proposal.source_fact =
            C2BoundedPromiseReliabilitySourceFactCandidate::Rejected(rejected_source);

        assert_rejected_before_persistence(
            store
                .record_c2_bounded_promise_reliability_fact(record_input(
                    subject_account_id,
                    proposal,
                ))
                .await
                .unwrap_or_else(|error| {
                    panic!(
                        "rejected source case {index} ({}) should reject before persistence, got error: {error:?}",
                        rejected_source.as_str()
                    )
                }),
            C2BoundedPromiseReliabilityRejection::RejectedSourceFact {
                source: rejected_source,
            },
        );
        assert_no_writer_projection_or_coordination_side_effects(
            &client,
            &subject_account_id,
            &before_coordination,
        )
        .await;
    }

    assert_public_projection_not_visible(&app, &subject).await;
    assert_no_score_display_or_relationship_depth_columns(&client).await;
}

#[tokio::test]
async fn rejected_c2_boundary_and_required_input_cases_do_not_persist_or_expose() {
    let (_test_state, app, config, client) = test_context().await;
    let subject = sign_in(
        &app,
        "pi-user-post-c2-rejection-inputs",
        "post-c2-rejection-inputs",
    )
    .await;
    let subject_account_id = Uuid::parse_str(&subject.account_id).expect("account id is a UUID");
    let before_coordination = coordination_counts(&client).await;
    let store = SocialTrustMutationStore::connect(&config)
        .await
        .expect("social trust mutation store should connect");

    for case in rejection_input_cases() {
        let idempotency_key = format!("post-c2-rejection-input-{}", case.name);
        let mut proposal = complete_proposal(
            &idempotency_key,
            C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
        );
        (case.mutate)(&mut proposal);

        assert_rejected_before_persistence(
            store
                .record_c2_bounded_promise_reliability_fact(record_input(
                    subject_account_id,
                    proposal,
                ))
                .await
                .unwrap_or_else(|error| {
                    panic!(
                        "rejection input case {} should reject before persistence, got error: {error:?}",
                        case.name
                    )
                }),
            case.expected,
        );
        assert_no_writer_projection_or_coordination_side_effects(
            &client,
            &subject_account_id,
            &before_coordination,
        )
        .await;
    }

    assert_public_projection_not_visible(&app, &subject).await;
    assert_no_score_display_or_relationship_depth_columns(&client).await;
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

fn complete_proposal(
    idempotency_key: &str,
    source: C2BoundedPromiseReliabilitySourceFact,
    mutation: C2BoundedPromiseReliabilityMutationFact,
) -> ProposedC2BoundedPromiseReliabilityMutationFact {
    let boundary_posture =
        if source == C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection {
            C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(
                C2BoundedPromiseReliabilityBoundaryIntersection::LegalHold,
            )
        } else {
            C2BoundedPromiseReliabilityBoundaryPosture::Clear
        };

    ProposedC2BoundedPromiseReliabilityMutationFact {
        source_fact: C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(source),
        requested_mutation_fact: C2BoundedPromiseReliabilityMutationFactCandidate::Accepted(
            mutation,
        ),
        writer_source_reference: Some(WriterSourceReference::new(format!(
            "writer-source-{idempotency_key}"
        ))),
        promise_reference: Some(PromiseReference::new(format!("promise-{idempotency_key}"))),
        promise_terms_reference: Some(PromiseTermsReference::new(format!(
            "promise-terms-{idempotency_key}"
        ))),
        consent_at_formation_reference: Some(ConsentStateReference::new(format!(
            "consent-formation-{idempotency_key}"
        ))),
        consent_at_resolution_reference: Some(ConsentStateReference::new(format!(
            "consent-resolution-{idempotency_key}"
        ))),
        block_withdrawal_state_reference: Some(BlockWithdrawalStateReference::new(format!(
            "block-withdrawal-clear-{idempotency_key}"
        ))),
        age_assurance_state_reference: Some(AgeAssuranceStateReference::new(format!(
            "age-assurance-adult-eligible-{idempotency_key}"
        ))),
        legal_hold_intersection_reference: Some(LegalHoldIntersectionReference::new(format!(
            "legal-hold-clear-{idempotency_key}"
        ))),
        critical_harm_case_reference: Some(CriticalHarmCaseReference::new(format!(
            "critical-harm-clear-{idempotency_key}"
        ))),
        account_lifecycle_reference: Some(AccountLifecycleReference::new(format!(
            "account-lifecycle-active-{idempotency_key}"
        ))),
        anti_abuse_continuity_reference: Some(AntiAbuseContinuityReference::new(format!(
            "anti-abuse-clear-{idempotency_key}"
        ))),
        safety_case_reference: Some(SafetyCaseReference::new(format!(
            "safety-case-clear-{idempotency_key}"
        ))),
        evidence_level_reference: Some(EvidenceLevelReference::new(format!(
            "evidence-level-bounded-{idempotency_key}"
        ))),
        audit_reference: Some(AuditReference::new(format!("audit-{idempotency_key}"))),
        reason_fact: Some(ReasonFactReference::new(format!(
            "reason-fact-{idempotency_key}"
        ))),
        fact_idempotency_key: Some(C2BoundedPromiseReliabilityFactIdempotencyKey::new(
            idempotency_key,
        )),
        evidence_posture: Some(EvidencePosture::Bounded),
        reviewability_posture: Some(ReviewabilityPosture::Reviewable),
        retention_posture: Some(RetentionPosture::Classified(RetentionClassReference::new(
            "R4 Trust / moderation / case",
        ))),
        authority_posture: SocialTrustAuthorityPosture::WriterTruthOnly,
        boundary_posture,
    }
}

fn record_input(
    subject_account_id: Uuid,
    proposal: ProposedC2BoundedPromiseReliabilityMutationFact,
) -> RecordC2BoundedPromiseReliabilityMutationFactInput {
    RecordC2BoundedPromiseReliabilityMutationFactInput {
        subject_account_id: subject_account_id.to_string(),
        realm_reference: Some(REALM_REFERENCE.to_owned()),
        proposal,
    }
}

fn assert_rejected_before_persistence(
    outcome: SocialTrustMutationPersistenceOutcome,
    expected: C2BoundedPromiseReliabilityRejection,
) {
    match outcome {
        SocialTrustMutationPersistenceOutcome::RejectedBeforePersistence {
            decision: C2BoundedPromiseReliabilityMutationDecision::Reject(actual),
        } => assert_eq!(actual, expected),
        other => panic!("expected rejected before persistence, got {other:?}"),
    }
}

async fn assert_no_writer_projection_or_coordination_side_effects(
    client: &tokio_postgres::Client,
    subject_account_id: &Uuid,
    before_coordination: &CoordinationCounts,
) {
    assert_eq!(
        source_count_for_subject(client, subject_account_id).await,
        0
    );
    assert_eq!(
        mutation_count_for_subject(client, subject_account_id).await,
        0
    );
    assert_projection_absent_for_subject(client, subject_account_id).await;
    assert_eq!(&coordination_counts(client).await, before_coordination);
}

async fn source_count_for_subject(
    client: &tokio_postgres::Client,
    subject_account_id: &Uuid,
) -> i64 {
    let row = client
        .query_one(
            "
            SELECT COUNT(*)::bigint AS count
            FROM social_trust.categorical_source_references
            WHERE subject_account_id = $1
            ",
            &[subject_account_id],
        )
        .await
        .expect("source reference count should load");
    row.get("count")
}

async fn mutation_count_for_subject(
    client: &tokio_postgres::Client,
    subject_account_id: &Uuid,
) -> i64 {
    let row = client
        .query_one(
            "
            SELECT COUNT(*)::bigint AS count
            FROM social_trust.categorical_mutation_facts
            WHERE subject_account_id = $1
            ",
            &[subject_account_id],
        )
        .await
        .expect("mutation fact count should load");
    row.get("count")
}

async fn assert_projection_absent_for_subject(
    client: &tokio_postgres::Client,
    subject_account_id: &Uuid,
) {
    let row = client
        .query_one(
            "
            SELECT
                (SELECT COUNT(*)::bigint
                 FROM projection.trust_snapshots
                 WHERE account_id = $1) AS trust_snapshot_count,
                (SELECT COUNT(*)::bigint
                 FROM projection.realm_trust_snapshots
                 WHERE account_id = $1) AS realm_trust_snapshot_count
            ",
            &[subject_account_id],
        )
        .await
        .expect("projection counts should load");

    assert_eq!(row.get::<_, i64>("trust_snapshot_count"), 0);
    assert_eq!(row.get::<_, i64>("realm_trust_snapshot_count"), 0);
}

#[derive(Debug, PartialEq, Eq)]
struct CoordinationCounts {
    outbox_events: i64,
    outbox_attempts: i64,
    command_inbox: i64,
    outbox_event_archive: i64,
    outbox_attempt_archive: i64,
    command_inbox_archive: i64,
}

async fn coordination_counts(client: &tokio_postgres::Client) -> CoordinationCounts {
    let row = client
        .query_one(
            "
            SELECT
                (SELECT COUNT(*)::bigint FROM outbox.events) AS outbox_events,
                (SELECT COUNT(*)::bigint FROM outbox.outbox_attempts) AS outbox_attempts,
                (SELECT COUNT(*)::bigint FROM outbox.command_inbox) AS command_inbox,
                (SELECT COUNT(*)::bigint FROM outbox.outbox_event_archive) AS outbox_event_archive,
                (SELECT COUNT(*)::bigint FROM outbox.outbox_attempt_archive) AS outbox_attempt_archive,
                (SELECT COUNT(*)::bigint FROM outbox.command_inbox_archive) AS command_inbox_archive
            ",
            &[],
        )
        .await
        .expect("coordination counts should load");

    CoordinationCounts {
        outbox_events: row.get("outbox_events"),
        outbox_attempts: row.get("outbox_attempts"),
        command_inbox: row.get("command_inbox"),
        outbox_event_archive: row.get("outbox_event_archive"),
        outbox_attempt_archive: row.get("outbox_attempt_archive"),
        command_inbox_archive: row.get("command_inbox_archive"),
    }
}

async fn assert_public_projection_not_visible(app: &Router, subject: &SignedInUser) {
    let global = get_json(
        app,
        &format!("/api/projection/trust-snapshots/{}", subject.account_id),
        Some(subject.token.as_str()),
    )
    .await;
    assert_ne!(global.status, StatusCode::OK);
    assert_no_trust_or_lifecycle_exposure_fields(&global.body);

    let realm = get_json(
        app,
        &format!(
            "/api/projection/realm-trust-snapshots/{}/{}",
            REALM_REFERENCE, subject.account_id
        ),
        Some(subject.token.as_str()),
    )
    .await;
    assert_ne!(realm.status, StatusCode::OK);
    assert_no_trust_or_lifecycle_exposure_fields(&realm.body);
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
              )
            ",
            &[],
        )
        .await
        .expect("social trust column metadata should load");

    assert!(
        rows.is_empty(),
        "C2 categorical rejection boundary must not expose score/display/projection/Relationship Depth columns: {:?}",
        rows.iter()
            .map(|row| format!(
                "{}.{}",
                row.get::<_, String>("table_name"),
                row.get::<_, String>("column_name")
            ))
            .collect::<Vec<_>>()
    );
}

fn assert_no_trust_or_lifecycle_exposure_fields(body: &Value) {
    for field in [
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
            "C2 categorical rejection boundary must not expose {field} in public API response"
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

#[derive(Clone)]
struct RejectionInputCase {
    name: &'static str,
    expected: C2BoundedPromiseReliabilityRejection,
    mutate: fn(&mut ProposedC2BoundedPromiseReliabilityMutationFact),
}

fn rejection_input_cases() -> Vec<RejectionInputCase> {
    vec![
        RejectionInputCase {
            name: "projection-only-authority",
            expected: C2BoundedPromiseReliabilityRejection::ProjectionOnlyAuthority,
            mutate: |proposal| {
                proposal.authority_posture = SocialTrustAuthorityPosture::ProjectionOnly;
            },
        },
        RejectionInputCase {
            name: "unresolved-non-review-boundary",
            expected: C2BoundedPromiseReliabilityRejection::BoundaryUnresolved {
                boundary: C2BoundedPromiseReliabilityBoundaryIntersection::LegalHold,
            },
            mutate: |proposal| {
                proposal.boundary_posture = C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(
                    C2BoundedPromiseReliabilityBoundaryIntersection::LegalHold,
                );
            },
        },
        RejectionInputCase {
            name: "missing-review-required-boundary",
            expected:
                C2BoundedPromiseReliabilityRejection::MissingReviewRequiredBoundaryIntersection,
            mutate: |proposal| {
                proposal.source_fact = C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(
                    C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection,
                );
                proposal.requested_mutation_fact =
                    C2BoundedPromiseReliabilityMutationFactCandidate::Accepted(
                        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze,
                    );
                proposal.boundary_posture = C2BoundedPromiseReliabilityBoundaryPosture::Clear;
            },
        },
        RejectionInputCase {
            name: "source-mutation-mismatch",
            expected: C2BoundedPromiseReliabilityRejection::SourceMutationMismatch {
                source: C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
                requested: C2BoundedPromiseReliabilityMutationFact::NoEffectValidExcusedExit,
                expected:
                    C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
            },
            mutate: |proposal| {
                proposal.requested_mutation_fact =
                    C2BoundedPromiseReliabilityMutationFactCandidate::Accepted(
                        C2BoundedPromiseReliabilityMutationFact::NoEffectValidExcusedExit,
                    );
            },
        },
        RejectionInputCase {
            name: "missing-writer-source-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingWriterSourceReference,
            mutate: |proposal| {
                proposal.writer_source_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-promise-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingPromiseReference,
            mutate: |proposal| {
                proposal.promise_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-promise-terms-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingPromiseTermsReference,
            mutate: |proposal| {
                proposal.promise_terms_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-consent-at-formation-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingConsentAtFormationReference,
            mutate: |proposal| {
                proposal.consent_at_formation_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-consent-at-resolution-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingConsentAtResolutionReference,
            mutate: |proposal| {
                proposal.consent_at_resolution_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-block-withdrawal-state-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingBlockWithdrawalStateReference,
            mutate: |proposal| {
                proposal.block_withdrawal_state_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-age-assurance-state-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingAgeAssuranceStateReference,
            mutate: |proposal| {
                proposal.age_assurance_state_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-legal-hold-intersection-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingLegalHoldIntersectionReference,
            mutate: |proposal| {
                proposal.legal_hold_intersection_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-critical-harm-case-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingCriticalHarmCaseReference,
            mutate: |proposal| {
                proposal.critical_harm_case_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-account-lifecycle-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingAccountLifecycleReference,
            mutate: |proposal| {
                proposal.account_lifecycle_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-anti-abuse-continuity-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingAntiAbuseContinuityReference,
            mutate: |proposal| {
                proposal.anti_abuse_continuity_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-safety-case-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingSafetyCaseReference,
            mutate: |proposal| {
                proposal.safety_case_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-evidence-level-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingEvidenceLevelReference,
            mutate: |proposal| {
                proposal.evidence_level_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-audit-reference",
            expected: C2BoundedPromiseReliabilityRejection::MissingAuditReference,
            mutate: |proposal| {
                proposal.audit_reference = None;
            },
        },
        RejectionInputCase {
            name: "missing-reason-fact",
            expected: C2BoundedPromiseReliabilityRejection::MissingReasonFact,
            mutate: |proposal| {
                proposal.reason_fact = None;
            },
        },
        RejectionInputCase {
            name: "missing-fact-idempotency-key",
            expected: C2BoundedPromiseReliabilityRejection::MissingFactIdempotencyKey,
            mutate: |proposal| {
                proposal.fact_idempotency_key = None;
            },
        },
        RejectionInputCase {
            name: "missing-evidence-posture",
            expected: C2BoundedPromiseReliabilityRejection::MissingEvidencePosture,
            mutate: |proposal| {
                proposal.evidence_posture = None;
            },
        },
        RejectionInputCase {
            name: "missing-reviewability-posture",
            expected: C2BoundedPromiseReliabilityRejection::MissingReviewabilityPosture,
            mutate: |proposal| {
                proposal.reviewability_posture = None;
            },
        },
        RejectionInputCase {
            name: "missing-retention-posture",
            expected: C2BoundedPromiseReliabilityRejection::MissingRetentionPosture,
            mutate: |proposal| {
                proposal.retention_posture = None;
            },
        },
    ]
}

fn rejected_source_facts() -> Vec<RejectedC2BoundedPromiseReliabilitySourceFact> {
    vec![
        RejectedC2BoundedPromiseReliabilitySourceFact::PromiseCreation,
        RejectedC2BoundedPromiseReliabilitySourceFact::PromiseAcceptance,
        RejectedC2BoundedPromiseReliabilitySourceFact::PromiseTerms,
        RejectedC2BoundedPromiseReliabilitySourceFact::PromiseEscrowCreation,
        RejectedC2BoundedPromiseReliabilitySourceFact::EscrowAmount,
        RejectedC2BoundedPromiseReliabilitySourceFact::EscrowRelease,
        RejectedC2BoundedPromiseReliabilitySourceFact::Forfeiture,
        RejectedC2BoundedPromiseReliabilitySourceFact::PaymentAmount,
        RejectedC2BoundedPromiseReliabilitySourceFact::PaymentFrequency,
        RejectedC2BoundedPromiseReliabilitySourceFact::SupportAmount,
        RejectedC2BoundedPromiseReliabilitySourceFact::SupportStatus,
        RejectedC2BoundedPromiseReliabilitySourceFact::TokenHoldings,
        RejectedC2BoundedPromiseReliabilitySourceFact::MeetingAttendanceClaimByOneParty,
        RejectedC2BoundedPromiseReliabilitySourceFact::RawVenuePresence,
        RejectedC2BoundedPromiseReliabilitySourceFact::RawGps,
        RejectedC2BoundedPromiseReliabilitySourceFact::StaticQrScan,
        RejectedC2BoundedPromiseReliabilitySourceFact::NfcTapAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::BleObservationAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::BleMacAddress,
        RejectedC2BoundedPromiseReliabilitySourceFact::DeviceAttestationAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::MissingDeviceAttestationAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProximityProofAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProofEligibilityAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProofCallbackAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::VendorCallbackAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProviderDashboardState,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProjectionReadiness,
        RejectedC2BoundedPromiseReliabilitySourceFact::ReflectionPraise,
        RejectedC2BoundedPromiseReliabilitySourceFact::ApologyText,
        RejectedC2BoundedPromiseReliabilitySourceFact::SubjectiveGratitude,
        RejectedC2BoundedPromiseReliabilitySourceFact::SinglePartyNarrative,
        RejectedC2BoundedPromiseReliabilitySourceFact::ReportCount,
        RejectedC2BoundedPromiseReliabilitySourceFact::MassReportCount,
        RejectedC2BoundedPromiseReliabilitySourceFact::OperatorNote,
        RejectedC2BoundedPromiseReliabilitySourceFact::StewardEndorsementByItself,
        RejectedC2BoundedPromiseReliabilitySourceFact::SupportTicket,
        RejectedC2BoundedPromiseReliabilitySourceFact::IssueComment,
        RejectedC2BoundedPromiseReliabilitySourceFact::Popularity,
        RejectedC2BoundedPromiseReliabilitySourceFact::FollowerCount,
        RejectedC2BoundedPromiseReliabilitySourceFact::ReplySpeed,
        RejectedC2BoundedPromiseReliabilitySourceFact::DwellTime,
        RejectedC2BoundedPromiseReliabilitySourceFact::MessageVolume,
        RejectedC2BoundedPromiseReliabilitySourceFact::AccountTenure,
        RejectedC2BoundedPromiseReliabilitySourceFact::RomanticDesirability,
        RejectedC2BoundedPromiseReliabilitySourceFact::EngagementTelemetry,
        RejectedC2BoundedPromiseReliabilitySourceFact::RelationshipDepth,
        RejectedC2BoundedPromiseReliabilitySourceFact::RoomStateByItself,
        RejectedC2BoundedPromiseReliabilitySourceFact::RoomProjection,
        RejectedC2BoundedPromiseReliabilitySourceFact::DiscoveryRanking,
        RejectedC2BoundedPromiseReliabilitySourceFact::RecommendationState,
        RejectedC2BoundedPromiseReliabilitySourceFact::ObservabilityState,
        RejectedC2BoundedPromiseReliabilitySourceFact::ModelOutput,
        RejectedC2BoundedPromiseReliabilitySourceFact::FrontendState,
        RejectedC2BoundedPromiseReliabilitySourceFact::ClientState,
        RejectedC2BoundedPromiseReliabilitySourceFact::ControlledExceptionalAccountActivity,
        RejectedC2BoundedPromiseReliabilitySourceFact::AgeAssurancePosture,
        RejectedC2BoundedPromiseReliabilitySourceFact::VerifiedAdultPosture,
        RejectedC2BoundedPromiseReliabilitySourceFact::LegalHoldExistence,
        RejectedC2BoundedPromiseReliabilitySourceFact::AntiAbuseContinuityMarkerExistence,
        RejectedC2BoundedPromiseReliabilitySourceFact::AccountLifecycleStateByItself,
        RejectedC2BoundedPromiseReliabilitySourceFact::DeletionClosureTombstoneAnonymizationKeyShreddingOrReEntry,
        RejectedC2BoundedPromiseReliabilitySourceFact::ImplementationConvenience,
    ]
}
