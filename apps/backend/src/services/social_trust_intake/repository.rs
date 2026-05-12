use std::{fmt::Write as _, sync::Arc};

use musubi_db_runtime::{DbConfig, connect_writer};
use musubi_social_trust_domain::{
    DurableIdempotencyPosture, EvidencePosture, ProposedSocialTrustMutationAttempt,
    RetentionPosture, ReviewabilityPosture, SocialTrustIntakeDecision, decide_social_trust_intake,
};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_postgres::{Client, GenericClient, Row, error::SqlState};
use uuid::Uuid;

use super::types::{
    RecordSocialTrustIntakeAttemptInput, SocialTrustIntakePersistenceError,
    SocialTrustIntakePersistenceOutcome, SocialTrustIntakeReplayStatus, SocialTrustIntakeSnapshot,
};

const POLICY_VERSION: i32 = 1;
const RETENTION_RECORD_FAMILY: &str = "Social Trust evidence or future Social Trust writer facts";

#[derive(Clone)]
pub struct SocialTrustIntakeStore {
    client: Arc<Mutex<Client>>,
}

struct NormalizedAttempt {
    subject_account_id: Uuid,
    source_category: &'static str,
    writer_source_reference: String,
    reason_fact_reference: String,
    attempt_idempotency_key: String,
    evidence_posture: &'static str,
    reviewability_posture: &'static str,
    retention_class_reference: String,
    authority_posture: &'static str,
    decision_kind: &'static str,
    rejection_reason_code: Option<&'static str>,
    request_payload_hash: String,
    decision_payload_hash: String,
}

impl SocialTrustIntakeStore {
    pub async fn connect(config: &DbConfig) -> musubi_db_runtime::Result<Self> {
        let client = connect_writer(config, "musubi-backend social-trust-intake").await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }

    pub async fn record_attempt(
        &self,
        input: RecordSocialTrustIntakeAttemptInput,
    ) -> Result<SocialTrustIntakePersistenceOutcome, SocialTrustIntakePersistenceError> {
        let subject_account_id = parse_uuid(&input.subject_account_id, "subject account id")?;
        let decision = decide_social_trust_intake(&input.attempt);

        if !has_persistence_minimum(&input.attempt) {
            return Ok(SocialTrustIntakePersistenceOutcome::RejectedBeforePersistence { decision });
        }

        let normalized = normalize_attempt(subject_account_id, &input.attempt, &decision)?;
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;

        if let Some(existing) = find_existing_attempt_by_dedupe_tx(&tx, &normalized).await? {
            let attempt_id = replay_attempt_id(existing, &normalized)?;
            let snapshot = load_snapshot_by_attempt_id_tx(
                &tx,
                &attempt_id,
                SocialTrustIntakeReplayStatus::ReplayedIdentical,
            )
            .await?;
            tx.commit().await.map_err(db_error)?;
            return Ok(SocialTrustIntakePersistenceOutcome::Recorded(snapshot));
        }

        ensure_active_ordinary_account_exists_tx(&tx, &normalized.subject_account_id).await?;

        let inserted_attempt_id = Uuid::new_v4();
        let inserted = tx
            .query_opt(
                "
                INSERT INTO social_trust.proposed_mutation_attempts (
                    attempt_id,
                    subject_account_id,
                    source_category,
                    writer_source_reference,
                    reason_fact_reference,
                    attempt_idempotency_key,
                    policy_version,
                    request_payload_hash,
                    evidence_posture,
                    reviewability_posture,
                    retention_record_family,
                    retention_class_reference,
                    authority_posture
                )
                VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
                )
                ON CONFLICT (
                    subject_account_id,
                    source_category,
                    writer_source_reference,
                    reason_fact_reference,
                    policy_version,
                    attempt_idempotency_key
                ) DO NOTHING
                RETURNING attempt_id
                ",
                &[
                    &inserted_attempt_id,
                    &normalized.subject_account_id,
                    &normalized.source_category,
                    &normalized.writer_source_reference,
                    &normalized.reason_fact_reference,
                    &normalized.attempt_idempotency_key,
                    &POLICY_VERSION,
                    &normalized.request_payload_hash,
                    &normalized.evidence_posture,
                    &normalized.reviewability_posture,
                    &RETENTION_RECORD_FAMILY,
                    &normalized.retention_class_reference,
                    &normalized.authority_posture,
                ],
            )
            .await
            .map_err(attempt_insert_error)?;

        let (attempt_id, replay_status) = match inserted {
            Some(row) => {
                let attempt_id: Uuid = row.get("attempt_id");
                let intake_decision_id = Uuid::new_v4();
                tx.execute(
                    "
                    INSERT INTO social_trust.intake_decisions (
                        intake_decision_id,
                        attempt_id,
                        decision_kind,
                        rejection_reason_code,
                        decision_payload_hash,
                        retention_record_family,
                        retention_class_reference
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7)
                    ",
                    &[
                        &intake_decision_id,
                        &attempt_id,
                        &normalized.decision_kind,
                        &normalized.rejection_reason_code,
                        &normalized.decision_payload_hash,
                        &RETENTION_RECORD_FAMILY,
                        &normalized.retention_class_reference,
                    ],
                )
                .await
                .map_err(db_error)?;
                (attempt_id, SocialTrustIntakeReplayStatus::Inserted)
            }
            None => {
                let existing = find_existing_attempt_by_dedupe_tx(&tx, &normalized)
                    .await?
                    .ok_or_else(|| {
                        SocialTrustIntakePersistenceError::Internal(
                            "idempotency conflict did not return existing attempt".to_owned(),
                        )
                    })?;
                let existing_attempt_id = replay_attempt_id(existing, &normalized)?;
                (
                    existing_attempt_id,
                    SocialTrustIntakeReplayStatus::ReplayedIdentical,
                )
            }
        };

        let snapshot = load_snapshot_by_attempt_id_tx(&tx, &attempt_id, replay_status).await?;
        tx.commit().await.map_err(db_error)?;
        Ok(SocialTrustIntakePersistenceOutcome::Recorded(snapshot))
    }
}

fn normalize_attempt(
    subject_account_id: Uuid,
    attempt: &ProposedSocialTrustMutationAttempt,
    decision: &SocialTrustIntakeDecision,
) -> Result<NormalizedAttempt, SocialTrustIntakePersistenceError> {
    let source_category = attempt.source_category.as_str();
    let writer_source_reference = required_ref(
        attempt
            .writer_source_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "writer source reference",
    )?;
    let reason_fact_reference = required_ref(
        attempt
            .reason_fact
            .as_ref()
            .map(|reference| reference.as_str()),
        "reason fact reference",
    )?;
    let attempt_idempotency_key = required_ref(idempotency_key(attempt), "idempotency key")?;
    let evidence_posture = evidence_posture(attempt)?;
    let reviewability_posture = reviewability_posture(attempt)?;
    let retention_class_reference = retention_class_reference(attempt)?;
    let authority_posture = attempt.authority_posture.as_str();
    let decision_kind = decision.kind();
    let rejection_reason_code = decision.rejection_reason_code();
    let policy_version = POLICY_VERSION.to_string();
    let subject = subject_account_id.to_string();
    let request_payload_hash = hash_parts(&[
        ("hash_kind", "social_trust_intake_attempt_request"),
        ("policy_version", &policy_version),
        ("subject_account_id", &subject),
        ("source_category", source_category),
        ("writer_source_reference", &writer_source_reference),
        ("reason_fact_reference", &reason_fact_reference),
        ("attempt_idempotency_key", &attempt_idempotency_key),
        ("evidence_posture", evidence_posture),
        ("reviewability_posture", reviewability_posture),
        ("retention_record_family", RETENTION_RECORD_FAMILY),
        ("retention_class_reference", &retention_class_reference),
        ("authority_posture", authority_posture),
    ]);
    let decision_payload_hash = hash_parts(&[
        ("hash_kind", "social_trust_intake_attempt_decision"),
        ("request_payload_hash", &request_payload_hash),
        ("decision_kind", decision_kind),
        (
            "rejection_reason_code",
            rejection_reason_code.unwrap_or("none"),
        ),
    ]);

    Ok(NormalizedAttempt {
        subject_account_id,
        source_category,
        writer_source_reference,
        reason_fact_reference,
        attempt_idempotency_key,
        evidence_posture,
        reviewability_posture,
        retention_class_reference,
        authority_posture,
        decision_kind,
        rejection_reason_code,
        request_payload_hash,
        decision_payload_hash,
    })
}

fn has_persistence_minimum(attempt: &ProposedSocialTrustMutationAttempt) -> bool {
    attempt
        .writer_source_reference
        .as_ref()
        .is_some_and(|reference| reference.is_present())
        && attempt
            .reason_fact
            .as_ref()
            .is_some_and(|reference| reference.is_present())
        && idempotency_key(attempt)
            .is_some_and(|idempotency_key| !idempotency_key.trim().is_empty())
        && attempt.evidence_posture.is_some()
        && attempt.reviewability_posture.is_some()
        && retention_class_reference(attempt)
            .is_ok_and(|retention_class_reference| !retention_class_reference.trim().is_empty())
}

fn idempotency_key(attempt: &ProposedSocialTrustMutationAttempt) -> Option<&str> {
    match attempt.idempotency_posture.as_ref()? {
        DurableIdempotencyPosture::DurableDedupeKey(key) => Some(key.as_str()),
    }
}

fn evidence_posture(
    attempt: &ProposedSocialTrustMutationAttempt,
) -> Result<&'static str, SocialTrustIntakePersistenceError> {
    match attempt.evidence_posture.as_ref() {
        Some(EvidencePosture::Bounded) => Ok(EvidencePosture::Bounded.as_str()),
        None => Err(SocialTrustIntakePersistenceError::BadRequest(
            "Social Trust intake persistence requires evidence posture".to_owned(),
        )),
    }
}

fn reviewability_posture(
    attempt: &ProposedSocialTrustMutationAttempt,
) -> Result<&'static str, SocialTrustIntakePersistenceError> {
    match attempt.reviewability_posture.as_ref() {
        Some(ReviewabilityPosture::Reviewable) => Ok(ReviewabilityPosture::Reviewable.as_str()),
        None => Err(SocialTrustIntakePersistenceError::BadRequest(
            "Social Trust intake persistence requires reviewability posture".to_owned(),
        )),
    }
}

fn retention_class_reference(
    attempt: &ProposedSocialTrustMutationAttempt,
) -> Result<String, SocialTrustIntakePersistenceError> {
    match attempt.retention_posture.as_ref() {
        Some(RetentionPosture::Classified(reference)) if reference.is_present() => {
            Ok(reference.as_str().trim().to_owned())
        }
        _ => Err(SocialTrustIntakePersistenceError::BadRequest(
            "Social Trust intake persistence requires retention classification".to_owned(),
        )),
    }
}

fn required_ref(
    value: Option<&str>,
    label: &'static str,
) -> Result<String, SocialTrustIntakePersistenceError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            SocialTrustIntakePersistenceError::BadRequest(format!(
                "Social Trust intake persistence requires {label}"
            ))
        })
}

async fn find_existing_attempt_by_dedupe_tx(
    client: &impl GenericClient,
    normalized: &NormalizedAttempt,
) -> Result<Option<Row>, SocialTrustIntakePersistenceError> {
    client
        .query_opt(
            "
            SELECT attempt.attempt_id, attempt.request_payload_hash
            FROM social_trust.proposed_mutation_attempts attempt
            WHERE attempt.subject_account_id = $1
              AND attempt.source_category = $2
              AND attempt.writer_source_reference = $3
              AND attempt.reason_fact_reference = $4
              AND attempt.policy_version = $5
              AND attempt.attempt_idempotency_key = $6
            ",
            &[
                &normalized.subject_account_id,
                &normalized.source_category,
                &normalized.writer_source_reference,
                &normalized.reason_fact_reference,
                &POLICY_VERSION,
                &normalized.attempt_idempotency_key,
            ],
        )
        .await
        .map_err(db_error)
}

async fn load_snapshot_by_attempt_id_tx(
    client: &impl GenericClient,
    attempt_id: &Uuid,
    replay_status: SocialTrustIntakeReplayStatus,
) -> Result<SocialTrustIntakeSnapshot, SocialTrustIntakePersistenceError> {
    let row = client
        .query_one(
            "
            SELECT
                attempt.attempt_id,
                decision.intake_decision_id,
                attempt.subject_account_id,
                attempt.source_category,
                decision.decision_kind,
                decision.rejection_reason_code,
                attempt.request_payload_hash,
                decision.decision_payload_hash,
                attempt.created_at
            FROM social_trust.proposed_mutation_attempts attempt
            JOIN social_trust.intake_decisions decision
              ON decision.attempt_id = attempt.attempt_id
            WHERE attempt.attempt_id = $1
            ",
            &[attempt_id],
        )
        .await
        .map_err(db_error)?;

    Ok(SocialTrustIntakeSnapshot {
        attempt_id: row.get::<_, Uuid>("attempt_id").to_string(),
        intake_decision_id: row.get::<_, Uuid>("intake_decision_id").to_string(),
        subject_account_id: row.get::<_, Uuid>("subject_account_id").to_string(),
        source_category: row.get("source_category"),
        decision_kind: row.get("decision_kind"),
        rejection_reason_code: row.get("rejection_reason_code"),
        request_payload_hash: row.get("request_payload_hash"),
        decision_payload_hash: row.get("decision_payload_hash"),
        replay_status,
        created_at: row.get("created_at"),
    })
}

fn replay_attempt_id(
    existing: Row,
    normalized: &NormalizedAttempt,
) -> Result<Uuid, SocialTrustIntakePersistenceError> {
    let existing_hash: String = existing.get("request_payload_hash");
    let existing_attempt_id: Uuid = existing.get("attempt_id");
    if existing_hash != normalized.request_payload_hash {
        return Err(SocialTrustIntakePersistenceError::IdempotencyConflict {
            message: "duplicate Social Trust intake attempt has payload drift".to_owned(),
            existing_attempt_id: existing_attempt_id.to_string(),
        });
    }
    Ok(existing_attempt_id)
}

async fn ensure_active_ordinary_account_exists_tx(
    client: &impl GenericClient,
    account_id: &Uuid,
) -> Result<(), SocialTrustIntakePersistenceError> {
    let Some(row) = client
        .query_opt(
            "
            SELECT account_class, account_state
            FROM core.accounts
            WHERE account_id = $1
            FOR UPDATE
            ",
            &[account_id],
        )
        .await
        .map_err(db_error)?
    else {
        return Err(SocialTrustIntakePersistenceError::BadRequest(
            "Social Trust intake subject must reference an existing account writer fact".to_owned(),
        ));
    };
    let account_class: String = row.get("account_class");
    let account_state: String = row.get("account_state");

    if account_class != "Ordinary Account" {
        return Err(SocialTrustIntakePersistenceError::BadRequest(
            "Social Trust intake subject must be an Ordinary Account".to_owned(),
        ));
    }

    if account_state != "active" {
        return Err(SocialTrustIntakePersistenceError::BadRequest(
            "Social Trust intake subject account must be active".to_owned(),
        ));
    }

    Ok(())
}

fn attempt_insert_error(error: tokio_postgres::Error) -> SocialTrustIntakePersistenceError {
    if matches!(error.code(), Some(&SqlState::FOREIGN_KEY_VIOLATION)) {
        return SocialTrustIntakePersistenceError::BadRequest(
            "Social Trust intake subject must reference an existing account writer fact".to_owned(),
        );
    }

    db_error(error)
}

fn parse_uuid(value: &str, label: &'static str) -> Result<Uuid, SocialTrustIntakePersistenceError> {
    Uuid::parse_str(value).map_err(|_| {
        SocialTrustIntakePersistenceError::BadRequest(format!("{label} must be a valid UUID"))
    })
}

fn hash_parts(parts: &[(&str, &str)]) -> String {
    let mut hasher = Sha256::new();
    for (name, value) in parts {
        hasher.update(name.as_bytes());
        hasher.update(b"\0");
        hasher.update(value.len().to_string().as_bytes());
        hasher.update(b":");
        hasher.update(value.as_bytes());
        hasher.update(b"\n");
    }
    hex_digest(hasher.finalize().as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing sha256 hex cannot fail");
    }
    output
}

fn db_error(error: tokio_postgres::Error) -> SocialTrustIntakePersistenceError {
    let code = error.code().map(SqlState::code).map(ToOwned::to_owned);
    let constraint = error
        .as_db_error()
        .and_then(|db_error| db_error.constraint())
        .map(ToOwned::to_owned);
    let retryable = matches!(
        error.code(),
        Some(&SqlState::T_R_SERIALIZATION_FAILURE) | Some(&SqlState::T_R_DEADLOCK_DETECTED)
    );

    SocialTrustIntakePersistenceError::Database {
        message: error.to_string(),
        code,
        constraint,
        retryable,
    }
}
