use std::{fmt::Write as _, sync::Arc};

use musubi_db_runtime::{DbConfig, connect_writer};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_postgres::{Client, GenericClient, Row, error::SqlState};
use uuid::Uuid;

use super::types::{
    PromiseCompletionAuthorityPosture, PromiseCompletionNonAuthorityProjectionSnapshot,
    PromiseCompletionProjectionNonAuthorityPosture, PromiseCompletionSourceRouteClass,
    PromiseCompletionStateClass, PromiseCompletionWriterFactFamily,
    PromiseCompletionWriterFactPersistenceError, PromiseCompletionWriterFactReplayStatus,
    PromiseCompletionWriterFactSnapshot, ProposedPromiseCompletionWriterFact,
    RecordMutualAcknowledgementAcceptedTransitionInput, RecordPromiseCompletionWriterFactInput,
};

const DECISION_KIND: &str = "accepted_for_writer_fact_persistence";

#[derive(Clone)]
pub struct PromiseCompletionWriterFactStore {
    client: Arc<Mutex<Client>>,
}

struct NormalizedWriterFact {
    promise_reference: String,
    realm_id: String,
    fact_family: &'static str,
    source_route_class: &'static str,
    previous_completion_state_class: Option<&'static str>,
    completion_state_class: &'static str,
    completed_reference_eligible: bool,
    promise_terms_reference: String,
    participant_set_reference: String,
    ordinary_participant_acknowledgement_reference: Option<String>,
    governed_review_reference: Option<String>,
    review_authority_reference: Option<String>,
    proof_eligibility_reference: Option<String>,
    proof_evidence_writer_fact_reference: Option<String>,
    consent_at_formation_reference: String,
    consent_at_resolution_reference: String,
    block_withdrawal_state_reference: String,
    age_assurance_state_reference: String,
    legal_hold_intersection_reference: String,
    critical_harm_case_reference: String,
    account_lifecycle_reference: String,
    anti_abuse_continuity_reference: String,
    safety_case_reference: String,
    reason_code_class: String,
    evidence_level_reference: String,
    correction_or_supersession_reference: Option<String>,
    prior_writer_fact_id: Option<Uuid>,
    policy_version: i32,
    fact_idempotency_key: String,
    retention_class_reference: String,
    access_audit_reference: String,
    projection_non_authority_posture: &'static str,
    authority_posture: &'static str,
    request_payload_hash: String,
    decision_payload_hash: String,
}

impl PromiseCompletionWriterFactStore {
    pub async fn connect(config: &DbConfig) -> musubi_db_runtime::Result<Self> {
        let client = connect_writer(config, "musubi-backend promise-completion").await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }

    pub async fn record_writer_fact(
        &self,
        input: RecordPromiseCompletionWriterFactInput,
    ) -> Result<PromiseCompletionWriterFactSnapshot, PromiseCompletionWriterFactPersistenceError>
    {
        let normalized = normalize_writer_fact(&input.fact)?;
        let client = self.client.lock().await;

        if let Some(existing) = find_existing_writer_fact_by_dedupe(&*client, &normalized).await? {
            let writer_fact_id = replay_writer_fact_id(existing, &normalized)?;
            return load_snapshot_by_writer_fact_id(
                &*client,
                &writer_fact_id,
                PromiseCompletionWriterFactReplayStatus::ReplayedIdentical,
            )
            .await;
        }

        let inserted_writer_fact_id = Uuid::new_v4();
        let inserted = client
            .query_opt(
                "
                INSERT INTO promise_completion.writer_fact_records (
                    writer_fact_id,
                    promise_reference,
                    realm_id,
                    fact_family,
                    source_route_class,
                    previous_completion_state_class,
                    completion_state_class,
                    completed_reference_eligible,
                    promise_terms_reference,
                    participant_set_reference,
                    ordinary_participant_acknowledgement_reference,
                    governed_review_reference,
                    review_authority_reference,
                    proof_eligibility_reference,
                    proof_evidence_writer_fact_reference,
                    consent_at_formation_reference,
                    consent_at_resolution_reference,
                    block_withdrawal_state_reference,
                    age_assurance_state_reference,
                    legal_hold_intersection_reference,
                    critical_harm_case_reference,
                    account_lifecycle_reference,
                    anti_abuse_continuity_reference,
                    safety_case_reference,
                    reason_code_class,
                    evidence_level_reference,
                    correction_or_supersession_reference,
                    prior_writer_fact_id,
                    policy_version,
                    fact_idempotency_key,
                    request_payload_hash,
                    decision_payload_hash,
                    retention_class_reference,
                    access_audit_reference,
                    projection_non_authority_posture,
                    authority_posture
                )
                VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8,
                    $9, $10, $11, $12, $13, $14, $15, $16,
                    $17, $18, $19, $20, $21, $22, $23, $24,
                    $25, $26, $27, $28, $29, $30, $31, $32,
                    $33, $34, $35, $36
                )
                ON CONFLICT (
                    realm_id,
                    promise_reference,
                    policy_version,
                    fact_idempotency_key
                ) DO NOTHING
                RETURNING writer_fact_id
                ",
                &[
                    &inserted_writer_fact_id,
                    &normalized.promise_reference,
                    &normalized.realm_id,
                    &normalized.fact_family,
                    &normalized.source_route_class,
                    &normalized.previous_completion_state_class,
                    &normalized.completion_state_class,
                    &normalized.completed_reference_eligible,
                    &normalized.promise_terms_reference,
                    &normalized.participant_set_reference,
                    &normalized.ordinary_participant_acknowledgement_reference,
                    &normalized.governed_review_reference,
                    &normalized.review_authority_reference,
                    &normalized.proof_eligibility_reference,
                    &normalized.proof_evidence_writer_fact_reference,
                    &normalized.consent_at_formation_reference,
                    &normalized.consent_at_resolution_reference,
                    &normalized.block_withdrawal_state_reference,
                    &normalized.age_assurance_state_reference,
                    &normalized.legal_hold_intersection_reference,
                    &normalized.critical_harm_case_reference,
                    &normalized.account_lifecycle_reference,
                    &normalized.anti_abuse_continuity_reference,
                    &normalized.safety_case_reference,
                    &normalized.reason_code_class,
                    &normalized.evidence_level_reference,
                    &normalized.correction_or_supersession_reference,
                    &normalized.prior_writer_fact_id,
                    &normalized.policy_version,
                    &normalized.fact_idempotency_key,
                    &normalized.request_payload_hash,
                    &normalized.decision_payload_hash,
                    &normalized.retention_class_reference,
                    &normalized.access_audit_reference,
                    &normalized.projection_non_authority_posture,
                    &normalized.authority_posture,
                ],
            )
            .await
            .map_err(writer_fact_insert_error)?;

        let (writer_fact_id, replay_status) = match inserted {
            Some(row) => {
                let writer_fact_id: Uuid = row.get("writer_fact_id");
                (
                    writer_fact_id,
                    PromiseCompletionWriterFactReplayStatus::Inserted,
                )
            }
            None => {
                let existing = find_existing_writer_fact_by_dedupe(&*client, &normalized)
                    .await?
                    .ok_or_else(|| {
                        PromiseCompletionWriterFactPersistenceError::Internal(
                            "idempotency conflict did not return existing Promise completion writer fact"
                                .to_owned(),
                        )
                    })?;
                let writer_fact_id = replay_writer_fact_id(existing, &normalized)?;
                (
                    writer_fact_id,
                    PromiseCompletionWriterFactReplayStatus::ReplayedIdentical,
                )
            }
        };

        let snapshot =
            load_snapshot_by_writer_fact_id(&*client, &writer_fact_id, replay_status).await?;
        Ok(snapshot)
    }

    pub async fn record_mutual_acknowledgement_accepted_transition(
        &self,
        input: RecordMutualAcknowledgementAcceptedTransitionInput,
    ) -> Result<PromiseCompletionWriterFactSnapshot, PromiseCompletionWriterFactPersistenceError>
    {
        let fact = validate_mutual_acknowledgement_accepted_transition(input.transition.fact)?;
        let normalized = normalize_writer_fact(&fact)?;
        {
            let client = self.client.lock().await;
            ensure_mutual_acknowledgement_prior_writer_fact(&*client, &normalized).await?;
        }
        self.record_writer_fact(RecordPromiseCompletionWriterFactInput { fact })
            .await
    }

    pub async fn derive_accepted_completion_non_authority_projection_snapshots(
        &self,
        promise_reference: &str,
        realm_id: &str,
    ) -> Result<
        Vec<PromiseCompletionNonAuthorityProjectionSnapshot>,
        PromiseCompletionWriterFactPersistenceError,
    > {
        let promise_reference = required_projection_ref(promise_reference, "Promise reference")?;
        let realm_id = required_projection_ref(realm_id, "realm_id")?;
        let client = self.client.lock().await;
        load_accepted_completion_non_authority_projection_snapshots(
            &*client,
            &promise_reference,
            &realm_id,
        )
        .await
    }
}

fn validate_mutual_acknowledgement_accepted_transition(
    fact: ProposedPromiseCompletionWriterFact,
) -> Result<ProposedPromiseCompletionWriterFact, PromiseCompletionWriterFactPersistenceError> {
    if fact.fact_family != PromiseCompletionWriterFactFamily::CompletionStateTransition {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition requires completion_state_transition fact family"
                .to_owned(),
        ));
    }

    if fact.source_route_class
        != PromiseCompletionSourceRouteClass::MutualAccountableCompletionAcknowledgement
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition rejects non-mutual acknowledgement source routes"
                .to_owned(),
        ));
    }

    if fact.previous_completion_state_class
        != Some(PromiseCompletionStateClass::CompletionPendingMutualAcknowledgement)
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition requires previous state completion_pending_mutual_acknowledgement"
                .to_owned(),
        ));
    }

    if fact.completion_state_class != PromiseCompletionStateClass::CompletionAccepted {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition requires next state completion_accepted"
                .to_owned(),
        ));
    }

    if !fact.completed_reference_eligible {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition requires completed reference eligibility for completion_accepted"
                .to_owned(),
        ));
    }

    if fact
        .ordinary_participant_acknowledgement_reference
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition requires Ordinary Account participant acknowledgement reference"
                .to_owned(),
        ));
    }

    if fact
        .governed_review_reference
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
        || fact
            .review_authority_reference
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition does not accept governed review references"
                .to_owned(),
        ));
    }

    if fact
        .proof_eligibility_reference
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
        || fact
            .proof_evidence_writer_fact_reference
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition does not accept proof references"
                .to_owned(),
        ));
    }

    if fact
        .correction_or_supersession_reference
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition does not accept correction or supersession references"
                .to_owned(),
        ));
    }

    if fact
        .prior_writer_fact_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition requires prior writer fact reference"
                .to_owned(),
        ));
    }

    Ok(fact)
}

async fn ensure_mutual_acknowledgement_prior_writer_fact(
    client: &impl GenericClient,
    normalized: &NormalizedWriterFact,
) -> Result<(), PromiseCompletionWriterFactPersistenceError> {
    let prior_writer_fact_id = normalized.prior_writer_fact_id.ok_or_else(|| {
        PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition requires prior writer fact reference"
                .to_owned(),
        )
    })?;
    let ordinary_participant_acknowledgement_reference = normalized
        .ordinary_participant_acknowledgement_reference
        .as_deref()
        .ok_or_else(|| {
            PromiseCompletionWriterFactPersistenceError::BadRequest(
                "Promise completion mutual acknowledgement accepted transition requires Ordinary Account participant acknowledgement reference"
                    .to_owned(),
            )
        })?;
    let expected_fact_idempotency_key =
        prior_bound_accepted_transition_idempotency_key(&prior_writer_fact_id);

    // Bind consumption to the prior fact so the existing DB idempotency index is the atomic guard.
    if normalized.fact_idempotency_key != expected_fact_idempotency_key {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition requires prior-bound fact idempotency key"
                .to_owned(),
        ));
    }

    let row = client
        .query_opt(
            "
            SELECT
                promise_reference,
                realm_id,
                fact_family,
                source_route_class,
                completion_state_class,
                promise_terms_reference,
                participant_set_reference,
                ordinary_participant_acknowledgement_reference,
                policy_version
            FROM promise_completion.writer_fact_records
            WHERE writer_fact_id = $1
            ",
            &[&prior_writer_fact_id],
        )
        .await
        .map_err(db_error)?;

    let row = row.ok_or_else(|| {
        PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition requires existing prior writer fact posture"
                .to_owned(),
        )
    })?;
    let prior_promise_reference: String = row.get("promise_reference");
    let prior_realm_id: String = row.get("realm_id");
    let prior_fact_family: String = row.get("fact_family");
    let prior_source_route_class: String = row.get("source_route_class");
    let prior_completion_state_class: String = row.get("completion_state_class");
    let prior_promise_terms_reference: String = row.get("promise_terms_reference");
    let prior_participant_set_reference: String = row.get("participant_set_reference");
    let prior_ordinary_participant_acknowledgement_reference: Option<String> =
        row.get("ordinary_participant_acknowledgement_reference");
    let prior_policy_version: i32 = row.get("policy_version");

    if prior_promise_reference != normalized.promise_reference
        || prior_realm_id != normalized.realm_id
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition prior writer fact must match Promise reference and realm_id"
                .to_owned(),
        ));
    }

    if prior_promise_terms_reference != normalized.promise_terms_reference
        || prior_participant_set_reference != normalized.participant_set_reference
        || prior_ordinary_participant_acknowledgement_reference.as_deref()
            != Some(ordinary_participant_acknowledgement_reference)
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition prior writer fact must match Promise terms, participant set, and Ordinary Account acknowledgement references"
                .to_owned(),
        ));
    }

    if prior_policy_version != normalized.policy_version {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition prior writer fact must match policy version"
                .to_owned(),
        ));
    }

    let existing_transition = client
        .query_opt(
            "
            SELECT fact_idempotency_key, policy_version
            FROM promise_completion.writer_fact_records
            WHERE prior_writer_fact_id = $1
              AND fact_family = 'completion_state_transition'
              AND source_route_class = 'mutual_accountable_completion_acknowledgement'
              AND completion_state_class = 'completion_accepted'
            ORDER BY created_at ASC, writer_fact_id ASC
            LIMIT 1
            ",
            &[&prior_writer_fact_id],
        )
        .await
        .map_err(db_error)?;

    if let Some(existing_transition) = existing_transition {
        let existing_transition_idempotency_key: String =
            existing_transition.get("fact_idempotency_key");
        let existing_transition_policy_version: i32 = existing_transition.get("policy_version");
        if existing_transition_idempotency_key != normalized.fact_idempotency_key
            || existing_transition_policy_version != normalized.policy_version
        {
            return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
                "Promise completion mutual acknowledgement accepted transition prior writer fact is already consumed by another accepted transition"
                    .to_owned(),
            ));
        }
    }

    if prior_fact_family != PromiseCompletionWriterFactFamily::SourceRouteCandidate.as_str()
        || prior_source_route_class
            != PromiseCompletionSourceRouteClass::MutualAccountableCompletionAcknowledgement
                .as_str()
        || prior_completion_state_class
            != PromiseCompletionStateClass::CompletionPendingMutualAcknowledgement.as_str()
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement accepted transition prior writer fact must be mutual acknowledgement pending posture"
                .to_owned(),
        ));
    }

    Ok(())
}

fn prior_bound_accepted_transition_idempotency_key(prior_writer_fact_id: &Uuid) -> String {
    format!("completion-accepted-from-prior-{prior_writer_fact_id}")
}

fn normalize_writer_fact(
    fact: &ProposedPromiseCompletionWriterFact,
) -> Result<NormalizedWriterFact, PromiseCompletionWriterFactPersistenceError> {
    if !fact.source_route_class.is_allowed() {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion writer fact persistence rejects forbidden source route classes before persistence"
                .to_owned(),
        ));
    }

    if fact.completed_reference_eligible
        && fact.completion_state_class != PromiseCompletionStateClass::CompletionAccepted
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion completed reference eligibility requires completion_accepted"
                .to_owned(),
        ));
    }

    let promise_reference = required_ref(fact.promise_reference.as_deref(), "Promise reference")?;
    let realm_id = required_ref(fact.realm_id.as_deref(), "realm_id")?;
    let fact_family = fact.fact_family.as_str();
    let source_route_class = fact.source_route_class.as_str();
    let previous_completion_state_class = fact
        .previous_completion_state_class
        .map(|state| state.as_str());
    let completion_state_class = fact.completion_state_class.as_str();
    let promise_terms_reference = required_ref(
        fact.promise_terms_reference.as_deref(),
        "Promise terms reference",
    )?;
    let participant_set_reference = required_ref(
        fact.participant_set_reference.as_deref(),
        "participant set reference",
    )?;
    let ordinary_participant_acknowledgement_reference = optional_ref(
        fact.ordinary_participant_acknowledgement_reference
            .as_deref(),
        "Ordinary Account participant acknowledgement reference",
    )?;
    let governed_review_reference = optional_ref(
        fact.governed_review_reference.as_deref(),
        "governed review reference",
    )?;
    let review_authority_reference = optional_ref(
        fact.review_authority_reference.as_deref(),
        "review authority reference",
    )?;
    let proof_eligibility_reference = optional_ref(
        fact.proof_eligibility_reference.as_deref(),
        "Proof Eligibility reference",
    )?;
    let proof_evidence_writer_fact_reference = optional_ref(
        fact.proof_evidence_writer_fact_reference.as_deref(),
        "proof evidence writer fact reference",
    )?;
    let consent_at_formation_reference = required_ref(
        fact.consent_at_formation_reference.as_deref(),
        "Consent at Promise formation reference",
    )?;
    let consent_at_resolution_reference = required_ref(
        fact.consent_at_resolution_reference.as_deref(),
        "Consent at resolution reference",
    )?;
    let block_withdrawal_state_reference = required_ref(
        fact.block_withdrawal_state_reference.as_deref(),
        "block, mute, refusal, or Withdrawal state reference",
    )?;
    let age_assurance_state_reference = required_ref(
        fact.age_assurance_state_reference.as_deref(),
        "Age Assurance state reference",
    )?;
    let legal_hold_intersection_reference = required_ref(
        fact.legal_hold_intersection_reference.as_deref(),
        "Legal Hold intersection reference",
    )?;
    let critical_harm_case_reference = required_ref(
        fact.critical_harm_case_reference.as_deref(),
        "Critical Harm case reference",
    )?;
    let account_lifecycle_reference = required_ref(
        fact.account_lifecycle_reference.as_deref(),
        "account lifecycle reference",
    )?;
    let anti_abuse_continuity_reference = required_ref(
        fact.anti_abuse_continuity_reference.as_deref(),
        "Anti-Abuse Continuity Marker reference",
    )?;
    let safety_case_reference = required_ref(
        fact.safety_case_reference.as_deref(),
        "safety case reference",
    )?;
    let reason_code_class = required_ref(fact.reason_code_class.as_deref(), "reason-code class")?;
    let evidence_level_reference = required_ref(
        fact.evidence_level_reference.as_deref(),
        "evidence level reference",
    )?;
    let correction_or_supersession_reference = optional_ref(
        fact.correction_or_supersession_reference.as_deref(),
        "correction or supersession reference",
    )?;
    let prior_writer_fact_id =
        optional_uuid_ref(fact.prior_writer_fact_id.as_deref(), "prior writer fact id")?;
    let prior_writer_fact_id_hash = prior_writer_fact_id.map(|id| id.to_string());
    let policy_version = required_policy_version(fact.policy_version)?;
    let fact_idempotency_key =
        required_ref(fact.fact_idempotency_key.as_deref(), "fact idempotency key")?;
    let retention_class_reference = required_ref(
        fact.retention_class_reference.as_deref(),
        "retention class reference",
    )?;
    let access_audit_reference = required_ref(
        fact.access_audit_reference.as_deref(),
        "access-audit reference",
    )?;
    let projection_non_authority_posture =
        projection_non_authority_posture(fact.projection_non_authority_posture)?;
    let authority_posture = authority_posture(fact.authority_posture)?;

    if fact.source_route_class
        == PromiseCompletionSourceRouteClass::MutualAccountableCompletionAcknowledgement
        && ordinary_participant_acknowledgement_reference.is_none()
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion mutual acknowledgement route requires Ordinary Account participant acknowledgement reference"
                .to_owned(),
        ));
    }

    if fact.source_route_class == PromiseCompletionSourceRouteClass::GovernedReviewCompletion
        && (governed_review_reference.is_none() || review_authority_reference.is_none())
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion governed review route requires governed review and review authority references"
                .to_owned(),
        ));
    }

    if proof_eligibility_reference.is_some() != proof_evidence_writer_fact_reference.is_some() {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion proof references require both Proof Eligibility and proof evidence writer fact references"
                .to_owned(),
        ));
    }

    if fact.fact_family == PromiseCompletionWriterFactFamily::CorrectionOrSupersession
        && correction_or_supersession_reference.is_none()
        && prior_writer_fact_id.is_none()
    {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion correction or supersession facts require correction/supersession or prior writer fact reference"
                .to_owned(),
        ));
    }

    let policy_version_string = policy_version.to_string();
    let completed_reference_eligible = if fact.completed_reference_eligible {
        "true"
    } else {
        "false"
    };
    let (previous_state_presence, previous_state_value) =
        optional_static_hash_value(&previous_completion_state_class);
    let (ordinary_ack_presence, ordinary_ack_value) =
        optional_hash_value(&ordinary_participant_acknowledgement_reference);
    let (governed_review_presence, governed_review_value) =
        optional_hash_value(&governed_review_reference);
    let (review_authority_presence, review_authority_value) =
        optional_hash_value(&review_authority_reference);
    let (proof_eligibility_presence, proof_eligibility_value) =
        optional_hash_value(&proof_eligibility_reference);
    let (proof_evidence_presence, proof_evidence_value) =
        optional_hash_value(&proof_evidence_writer_fact_reference);
    let (correction_presence, correction_value) =
        optional_hash_value(&correction_or_supersession_reference);
    let (prior_fact_presence, prior_fact_value) = optional_hash_value(&prior_writer_fact_id_hash);

    let request_payload_hash = hash_parts(&[
        (
            "hash_kind",
            "promise_completion_writer_fact_persistence_request",
        ),
        ("policy_version", &policy_version_string),
        ("promise_reference", &promise_reference),
        ("realm_id", &realm_id),
        ("fact_family", fact_family),
        ("source_route_class", source_route_class),
        (
            "previous_completion_state_class_presence",
            previous_state_presence,
        ),
        ("previous_completion_state_class", previous_state_value),
        ("completion_state_class", completion_state_class),
        ("completed_reference_eligible", completed_reference_eligible),
        ("promise_terms_reference", &promise_terms_reference),
        ("participant_set_reference", &participant_set_reference),
        (
            "ordinary_participant_acknowledgement_reference_presence",
            ordinary_ack_presence,
        ),
        (
            "ordinary_participant_acknowledgement_reference",
            ordinary_ack_value,
        ),
        (
            "governed_review_reference_presence",
            governed_review_presence,
        ),
        ("governed_review_reference", governed_review_value),
        (
            "review_authority_reference_presence",
            review_authority_presence,
        ),
        ("review_authority_reference", review_authority_value),
        (
            "proof_eligibility_reference_presence",
            proof_eligibility_presence,
        ),
        ("proof_eligibility_reference", proof_eligibility_value),
        (
            "proof_evidence_writer_fact_reference_presence",
            proof_evidence_presence,
        ),
        ("proof_evidence_writer_fact_reference", proof_evidence_value),
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
        ("reason_code_class", &reason_code_class),
        ("evidence_level_reference", &evidence_level_reference),
        (
            "correction_or_supersession_reference_presence",
            correction_presence,
        ),
        ("correction_or_supersession_reference", correction_value),
        ("prior_writer_fact_id_presence", prior_fact_presence),
        ("prior_writer_fact_id", prior_fact_value),
        ("fact_idempotency_key", &fact_idempotency_key),
        ("retention_class_reference", &retention_class_reference),
        ("access_audit_reference", &access_audit_reference),
        (
            "projection_non_authority_posture",
            projection_non_authority_posture,
        ),
        ("authority_posture", authority_posture),
    ]);
    let decision_payload_hash = hash_parts(&[
        (
            "hash_kind",
            "promise_completion_writer_fact_persistence_decision",
        ),
        ("request_payload_hash", &request_payload_hash),
        ("decision_kind", DECISION_KIND),
        ("completion_state_class", completion_state_class),
        ("completed_reference_eligible", completed_reference_eligible),
    ]);

    Ok(NormalizedWriterFact {
        promise_reference,
        realm_id,
        fact_family,
        source_route_class,
        previous_completion_state_class,
        completion_state_class,
        completed_reference_eligible: fact.completed_reference_eligible,
        promise_terms_reference,
        participant_set_reference,
        ordinary_participant_acknowledgement_reference,
        governed_review_reference,
        review_authority_reference,
        proof_eligibility_reference,
        proof_evidence_writer_fact_reference,
        consent_at_formation_reference,
        consent_at_resolution_reference,
        block_withdrawal_state_reference,
        age_assurance_state_reference,
        legal_hold_intersection_reference,
        critical_harm_case_reference,
        account_lifecycle_reference,
        anti_abuse_continuity_reference,
        safety_case_reference,
        reason_code_class,
        evidence_level_reference,
        correction_or_supersession_reference,
        prior_writer_fact_id,
        policy_version,
        fact_idempotency_key,
        retention_class_reference,
        access_audit_reference,
        projection_non_authority_posture,
        authority_posture,
        request_payload_hash,
        decision_payload_hash,
    })
}

fn projection_non_authority_posture(
    posture: Option<PromiseCompletionProjectionNonAuthorityPosture>,
) -> Result<&'static str, PromiseCompletionWriterFactPersistenceError> {
    match posture {
        Some(PromiseCompletionProjectionNonAuthorityPosture::ProjectionNonAuthoritative) => {
            Ok(PromiseCompletionProjectionNonAuthorityPosture::ProjectionNonAuthoritative.as_str())
        }
        Some(PromiseCompletionProjectionNonAuthorityPosture::ProjectionAuthority) => Err(
            PromiseCompletionWriterFactPersistenceError::BadRequest(
                "Promise completion writer fact persistence requires projection non-authority posture"
                    .to_owned(),
            ),
        ),
        None => Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion writer fact persistence requires projection non-authority posture"
                .to_owned(),
        )),
    }
}

fn authority_posture(
    posture: Option<PromiseCompletionAuthorityPosture>,
) -> Result<&'static str, PromiseCompletionWriterFactPersistenceError> {
    match posture {
        Some(PromiseCompletionAuthorityPosture::WriterTruthOnly) => {
            Ok(PromiseCompletionAuthorityPosture::WriterTruthOnly.as_str())
        }
        Some(PromiseCompletionAuthorityPosture::ProjectionOnly) => Err(
            PromiseCompletionWriterFactPersistenceError::BadRequest(
                "Promise completion writer fact persistence requires writer truth authority posture"
                    .to_owned(),
            ),
        ),
        None => Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion writer fact persistence requires authority posture".to_owned(),
        )),
    }
}

fn required_policy_version(
    value: Option<i32>,
) -> Result<i32, PromiseCompletionWriterFactPersistenceError> {
    match value {
        Some(value) if value > 0 => Ok(value),
        _ => Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            "Promise completion writer fact persistence requires positive policy version"
                .to_owned(),
        )),
    }
}

fn required_ref(
    value: Option<&str>,
    label: &'static str,
) -> Result<String, PromiseCompletionWriterFactPersistenceError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            PromiseCompletionWriterFactPersistenceError::BadRequest(format!(
                "Promise completion writer fact persistence requires {label}"
            ))
        })
}

fn required_projection_ref(
    value: &str,
    label: &'static str,
) -> Result<String, PromiseCompletionWriterFactPersistenceError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
            format!("Promise completion non-authority projection read requires {label}"),
        ));
    }
    Ok(value.to_owned())
}

fn optional_ref(
    value: Option<&str>,
    label: &'static str,
) -> Result<Option<String>, PromiseCompletionWriterFactPersistenceError> {
    value
        .map(str::trim)
        .map(|value| {
            if value.is_empty() {
                Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
                    format!(
                        "Promise completion writer fact persistence requires {label} when provided"
                    ),
                ))
            } else {
                Ok(value.to_owned())
            }
        })
        .transpose()
}

fn optional_uuid_ref(
    value: Option<&str>,
    label: &'static str,
) -> Result<Option<Uuid>, PromiseCompletionWriterFactPersistenceError> {
    optional_ref(value, label)?
        .map(|value| {
            Uuid::parse_str(&value).map_err(|_| {
                PromiseCompletionWriterFactPersistenceError::BadRequest(format!(
                    "{label} must be a valid UUID"
                ))
            })
        })
        .transpose()
}

fn optional_hash_value(value: &Option<String>) -> (&'static str, &str) {
    match value {
        Some(value) => ("present", value.as_str()),
        None => ("absent", ""),
    }
}

fn optional_static_hash_value(value: &Option<&'static str>) -> (&'static str, &'static str) {
    match value {
        Some(value) => ("present", value),
        None => ("absent", ""),
    }
}

async fn find_existing_writer_fact_by_dedupe(
    client: &impl GenericClient,
    normalized: &NormalizedWriterFact,
) -> Result<Option<Row>, PromiseCompletionWriterFactPersistenceError> {
    client
        .query_opt(
            "
            SELECT writer_fact_id, request_payload_hash, decision_payload_hash
            FROM promise_completion.writer_fact_records
            WHERE realm_id = $1
              AND promise_reference = $2
              AND policy_version = $3
              AND fact_idempotency_key = $4
            ",
            &[
                &normalized.realm_id,
                &normalized.promise_reference,
                &normalized.policy_version,
                &normalized.fact_idempotency_key,
            ],
        )
        .await
        .map_err(db_error)
}

async fn load_snapshot_by_writer_fact_id(
    client: &impl GenericClient,
    writer_fact_id: &Uuid,
    replay_status: PromiseCompletionWriterFactReplayStatus,
) -> Result<PromiseCompletionWriterFactSnapshot, PromiseCompletionWriterFactPersistenceError> {
    let row = client
        .query_one(
            "
            SELECT
                writer_fact_id,
                promise_reference,
                realm_id,
                fact_family,
                source_route_class,
                completion_state_class,
                completed_reference_eligible,
                request_payload_hash,
                decision_payload_hash,
                created_at
            FROM promise_completion.writer_fact_records
            WHERE writer_fact_id = $1
            ",
            &[writer_fact_id],
        )
        .await
        .map_err(db_error)?;

    Ok(PromiseCompletionWriterFactSnapshot {
        writer_fact_id: row.get::<_, Uuid>("writer_fact_id").to_string(),
        promise_reference: row.get("promise_reference"),
        realm_id: row.get("realm_id"),
        fact_family: row.get("fact_family"),
        source_route_class: row.get("source_route_class"),
        completion_state_class: row.get("completion_state_class"),
        completed_reference_eligible: row.get("completed_reference_eligible"),
        request_payload_hash: row.get("request_payload_hash"),
        decision_payload_hash: row.get("decision_payload_hash"),
        replay_status,
        created_at: row.get("created_at"),
    })
}

async fn load_accepted_completion_non_authority_projection_snapshots(
    client: &impl GenericClient,
    promise_reference: &str,
    realm_id: &str,
) -> Result<
    Vec<PromiseCompletionNonAuthorityProjectionSnapshot>,
    PromiseCompletionWriterFactPersistenceError,
> {
    let rows = client
        .query(
            "
            WITH writer_truth_boundary AS (
                SELECT
                    writer_truth.writer_fact_id AS boundary_writer_fact_id,
                    writer_truth.prior_writer_fact_id AS prior_writer_fact_id,
                    writer_truth.promise_reference,
                    writer_truth.realm_id,
                    writer_truth.fact_family,
                    writer_truth.previous_completion_state_class,
                    writer_truth.promise_terms_reference,
                    writer_truth.participant_set_reference,
                    writer_truth.ordinary_participant_acknowledgement_reference,
                    writer_truth.source_route_class,
                    writer_truth.completion_state_class,
                    writer_truth.completed_reference_eligible,
                    writer_truth.fact_idempotency_key,
                    writer_truth.policy_version,
                    writer_truth.governed_review_reference,
                    writer_truth.review_authority_reference,
                    writer_truth.proof_eligibility_reference,
                    writer_truth.proof_evidence_writer_fact_reference,
                    writer_truth.correction_or_supersession_reference,
                    writer_truth.projection_non_authority_posture,
                    writer_truth.authority_posture,
                    writer_truth.created_at AS writer_recorded_at
                FROM promise_completion.writer_fact_records writer_truth
                WHERE writer_truth.promise_reference = $1
                  AND writer_truth.realm_id = $2
                  AND writer_truth.fact_family IN (
                          'completion_state_transition',
                          'completion_outcome_reference',
                          'correction_or_supersession',
                          'source_route_candidate'
                      )
            ),
            eligible AS (
                SELECT
                    accepted.boundary_writer_fact_id AS accepted_writer_fact_id,
                    accepted.prior_writer_fact_id,
                    accepted.promise_reference,
                    accepted.realm_id,
                    accepted.promise_terms_reference,
                    accepted.participant_set_reference,
                    accepted.source_route_class,
                    accepted.completion_state_class,
                    accepted.completed_reference_eligible,
                    accepted.policy_version,
                    accepted.projection_non_authority_posture,
                    accepted.authority_posture,
                    accepted.writer_recorded_at,
                    (
                        SELECT COUNT(*)
                        FROM promise_completion.writer_fact_records boundary_truth
                        WHERE boundary_truth.fact_family IN (
                                  'completion_state_transition',
                                  'completion_outcome_reference',
                                  'correction_or_supersession',
                                  'source_route_candidate'
                              )
                          AND (
                              (
                                  boundary_truth.promise_reference =
                                      accepted.promise_reference
                                  AND boundary_truth.realm_id = accepted.realm_id
                                  AND boundary_truth.promise_terms_reference =
                                      accepted.promise_terms_reference
                                  AND boundary_truth.participant_set_reference =
                                      accepted.participant_set_reference
                                  AND boundary_truth.policy_version =
                                      accepted.policy_version
                              )
                              OR boundary_truth.prior_writer_fact_id =
                                  accepted.boundary_writer_fact_id
                              OR boundary_truth.prior_writer_fact_id =
                                  prior.writer_fact_id
                          )
                          AND (
                              boundary_truth.fact_family <> 'source_route_candidate'
                              OR boundary_truth.writer_fact_id <>
                                  prior.writer_fact_id
                          )
                    ) AS boundary_writer_truth_count
                FROM writer_truth_boundary accepted
                JOIN promise_completion.writer_fact_records prior
                  ON prior.writer_fact_id = accepted.prior_writer_fact_id
                WHERE accepted.fact_family = 'completion_state_transition'
                  AND accepted.previous_completion_state_class =
                      'completion_pending_mutual_acknowledgement'
                  AND accepted.source_route_class =
                      'mutual_accountable_completion_acknowledgement'
                  AND accepted.completion_state_class = 'completion_accepted'
                  AND accepted.completed_reference_eligible = TRUE
                  AND accepted.prior_writer_fact_id IS NOT NULL
                  AND accepted.fact_idempotency_key =
                      'completion-accepted-from-prior-' ||
                      accepted.prior_writer_fact_id::text
                  AND accepted.projection_non_authority_posture =
                      'projection_non_authoritative'
                  AND accepted.authority_posture = 'writer_truth_only'
                  AND accepted.governed_review_reference IS NULL
                  AND accepted.review_authority_reference IS NULL
                  AND accepted.proof_eligibility_reference IS NULL
                  AND accepted.proof_evidence_writer_fact_reference IS NULL
                  AND accepted.correction_or_supersession_reference IS NULL
                  AND prior.promise_reference = accepted.promise_reference
                  AND prior.realm_id = accepted.realm_id
                  AND prior.promise_terms_reference = accepted.promise_terms_reference
                  AND prior.participant_set_reference = accepted.participant_set_reference
                  AND prior.ordinary_participant_acknowledgement_reference =
                      accepted.ordinary_participant_acknowledgement_reference
                  AND prior.policy_version = accepted.policy_version
                  AND prior.fact_family = 'source_route_candidate'
                  AND prior.source_route_class =
                      'mutual_accountable_completion_acknowledgement'
                  AND prior.completion_state_class =
                      'completion_pending_mutual_acknowledgement'
                  AND prior.governed_review_reference IS NULL
                  AND prior.review_authority_reference IS NULL
                  AND prior.proof_eligibility_reference IS NULL
                  AND prior.proof_evidence_writer_fact_reference IS NULL
                  AND prior.correction_or_supersession_reference IS NULL
            )
            SELECT
                accepted_writer_fact_id,
                prior_writer_fact_id,
                promise_reference,
                realm_id,
                promise_terms_reference,
                participant_set_reference,
                source_route_class,
                completion_state_class,
                completed_reference_eligible,
                policy_version,
                projection_non_authority_posture,
                authority_posture,
                writer_recorded_at,
                boundary_writer_truth_count
            FROM eligible
            ORDER BY writer_recorded_at ASC, accepted_writer_fact_id ASC
            ",
            &[&promise_reference, &realm_id],
        )
        .await
        .map_err(db_error)?;

    let mut snapshots = Vec::with_capacity(rows.len());
    for row in rows {
        let boundary_writer_truth_count: i64 = row.get("boundary_writer_truth_count");
        if boundary_writer_truth_count > 1 {
            return Err(PromiseCompletionWriterFactPersistenceError::BadRequest(
                "Promise completion non-authority projection refuses contradictory writer truth"
                    .to_owned(),
            ));
        }

        snapshots.push(PromiseCompletionNonAuthorityProjectionSnapshot {
            accepted_writer_fact_id: row.get::<_, Uuid>("accepted_writer_fact_id").to_string(),
            prior_writer_fact_id: row.get::<_, Uuid>("prior_writer_fact_id").to_string(),
            promise_reference: row.get("promise_reference"),
            realm_id: row.get("realm_id"),
            promise_terms_reference: row.get("promise_terms_reference"),
            participant_set_reference: row.get("participant_set_reference"),
            source_route_class: row.get("source_route_class"),
            completion_state_class: row.get("completion_state_class"),
            completed_reference_eligible: row.get("completed_reference_eligible"),
            policy_version: row.get("policy_version"),
            projection_non_authority_posture: row.get("projection_non_authority_posture"),
            authority_posture: row.get("authority_posture"),
            writer_recorded_at: row.get("writer_recorded_at"),
        });
    }

    Ok(snapshots)
}

fn replay_writer_fact_id(
    existing: Row,
    normalized: &NormalizedWriterFact,
) -> Result<Uuid, PromiseCompletionWriterFactPersistenceError> {
    let existing_request_hash: String = existing.get("request_payload_hash");
    let existing_decision_hash: String = existing.get("decision_payload_hash");
    let existing_writer_fact_id: Uuid = existing.get("writer_fact_id");
    if existing_request_hash != normalized.request_payload_hash
        || existing_decision_hash != normalized.decision_payload_hash
    {
        return Err(
            PromiseCompletionWriterFactPersistenceError::IdempotencyConflict {
                message: "duplicate Promise completion writer fact has payload drift".to_owned(),
                existing_writer_fact_id: existing_writer_fact_id.to_string(),
            },
        );
    }
    Ok(existing_writer_fact_id)
}

fn writer_fact_insert_error(
    error: tokio_postgres::Error,
) -> PromiseCompletionWriterFactPersistenceError {
    if matches!(
        error.code(),
        Some(&SqlState::FOREIGN_KEY_VIOLATION) | Some(&SqlState::CHECK_VIOLATION)
    ) {
        return PromiseCompletionWriterFactPersistenceError::BadRequest(error.to_string());
    }

    db_error(error)
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

fn db_error(error: tokio_postgres::Error) -> PromiseCompletionWriterFactPersistenceError {
    let code = error.code().map(SqlState::code).map(ToOwned::to_owned);
    let constraint = error
        .as_db_error()
        .and_then(|db_error| db_error.constraint())
        .map(ToOwned::to_owned);
    let retryable = matches!(
        error.code(),
        Some(&SqlState::T_R_SERIALIZATION_FAILURE) | Some(&SqlState::T_R_DEADLOCK_DETECTED)
    );

    PromiseCompletionWriterFactPersistenceError::Database {
        message: error.to_string(),
        code,
        constraint,
        retryable,
    }
}
