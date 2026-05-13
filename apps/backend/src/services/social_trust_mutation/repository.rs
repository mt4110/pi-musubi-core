use std::{fmt::Write as _, sync::Arc};

use musubi_db_runtime::{DbConfig, connect_writer};
use musubi_social_trust_domain::{
    C2BoundedPromiseReliabilityMutationDecision, EvidencePosture,
    ProposedC2BoundedPromiseReliabilityMutationFact, RetentionPosture, ReviewabilityPosture,
    decide_c2_bounded_promise_reliability_mutation,
};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_postgres::{Client, GenericClient, Row, error::SqlState};
use uuid::Uuid;

use super::types::{
    C2BoundedPromiseReliabilityReplayStatus, C2BoundedPromiseReliabilitySnapshot,
    RecordC2BoundedPromiseReliabilityMutationFactInput, SocialTrustMutationPersistenceError,
    SocialTrustMutationPersistenceOutcome,
};

const POLICY_VERSION: i32 = 1;
const RETENTION_RECORD_FAMILY: &str = "Social Trust evidence or future Social Trust writer facts";
const RETENTION_CLASS_REFERENCE: &str = "R4 Trust / moderation / case";

#[derive(Clone)]
pub struct SocialTrustMutationStore {
    client: Arc<Mutex<Client>>,
}

struct NormalizedFact {
    subject_account_id: Uuid,
    source_fact_label: &'static str,
    mutation_fact_label: &'static str,
    mutation_direction: &'static str,
    mutation_magnitude: &'static str,
    boundary_intersection_label: Option<&'static str>,
    writer_source_reference: String,
    promise_reference: String,
    realm_reference: Option<String>,
    promise_terms_reference: String,
    consent_at_formation_reference: String,
    consent_at_resolution_reference: String,
    block_withdrawal_state_reference: String,
    age_assurance_state_reference: String,
    legal_hold_intersection_reference: String,
    critical_harm_case_reference: String,
    account_lifecycle_reference: String,
    anti_abuse_continuity_reference: String,
    safety_case_reference: String,
    evidence_level_reference: String,
    reason_fact_reference: String,
    audit_reference: String,
    fact_idempotency_key: String,
    evidence_posture: &'static str,
    reviewability_posture: &'static str,
    retention_class_reference: String,
    authority_posture: &'static str,
    request_payload_hash: String,
    decision_payload_hash: String,
}

impl SocialTrustMutationStore {
    pub async fn connect(config: &DbConfig) -> musubi_db_runtime::Result<Self> {
        let client = connect_writer(config, "musubi-backend social-trust-mutation").await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }

    pub async fn record_c2_bounded_promise_reliability_fact(
        &self,
        input: RecordC2BoundedPromiseReliabilityMutationFactInput,
    ) -> Result<SocialTrustMutationPersistenceOutcome, SocialTrustMutationPersistenceError> {
        let subject_account_id = parse_uuid(&input.subject_account_id, "subject account id")?;
        let decision = decide_c2_bounded_promise_reliability_mutation(&input.proposal);

        if matches!(
            decision,
            C2BoundedPromiseReliabilityMutationDecision::Reject(_)
        ) {
            return Ok(
                SocialTrustMutationPersistenceOutcome::RejectedBeforePersistence { decision },
            );
        }

        let normalized = normalize_fact(
            subject_account_id,
            input.realm_reference.as_deref(),
            &input.proposal,
            &decision,
        )?;
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;

        if let Some(existing) = find_existing_source_by_dedupe_tx(&tx, &normalized).await? {
            let source_reference_id = replay_source_reference_id(existing, &normalized)?;
            let snapshot = load_snapshot_by_source_reference_id_tx(
                &tx,
                &source_reference_id,
                C2BoundedPromiseReliabilityReplayStatus::ReplayedIdentical,
            )
            .await?;
            tx.commit().await.map_err(db_error)?;
            return Ok(SocialTrustMutationPersistenceOutcome::Recorded(snapshot));
        }

        ensure_active_ordinary_account_exists_tx(&tx, &normalized.subject_account_id).await?;

        let inserted_source_reference_id = Uuid::new_v4();
        let inserted = tx
            .query_opt(
                "
                INSERT INTO social_trust.categorical_source_references (
                    source_reference_id,
                    subject_account_id,
                    source_fact_label,
                    writer_source_reference,
                    promise_reference,
                    realm_reference,
                    boundary_intersection_label,
                    promise_terms_reference,
                    consent_at_formation_reference,
                    consent_at_resolution_reference,
                    block_withdrawal_state_reference,
                    age_assurance_state_reference,
                    legal_hold_intersection_reference,
                    critical_harm_case_reference,
                    account_lifecycle_reference,
                    anti_abuse_continuity_reference,
                    safety_case_reference,
                    evidence_level_reference,
                    reason_fact_reference,
                    audit_reference,
                    fact_idempotency_key,
                    policy_version,
                    request_payload_hash,
                    evidence_posture,
                    reviewability_posture,
                    retention_record_family,
                    retention_class_reference,
                    authority_posture
                )
                VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                    $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
                    $21, $22, $23, $24, $25, $26, $27, $28
                )
                ON CONFLICT (
                    subject_account_id,
                    policy_version,
                    fact_idempotency_key
                ) DO NOTHING
                RETURNING source_reference_id
                ",
                &[
                    &inserted_source_reference_id,
                    &normalized.subject_account_id,
                    &normalized.source_fact_label,
                    &normalized.writer_source_reference,
                    &normalized.promise_reference,
                    &normalized.realm_reference,
                    &normalized.boundary_intersection_label,
                    &normalized.promise_terms_reference,
                    &normalized.consent_at_formation_reference,
                    &normalized.consent_at_resolution_reference,
                    &normalized.block_withdrawal_state_reference,
                    &normalized.age_assurance_state_reference,
                    &normalized.legal_hold_intersection_reference,
                    &normalized.critical_harm_case_reference,
                    &normalized.account_lifecycle_reference,
                    &normalized.anti_abuse_continuity_reference,
                    &normalized.safety_case_reference,
                    &normalized.evidence_level_reference,
                    &normalized.reason_fact_reference,
                    &normalized.audit_reference,
                    &normalized.fact_idempotency_key,
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
            .map_err(source_insert_error)?;

        let (source_reference_id, replay_status) = match inserted {
            Some(row) => {
                let source_reference_id: Uuid = row.get("source_reference_id");
                let mutation_fact_id = Uuid::new_v4();
                tx.execute(
                    "
                    INSERT INTO social_trust.categorical_mutation_facts (
                        mutation_fact_id,
                        source_reference_id,
                        subject_account_id,
                        source_fact_label,
                        mutation_fact_label,
                        mutation_direction,
                        mutation_magnitude,
                        fact_idempotency_key,
                        policy_version,
                        decision_payload_hash,
                        evidence_posture,
                        reviewability_posture,
                        retention_record_family,
                        retention_class_reference,
                        authority_posture
                    )
                    VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8,
                        $9, $10, $11, $12, $13, $14, $15
                    )
                    ",
                    &[
                        &mutation_fact_id,
                        &source_reference_id,
                        &normalized.subject_account_id,
                        &normalized.source_fact_label,
                        &normalized.mutation_fact_label,
                        &normalized.mutation_direction,
                        &normalized.mutation_magnitude,
                        &normalized.fact_idempotency_key,
                        &POLICY_VERSION,
                        &normalized.decision_payload_hash,
                        &normalized.evidence_posture,
                        &normalized.reviewability_posture,
                        &RETENTION_RECORD_FAMILY,
                        &normalized.retention_class_reference,
                        &normalized.authority_posture,
                    ],
                )
                .await
                .map_err(db_error)?;
                (
                    source_reference_id,
                    C2BoundedPromiseReliabilityReplayStatus::Inserted,
                )
            }
            None => {
                let existing = find_existing_source_by_dedupe_tx(&tx, &normalized)
                    .await?
                    .ok_or_else(|| {
                        SocialTrustMutationPersistenceError::Internal(
                            "idempotency conflict did not return existing source reference"
                                .to_owned(),
                        )
                    })?;
                let source_reference_id = replay_source_reference_id(existing, &normalized)?;
                (
                    source_reference_id,
                    C2BoundedPromiseReliabilityReplayStatus::ReplayedIdentical,
                )
            }
        };

        let snapshot =
            load_snapshot_by_source_reference_id_tx(&tx, &source_reference_id, replay_status)
                .await?;
        tx.commit().await.map_err(db_error)?;
        Ok(SocialTrustMutationPersistenceOutcome::Recorded(snapshot))
    }
}

fn normalize_fact(
    subject_account_id: Uuid,
    realm_reference: Option<&str>,
    proposal: &ProposedC2BoundedPromiseReliabilityMutationFact,
    decision: &C2BoundedPromiseReliabilityMutationDecision,
) -> Result<NormalizedFact, SocialTrustMutationPersistenceError> {
    let C2BoundedPromiseReliabilityMutationDecision::Persist {
        source_fact,
        mutation_fact,
        direction,
        magnitude,
    } = decision
    else {
        return Err(SocialTrustMutationPersistenceError::Internal(
            "C2 mutation persistence requires an accepted mutation decision".to_owned(),
        ));
    };

    let source_fact_label = source_fact.as_str();
    let mutation_fact_label = mutation_fact.as_str();
    let mutation_direction = direction.as_str();
    let mutation_magnitude = magnitude.as_str();
    let boundary_intersection_label = match proposal.boundary_posture {
        musubi_social_trust_domain::C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(
            boundary,
        ) => Some(boundary.as_str()),
        musubi_social_trust_domain::C2BoundedPromiseReliabilityBoundaryPosture::Clear => None,
    };
    let writer_source_reference = required_ref(
        proposal
            .writer_source_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "writer source reference",
    )?;
    let promise_reference = required_ref(
        proposal
            .promise_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "Promise reference",
    )?;
    let realm_reference = optional_ref(realm_reference, "Realm reference")?;
    let promise_terms_reference = required_ref(
        proposal
            .promise_terms_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "Promise terms reference",
    )?;
    let consent_at_formation_reference = required_ref(
        proposal
            .consent_at_formation_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "Consent state at Promise formation reference",
    )?;
    let consent_at_resolution_reference = required_ref(
        proposal
            .consent_at_resolution_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "Consent state at resolution reference",
    )?;
    let block_withdrawal_state_reference = required_ref(
        proposal
            .block_withdrawal_state_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "block or Withdrawal state reference",
    )?;
    let age_assurance_state_reference = required_ref(
        proposal
            .age_assurance_state_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "Age Assurance state reference",
    )?;
    let legal_hold_intersection_reference = required_ref(
        proposal
            .legal_hold_intersection_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "Legal Hold intersection reference",
    )?;
    let critical_harm_case_reference = required_ref(
        proposal
            .critical_harm_case_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "Critical Harm case reference",
    )?;
    let account_lifecycle_reference = required_ref(
        proposal
            .account_lifecycle_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "account lifecycle reference",
    )?;
    let anti_abuse_continuity_reference = required_ref(
        proposal
            .anti_abuse_continuity_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "Anti-Abuse Continuity Marker reference",
    )?;
    let safety_case_reference = required_ref(
        proposal
            .safety_case_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "safety case reference",
    )?;
    let evidence_level_reference = required_ref(
        proposal
            .evidence_level_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "evidence-level reference",
    )?;
    let reason_fact_reference = required_ref(
        proposal
            .reason_fact
            .as_ref()
            .map(|reference| reference.as_str()),
        "reason fact reference",
    )?;
    let audit_reference = required_ref(
        proposal
            .audit_reference
            .as_ref()
            .map(|reference| reference.as_str()),
        "audit reference",
    )?;
    let fact_idempotency_key = required_ref(
        proposal
            .fact_idempotency_key
            .as_ref()
            .map(|reference| reference.as_str()),
        "fact idempotency key",
    )?;
    let evidence_posture = evidence_posture(proposal)?;
    let reviewability_posture = reviewability_posture(proposal)?;
    let retention_class_reference = retention_class_reference(proposal)?;
    let authority_posture = proposal.authority_posture.as_str();
    let policy_version = POLICY_VERSION.to_string();
    let subject = subject_account_id.to_string();
    let (realm_reference_presence, realm) = match realm_reference.as_deref() {
        Some(reference) => ("present", reference),
        None => ("absent", ""),
    };
    let request_payload_hash = hash_parts(&[
        (
            "hash_kind",
            "c2_categorical_social_trust_source_reference_request",
        ),
        ("policy_version", &policy_version),
        ("subject_account_id", &subject),
        ("source_fact_label", source_fact_label),
        ("writer_source_reference", &writer_source_reference),
        ("promise_reference", &promise_reference),
        ("realm_reference_presence", realm_reference_presence),
        ("realm_reference", realm),
        (
            "boundary_intersection_label",
            boundary_intersection_label.unwrap_or("none"),
        ),
        ("promise_terms_reference", &promise_terms_reference),
        (
            "consent_at_formation_reference",
            &consent_at_formation_reference,
        ),
        (
            "consent_at_resolution_reference",
            &consent_at_resolution_reference,
        ),
        (
            "block_withdrawal_state_reference",
            &block_withdrawal_state_reference,
        ),
        (
            "age_assurance_state_reference",
            &age_assurance_state_reference,
        ),
        (
            "legal_hold_intersection_reference",
            &legal_hold_intersection_reference,
        ),
        (
            "critical_harm_case_reference",
            &critical_harm_case_reference,
        ),
        ("account_lifecycle_reference", &account_lifecycle_reference),
        (
            "anti_abuse_continuity_reference",
            &anti_abuse_continuity_reference,
        ),
        ("safety_case_reference", &safety_case_reference),
        ("evidence_level_reference", &evidence_level_reference),
        ("reason_fact_reference", &reason_fact_reference),
        ("audit_reference", &audit_reference),
        ("fact_idempotency_key", &fact_idempotency_key),
        ("evidence_posture", evidence_posture),
        ("reviewability_posture", reviewability_posture),
        ("retention_record_family", RETENTION_RECORD_FAMILY),
        ("retention_class_reference", &retention_class_reference),
        ("authority_posture", authority_posture),
    ]);
    let decision_payload_hash = hash_parts(&[
        ("hash_kind", "c2_categorical_social_trust_mutation_decision"),
        ("request_payload_hash", &request_payload_hash),
        ("decision_kind", decision.kind()),
        ("mutation_fact_label", mutation_fact_label),
        ("mutation_direction", mutation_direction),
        ("mutation_magnitude", mutation_magnitude),
        (
            "rejection_reason_code",
            decision.rejection_reason_code().unwrap_or("none"),
        ),
    ]);

    Ok(NormalizedFact {
        subject_account_id,
        source_fact_label,
        mutation_fact_label,
        mutation_direction,
        mutation_magnitude,
        boundary_intersection_label,
        writer_source_reference,
        promise_reference,
        realm_reference,
        promise_terms_reference,
        consent_at_formation_reference,
        consent_at_resolution_reference,
        block_withdrawal_state_reference,
        age_assurance_state_reference,
        legal_hold_intersection_reference,
        critical_harm_case_reference,
        account_lifecycle_reference,
        anti_abuse_continuity_reference,
        safety_case_reference,
        evidence_level_reference,
        reason_fact_reference,
        audit_reference,
        fact_idempotency_key,
        evidence_posture,
        reviewability_posture,
        retention_class_reference,
        authority_posture,
        request_payload_hash,
        decision_payload_hash,
    })
}

fn evidence_posture(
    proposal: &ProposedC2BoundedPromiseReliabilityMutationFact,
) -> Result<&'static str, SocialTrustMutationPersistenceError> {
    match proposal.evidence_posture.as_ref() {
        Some(EvidencePosture::Bounded) => Ok(EvidencePosture::Bounded.as_str()),
        None => Err(SocialTrustMutationPersistenceError::BadRequest(
            "C2 bounded Promise reliability persistence requires evidence posture".to_owned(),
        )),
    }
}

fn reviewability_posture(
    proposal: &ProposedC2BoundedPromiseReliabilityMutationFact,
) -> Result<&'static str, SocialTrustMutationPersistenceError> {
    match proposal.reviewability_posture.as_ref() {
        Some(ReviewabilityPosture::Reviewable) => Ok(ReviewabilityPosture::Reviewable.as_str()),
        None => Err(SocialTrustMutationPersistenceError::BadRequest(
            "C2 bounded Promise reliability persistence requires reviewability posture".to_owned(),
        )),
    }
}

fn retention_class_reference(
    proposal: &ProposedC2BoundedPromiseReliabilityMutationFact,
) -> Result<String, SocialTrustMutationPersistenceError> {
    match proposal.retention_posture.as_ref() {
        Some(RetentionPosture::Classified(reference))
            if reference.as_str().trim() == RETENTION_CLASS_REFERENCE =>
        {
            Ok(RETENTION_CLASS_REFERENCE.to_owned())
        }
        _ => Err(SocialTrustMutationPersistenceError::BadRequest(
            "C2 bounded Promise reliability persistence requires R4 Trust / moderation / case retention"
                .to_owned(),
        )),
    }
}

fn required_ref(
    value: Option<&str>,
    label: &'static str,
) -> Result<String, SocialTrustMutationPersistenceError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            SocialTrustMutationPersistenceError::BadRequest(format!(
                "C2 bounded Promise reliability persistence requires {label}"
            ))
        })
}

fn optional_ref(
    value: Option<&str>,
    label: &'static str,
) -> Result<Option<String>, SocialTrustMutationPersistenceError> {
    value
        .map(str::trim)
        .map(|value| {
            if value.is_empty() {
                Err(SocialTrustMutationPersistenceError::BadRequest(format!(
                    "C2 bounded Promise reliability persistence requires {label} when provided"
                )))
            } else {
                Ok(value.to_owned())
            }
        })
        .transpose()
}

async fn find_existing_source_by_dedupe_tx(
    client: &impl GenericClient,
    normalized: &NormalizedFact,
) -> Result<Option<Row>, SocialTrustMutationPersistenceError> {
    client
        .query_opt(
            "
            SELECT source_reference_id, request_payload_hash
            FROM social_trust.categorical_source_references
            WHERE subject_account_id = $1
              AND policy_version = $2
              AND fact_idempotency_key = $3
            ",
            &[
                &normalized.subject_account_id,
                &POLICY_VERSION,
                &normalized.fact_idempotency_key,
            ],
        )
        .await
        .map_err(db_error)
}

async fn load_snapshot_by_source_reference_id_tx(
    client: &impl GenericClient,
    source_reference_id: &Uuid,
    replay_status: C2BoundedPromiseReliabilityReplayStatus,
) -> Result<C2BoundedPromiseReliabilitySnapshot, SocialTrustMutationPersistenceError> {
    let row = client
        .query_one(
            "
            SELECT
                source.source_reference_id,
                mutation.mutation_fact_id,
                source.subject_account_id,
                source.source_fact_label,
                mutation.mutation_fact_label,
                mutation.mutation_direction,
                mutation.mutation_magnitude,
                source.request_payload_hash,
                mutation.decision_payload_hash,
                mutation.created_at
            FROM social_trust.categorical_source_references source
            JOIN social_trust.categorical_mutation_facts mutation
              ON mutation.source_reference_id = source.source_reference_id
            WHERE source.source_reference_id = $1
            ",
            &[source_reference_id],
        )
        .await
        .map_err(db_error)?;

    Ok(C2BoundedPromiseReliabilitySnapshot {
        source_reference_id: row.get::<_, Uuid>("source_reference_id").to_string(),
        mutation_fact_id: row.get::<_, Uuid>("mutation_fact_id").to_string(),
        subject_account_id: row.get::<_, Uuid>("subject_account_id").to_string(),
        source_fact_label: row.get("source_fact_label"),
        mutation_fact_label: row.get("mutation_fact_label"),
        mutation_direction: row.get("mutation_direction"),
        mutation_magnitude: row.get("mutation_magnitude"),
        request_payload_hash: row.get("request_payload_hash"),
        decision_payload_hash: row.get("decision_payload_hash"),
        replay_status,
        created_at: row.get("created_at"),
    })
}

fn replay_source_reference_id(
    existing: Row,
    normalized: &NormalizedFact,
) -> Result<Uuid, SocialTrustMutationPersistenceError> {
    let existing_hash: String = existing.get("request_payload_hash");
    let existing_source_reference_id: Uuid = existing.get("source_reference_id");
    if existing_hash != normalized.request_payload_hash {
        return Err(SocialTrustMutationPersistenceError::IdempotencyConflict {
            message: "duplicate C2 bounded Promise reliability fact has payload drift".to_owned(),
            existing_source_reference_id: existing_source_reference_id.to_string(),
        });
    }
    Ok(existing_source_reference_id)
}

async fn ensure_active_ordinary_account_exists_tx(
    client: &impl GenericClient,
    account_id: &Uuid,
) -> Result<(), SocialTrustMutationPersistenceError> {
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
        return Err(SocialTrustMutationPersistenceError::BadRequest(
            "C2 bounded Promise reliability subject must reference an existing account writer fact"
                .to_owned(),
        ));
    };
    let account_class: String = row.get("account_class");
    let account_state: String = row.get("account_state");

    if account_class != "Ordinary Account" {
        return Err(SocialTrustMutationPersistenceError::BadRequest(
            "C2 bounded Promise reliability subject must be an Ordinary Account".to_owned(),
        ));
    }

    if account_state != "active" {
        return Err(SocialTrustMutationPersistenceError::BadRequest(
            "C2 bounded Promise reliability subject account must be active".to_owned(),
        ));
    }

    Ok(())
}

fn source_insert_error(error: tokio_postgres::Error) -> SocialTrustMutationPersistenceError {
    if matches!(error.code(), Some(&SqlState::FOREIGN_KEY_VIOLATION)) {
        return SocialTrustMutationPersistenceError::BadRequest(
            "C2 bounded Promise reliability subject must reference an existing account writer fact"
                .to_owned(),
        );
    }

    db_error(error)
}

fn parse_uuid(
    value: &str,
    label: &'static str,
) -> Result<Uuid, SocialTrustMutationPersistenceError> {
    Uuid::parse_str(value).map_err(|_| {
        SocialTrustMutationPersistenceError::BadRequest(format!("{label} must be a valid UUID"))
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

fn db_error(error: tokio_postgres::Error) -> SocialTrustMutationPersistenceError {
    let code = error.code().map(SqlState::code).map(ToOwned::to_owned);
    let constraint = error
        .as_db_error()
        .and_then(|db_error| db_error.constraint())
        .map(ToOwned::to_owned);
    let retryable = matches!(
        error.code(),
        Some(&SqlState::T_R_SERIALIZATION_FAILURE) | Some(&SqlState::T_R_DEADLOCK_DETECTED)
    );

    SocialTrustMutationPersistenceError::Database {
        message: error.to_string(),
        code,
        constraint,
        retryable,
    }
}
