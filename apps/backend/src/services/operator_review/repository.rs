use std::sync::Arc;

use chrono::Utc;
use musubi_db_runtime::{DbConfig, connect_writer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_postgres::{Client, GenericClient, Row, Transaction, error::SqlState};
use uuid::Uuid;

use super::types::{
    AppealCaseSnapshot, AttachEvidenceBundleInput, CreateAppealCaseInput, CreateReviewCaseInput,
    EvidenceAccessGrantSnapshot, EvidenceBundleSnapshot, GrantEvidenceAccessInput,
    OperatorDecisionFactSnapshot, OperatorReviewError, OperatorRole, ReadReviewCaseSnapshot,
    RecordOperatorDecisionInput, ReviewCaseSnapshot, ReviewStatusReadModelSnapshot,
};

const READ_ROLES: &[OperatorRole] = &[
    OperatorRole::Reviewer,
    OperatorRole::Approver,
    OperatorRole::Steward,
    OperatorRole::Auditor,
    OperatorRole::Support,
];
const WRITE_REVIEW_ROLES: &[OperatorRole] = &[
    OperatorRole::Reviewer,
    OperatorRole::Approver,
    OperatorRole::Steward,
];

const CASE_TYPES: &[&str] = &[
    "proof_anomaly",
    "promise_dispute",
    "settlement_conflict",
    "safety_escalation",
    "realm_admission_review",
    "operator_manual_hold",
    "sealed_room_fallback",
    "appeal",
];
const SEVERITIES: &[&str] = &["sev0", "sev1", "sev2", "sev3"];
const EVIDENCE_VISIBILITIES: &[&str] = &["summary_only", "redacted_raw", "full_raw"];
const EVIDENCE_ACCESS_SCOPES: &[&str] = &["summary_only", "redacted_raw", "full_raw"];
const RETENTION_CLASSES: &[&str] = &["R4", "R6", "R7"];
const DECISION_KINDS: &[&str] = &[
    "no_action",
    "uphold",
    "restrict",
    "restore",
    "request_more_evidence",
    "escalate",
];
const USER_FACING_REASON_CODES: &[&str] = &[
    "verification_pending_review",
    "proof_rejected_expired",
    "promise_completion_under_review",
    "manual_hold_safety_review",
    "appeal_received",
    "proof_missing",
    "proof_inconclusive",
    "safety_review",
    "policy_review",
    "duplicate_or_invalid",
    "resolved_no_action",
    "restricted_after_review",
    "restored_after_review",
];

#[derive(Clone)]
pub struct OperatorReviewStore {
    client: Arc<Mutex<Client>>,
}

impl OperatorReviewStore {
    pub(crate) async fn connect(config: &DbConfig) -> musubi_db_runtime::Result<Self> {
        let client = connect_writer(config, "musubi-backend operator-review").await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }

    pub(crate) async fn reset_for_test(&self) -> Result<(), OperatorReviewError> {
        let client = self.client.lock().await;
        client
            .batch_execute(
                "
                TRUNCATE
                    projection.review_status_views,
                    dao.operator_decision_facts,
                    dao.appeal_cases,
                    dao.evidence_access_grants,
                    dao.evidence_bundles,
                    dao.review_cases,
                    core.operator_role_assignments
                RESTART IDENTITY CASCADE
                ",
            )
            .await
            .map_err(db_error)?;
        Ok(())
    }

    pub async fn create_review_case(
        &self,
        operator_id: &str,
        input: CreateReviewCaseInput,
    ) -> Result<ReviewCaseSnapshot, OperatorReviewError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        validate_allowed("case_type", &input.case_type, CASE_TYPES)?;
        validate_allowed("severity", &input.severity, SEVERITIES)?;
        validate_allowed(
            "opened_reason_code",
            &input.opened_reason_code,
            USER_FACING_REASON_CODES,
        )?;
        require_non_empty("source_fact_kind", &input.source_fact_kind)?;
        require_non_empty("source_fact_id", &input.source_fact_id)?;
        let subject_account_id =
            parse_optional_uuid(&input.subject_account_id, "subject account id")?;
        let related_promise_intent_id = parse_optional_uuid(
            &input.related_promise_intent_id,
            "related promise intent id",
        )?;
        let related_settlement_case_id = parse_optional_uuid(
            &input.related_settlement_case_id,
            "related settlement case id",
        )?;
        let assigned_operator_id =
            parse_optional_uuid(&input.assigned_operator_id, "assigned operator id")?;
        let request_idempotency_key = normalize_optional(&input.request_idempotency_key);
        let request_payload_hash = review_case_payload_hash(
            &input,
            &subject_account_id,
            &related_promise_intent_id,
            &related_settlement_case_id,
            &assigned_operator_id,
        );

        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        ensure_operator_has_any_role(&tx, &operator_id, WRITE_REVIEW_ROLES).await?;
        let review_case_id = Uuid::new_v4();
        let row = if request_idempotency_key.is_some() {
            if let Some(row) = tx
                .query_opt(
                    "
                    INSERT INTO dao.review_cases (
                        review_case_id,
                        case_type,
                        severity,
                        review_status,
                        subject_account_id,
                        related_promise_intent_id,
                        related_settlement_case_id,
                        related_realm_id,
                        opened_reason_code,
                        source_fact_kind,
                        source_fact_id,
                        source_snapshot_json,
                        assigned_operator_id,
                        opened_by_operator_id,
                        request_idempotency_key,
                        request_payload_hash
                    )
                    VALUES ($1, $2, $3, 'open', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                    ON CONFLICT (opened_by_operator_id, request_idempotency_key)
                    WHERE request_idempotency_key IS NOT NULL
                    DO NOTHING
                    RETURNING
                        review_case_id,
                        case_type,
                        severity,
                        review_status,
                        subject_account_id,
                        related_promise_intent_id,
                        related_settlement_case_id,
                        related_realm_id,
                        opened_reason_code,
                        source_fact_kind,
                        source_fact_id,
                        assigned_operator_id,
                        opened_at,
                        updated_at
                    ",
                    &[
                        &review_case_id,
                        &input.case_type,
                        &input.severity,
                        &subject_account_id,
                        &related_promise_intent_id,
                        &related_settlement_case_id,
                        &input.related_realm_id,
                        &input.opened_reason_code,
                        &input.source_fact_kind,
                        &input.source_fact_id,
                        &input.source_snapshot_json,
                        &assigned_operator_id,
                        &operator_id,
                        &request_idempotency_key,
                        &request_payload_hash,
                    ],
                )
                .await
                .map_err(db_error)?
            {
                refresh_review_status_view_tx(&tx, &review_case_id).await?;
                row
            } else {
                let row = find_existing_review_case_by_idempotency(
                    &tx,
                    &operator_id,
                    &request_idempotency_key,
                )
                .await?
                .ok_or_else(|| {
                    OperatorReviewError::Internal(
                        "review case idempotency row disappeared after conflict".to_owned(),
                    )
                })?;
                ensure_review_case_matches_payload_hash(&row, &request_payload_hash)?;
                row
            }
        } else {
            let row = tx
                .query_one(
                    "
                    INSERT INTO dao.review_cases (
                        review_case_id,
                        case_type,
                        severity,
                        review_status,
                        subject_account_id,
                        related_promise_intent_id,
                        related_settlement_case_id,
                        related_realm_id,
                        opened_reason_code,
                        source_fact_kind,
                        source_fact_id,
                        source_snapshot_json,
                        assigned_operator_id,
                        opened_by_operator_id,
                        request_idempotency_key,
                        request_payload_hash
                    )
                    VALUES ($1, $2, $3, 'open', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                    RETURNING
                        review_case_id,
                        case_type,
                        severity,
                        review_status,
                        subject_account_id,
                        related_promise_intent_id,
                        related_settlement_case_id,
                        related_realm_id,
                        opened_reason_code,
                        source_fact_kind,
                        source_fact_id,
                        assigned_operator_id,
                        opened_at,
                        updated_at
                    ",
                    &[
                        &review_case_id,
                        &input.case_type,
                        &input.severity,
                        &subject_account_id,
                        &related_promise_intent_id,
                        &related_settlement_case_id,
                        &input.related_realm_id,
                        &input.opened_reason_code,
                        &input.source_fact_kind,
                        &input.source_fact_id,
                        &input.source_snapshot_json,
                        &assigned_operator_id,
                        &operator_id,
                        &request_idempotency_key,
                        &request_payload_hash,
                    ],
                )
                .await
                .map_err(db_error)?;
            refresh_review_status_view_tx(&tx, &review_case_id).await?;
            row
        };
        let snapshot = review_case_from_row(&row);
        tx.commit().await.map_err(db_error)?;
        Ok(snapshot)
    }

    pub async fn list_review_cases(
        &self,
        operator_id: &str,
    ) -> Result<Vec<ReviewCaseSnapshot>, OperatorReviewError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        let client = self.client.lock().await;
        ensure_operator_has_any_role(&*client, &operator_id, READ_ROLES).await?;
        let rows = client
            .query(
                "
                SELECT
                    review_case_id,
                    case_type,
                    severity,
                    review_status,
                    subject_account_id,
                    related_promise_intent_id,
                    related_settlement_case_id,
                    related_realm_id,
                    opened_reason_code,
                    source_fact_kind,
                    source_fact_id,
                    assigned_operator_id,
                    opened_at,
                    updated_at
                FROM dao.review_cases
                ORDER BY opened_at DESC
                LIMIT 100
                ",
                &[],
            )
            .await
            .map_err(db_error)?;
        Ok(rows.iter().map(review_case_from_row).collect())
    }

    pub async fn read_review_case(
        &self,
        operator_id: &str,
        review_case_id: &str,
    ) -> Result<ReadReviewCaseSnapshot, OperatorReviewError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        let review_case_id = parse_uuid(review_case_id, "review case id")?;
        let client = self.client.lock().await;
        ensure_operator_has_any_role(&*client, &operator_id, READ_ROLES).await?;
        read_review_case_tx(&*client, &review_case_id).await
    }

    pub async fn attach_evidence_bundle(
        &self,
        operator_id: &str,
        review_case_id: &str,
        input: AttachEvidenceBundleInput,
    ) -> Result<EvidenceBundleSnapshot, OperatorReviewError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        let review_case_id = parse_uuid(review_case_id, "review case id")?;
        validate_allowed(
            "evidence_visibility",
            &input.evidence_visibility,
            EVIDENCE_VISIBILITIES,
        )?;
        validate_allowed("retention_class", &input.retention_class, RETENTION_CLASSES)?;

        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        ensure_operator_has_any_role(&tx, &operator_id, WRITE_REVIEW_ROLES).await?;
        ensure_review_case_exists_tx(&tx, &review_case_id).await?;
        let evidence_bundle_id = Uuid::new_v4();
        let row = tx
            .query_one(
                "
                INSERT INTO dao.evidence_bundles (
                    evidence_bundle_id,
                    review_case_id,
                    evidence_visibility,
                    summary_json,
                    raw_locator_json,
                    retention_class,
                    created_by_operator_id
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                RETURNING
                    evidence_bundle_id,
                    review_case_id,
                    evidence_visibility,
                    summary_json,
                    retention_class,
                    created_by_operator_id,
                    created_at
                ",
                &[
                    &evidence_bundle_id,
                    &review_case_id,
                    &input.evidence_visibility,
                    &input.summary_json,
                    &input.raw_locator_json,
                    &input.retention_class,
                    &operator_id,
                ],
            )
            .await
            .map_err(db_error)?;
        sync_review_case_status_tx(&tx, &review_case_id).await?;
        refresh_review_status_view_tx(&tx, &review_case_id).await?;
        tx.commit().await.map_err(db_error)?;
        Ok(evidence_bundle_from_row(&row))
    }

    pub async fn grant_evidence_access(
        &self,
        operator_id: &str,
        review_case_id: &str,
        input: GrantEvidenceAccessInput,
    ) -> Result<EvidenceAccessGrantSnapshot, OperatorReviewError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        let review_case_id = parse_uuid(review_case_id, "review case id")?;
        let evidence_bundle_id =
            parse_optional_uuid(&input.evidence_bundle_id, "evidence bundle id")?;
        let grantee_operator_id = parse_uuid(&input.grantee_operator_id, "grantee operator id")?;
        validate_allowed("access_scope", &input.access_scope, EVIDENCE_ACCESS_SCOPES)?;
        require_non_empty("grant_reason", &input.grant_reason)?;

        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        let db_now: chrono::DateTime<Utc> = tx
            .query_one("SELECT CURRENT_TIMESTAMP AS current_timestamp", &[])
            .await
            .map_err(db_error)?
            .get("current_timestamp");
        if input.expires_at <= db_now {
            return Err(OperatorReviewError::BadRequest(
                "expires_at must be in the future".to_owned(),
            ));
        }
        ensure_operator_has_role(&tx, &operator_id, OperatorRole::Approver).await?;
        ensure_review_case_exists_tx(&tx, &review_case_id).await?;
        if let Some(bundle_id) = evidence_bundle_id.as_ref() {
            ensure_evidence_bundle_belongs_to_case_tx(&tx, &bundle_id, &review_case_id).await?;
        }
        ensure_grantee_allowed_for_scope(&tx, &grantee_operator_id, &input.access_scope).await?;

        let access_grant_id = Uuid::new_v4();
        let row = tx
            .query_one(
                "
                INSERT INTO dao.evidence_access_grants (
                    access_grant_id,
                    review_case_id,
                    evidence_bundle_id,
                    grantee_operator_id,
                    access_scope,
                    grant_reason,
                    approved_by_operator_id,
                    expires_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                RETURNING
                    access_grant_id,
                    review_case_id,
                    evidence_bundle_id,
                    grantee_operator_id,
                    access_scope,
                    grant_reason,
                    approved_by_operator_id,
                    expires_at,
                    created_at
                ",
                &[
                    &access_grant_id,
                    &review_case_id,
                    &evidence_bundle_id,
                    &grantee_operator_id,
                    &input.access_scope,
                    &input.grant_reason,
                    &operator_id,
                    &input.expires_at,
                ],
            )
            .await
            .map_err(db_error)?;
        sync_review_case_status_tx(&tx, &review_case_id).await?;
        refresh_review_status_view_tx(&tx, &review_case_id).await?;
        tx.commit().await.map_err(db_error)?;
        Ok(evidence_access_grant_from_row(&row))
    }

    pub async fn record_operator_decision(
        &self,
        operator_id: &str,
        review_case_id: &str,
        input: RecordOperatorDecisionInput,
    ) -> Result<OperatorDecisionFactSnapshot, OperatorReviewError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        let review_case_id = parse_uuid(review_case_id, "review case id")?;
        validate_allowed("decision_kind", &input.decision_kind, DECISION_KINDS)?;
        validate_allowed(
            "user_facing_reason_code",
            &input.user_facing_reason_code,
            USER_FACING_REASON_CODES,
        )?;
        let decision_idempotency_key = normalize_optional(&input.decision_idempotency_key);
        let decision_payload_hash = operator_decision_payload_hash(&input);

        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        ensure_operator_has_role(&tx, &operator_id, OperatorRole::Approver).await?;
        ensure_review_case_exists_tx(&tx, &review_case_id).await?;

        let operator_decision_fact_id = Uuid::new_v4();
        let row = if decision_idempotency_key.is_some() {
            if let Some(row) = tx
                .query_opt(
                    "
                    INSERT INTO dao.operator_decision_facts (
                        operator_decision_fact_id,
                        review_case_id,
                        decision_kind,
                        user_facing_reason_code,
                        operator_note_internal,
                        decision_payload_json,
                        decided_by_operator_id,
                        decision_idempotency_key,
                        decision_payload_hash
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                    ON CONFLICT (review_case_id, decided_by_operator_id, decision_idempotency_key)
                    WHERE decision_idempotency_key IS NOT NULL
                    DO NOTHING
                    RETURNING
                        operator_decision_fact_id,
                        review_case_id,
                        appeal_case_id,
                        decision_kind,
                        user_facing_reason_code,
                        decided_by_operator_id,
                        recorded_at
                    ",
                    &[
                        &operator_decision_fact_id,
                        &review_case_id,
                        &input.decision_kind,
                        &input.user_facing_reason_code,
                        &input.operator_note_internal,
                        &input.decision_payload_json,
                        &operator_id,
                        &decision_idempotency_key,
                        &decision_payload_hash,
                    ],
                )
                .await
                .map_err(db_error)?
            {
                row
            } else {
                let row = find_existing_decision_by_idempotency(
                    &tx,
                    &review_case_id,
                    &operator_id,
                    &decision_idempotency_key,
                )
                .await?
                .ok_or_else(|| {
                    OperatorReviewError::Internal(
                        "operator decision idempotency row disappeared after conflict".to_owned(),
                    )
                })?;
                ensure_operator_decision_matches_payload_hash(&row, &decision_payload_hash)?;
                row
            }
        } else {
            tx.query_one(
                "
                INSERT INTO dao.operator_decision_facts (
                    operator_decision_fact_id,
                    review_case_id,
                    decision_kind,
                    user_facing_reason_code,
                    operator_note_internal,
                    decision_payload_json,
                    decided_by_operator_id,
                    decision_idempotency_key,
                    decision_payload_hash
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                RETURNING
                    operator_decision_fact_id,
                    review_case_id,
                    appeal_case_id,
                    decision_kind,
                    user_facing_reason_code,
                    decided_by_operator_id,
                    recorded_at
                ",
                &[
                    &operator_decision_fact_id,
                    &review_case_id,
                    &input.decision_kind,
                    &input.user_facing_reason_code,
                    &input.operator_note_internal,
                    &input.decision_payload_json,
                    &operator_id,
                    &decision_idempotency_key,
                    &decision_payload_hash,
                ],
            )
            .await
            .map_err(db_error)?
        };
        sync_review_case_status_tx(&tx, &review_case_id).await?;
        refresh_review_status_view_tx(&tx, &review_case_id).await?;
        tx.commit().await.map_err(db_error)?;
        Ok(operator_decision_fact_from_row(&row))
    }

    pub async fn create_appeal_case(
        &self,
        submitted_by_account_id: &str,
        review_case_id: &str,
        input: CreateAppealCaseInput,
    ) -> Result<AppealCaseSnapshot, OperatorReviewError> {
        let submitted_by_account_id =
            parse_uuid(submitted_by_account_id, "submitted by account id")?;
        let review_case_id = parse_uuid(review_case_id, "review case id")?;
        let source_decision_fact_id =
            parse_optional_uuid(&input.source_decision_fact_id, "source decision fact id")?;
        validate_allowed(
            "submitted_reason_code",
            &input.submitted_reason_code,
            USER_FACING_REASON_CODES,
        )?;
        let appeal_idempotency_key = normalize_optional(&input.appeal_idempotency_key);
        let appeal_request_payload_hash =
            appeal_payload_hash(&input, source_decision_fact_id.as_ref());

        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        ensure_subject_can_access_case_tx(&tx, &review_case_id, &submitted_by_account_id).await?;
        ensure_appeal_source_tx(&tx, &review_case_id, source_decision_fact_id.as_ref()).await?;

        let appeal_case_id = Uuid::new_v4();
        let row = if appeal_idempotency_key.is_some() {
            if let Some(row) = tx
                .query_opt(
                    "
                    INSERT INTO dao.appeal_cases (
                        appeal_case_id,
                        source_review_case_id,
                        source_decision_fact_id,
                        appeal_status,
                        submitted_by_account_id,
                        submitted_reason_code,
                        appellant_statement,
                        new_evidence_summary_json,
                        appeal_idempotency_key,
                        appeal_payload_hash
                    )
                    VALUES ($1, $2, $3, 'submitted', $4, $5, $6, $7, $8, $9)
                    ON CONFLICT (
                        source_review_case_id,
                        submitted_by_account_id,
                        appeal_idempotency_key
                    )
                    WHERE appeal_idempotency_key IS NOT NULL
                    DO NOTHING
                    RETURNING
                        appeal_case_id,
                        source_review_case_id,
                        source_decision_fact_id,
                        appeal_status,
                        submitted_by_account_id,
                        submitted_reason_code,
                        created_at,
                        updated_at
                    ",
                    &[
                        &appeal_case_id,
                        &review_case_id,
                        &source_decision_fact_id,
                        &submitted_by_account_id,
                        &input.submitted_reason_code,
                        &input.appellant_statement,
                        &input.new_evidence_summary_json,
                        &appeal_idempotency_key,
                        &appeal_request_payload_hash,
                    ],
                )
                .await
                .map_err(db_error)?
            {
                row
            } else {
                let row = find_existing_appeal_by_idempotency(
                    &tx,
                    &review_case_id,
                    &submitted_by_account_id,
                    &appeal_idempotency_key,
                )
                .await?
                .ok_or_else(|| {
                    OperatorReviewError::Internal(
                        "appeal idempotency row disappeared after conflict".to_owned(),
                    )
                })?;
                ensure_appeal_matches_payload_hash(&row, &appeal_request_payload_hash)?;
                row
            }
        } else {
            tx.query_one(
                "
                INSERT INTO dao.appeal_cases (
                    appeal_case_id,
                    source_review_case_id,
                    source_decision_fact_id,
                    appeal_status,
                    submitted_by_account_id,
                    submitted_reason_code,
                    appellant_statement,
                    new_evidence_summary_json,
                    appeal_idempotency_key,
                    appeal_payload_hash
                )
                VALUES ($1, $2, $3, 'submitted', $4, $5, $6, $7, $8, $9)
                RETURNING
                    appeal_case_id,
                    source_review_case_id,
                    source_decision_fact_id,
                    appeal_status,
                    submitted_by_account_id,
                    submitted_reason_code,
                    created_at,
                    updated_at
                ",
                &[
                    &appeal_case_id,
                    &review_case_id,
                    &source_decision_fact_id,
                    &submitted_by_account_id,
                    &input.submitted_reason_code,
                    &input.appellant_statement,
                    &input.new_evidence_summary_json,
                    &appeal_idempotency_key,
                    &appeal_request_payload_hash,
                ],
            )
            .await
            .map_err(db_error)?
        };
        sync_review_case_status_tx(&tx, &review_case_id).await?;
        refresh_review_status_view_tx(&tx, &review_case_id).await?;
        tx.commit().await.map_err(db_error)?;
        Ok(appeal_case_from_row(&row))
    }

    pub async fn list_appeal_cases_for_subject(
        &self,
        subject_account_id: &str,
        review_case_id: &str,
    ) -> Result<Vec<AppealCaseSnapshot>, OperatorReviewError> {
        let subject_account_id = parse_uuid(subject_account_id, "subject account id")?;
        let review_case_id = parse_uuid(review_case_id, "review case id")?;
        let client = self.client.lock().await;
        ensure_subject_can_access_case_tx(&*client, &review_case_id, &subject_account_id).await?;
        let rows = client
            .query(
                "
                SELECT
                    appeal_case_id,
                    source_review_case_id,
                    source_decision_fact_id,
                    appeal_status,
                    submitted_by_account_id,
                    submitted_reason_code,
                    created_at,
                    updated_at
                FROM dao.appeal_cases
                WHERE source_review_case_id = $1
                ORDER BY created_at ASC
                ",
                &[&review_case_id],
            )
            .await
            .map_err(db_error)?;
        Ok(rows.iter().map(appeal_case_from_row).collect())
    }

    pub async fn get_review_status_for_subject(
        &self,
        subject_account_id: &str,
        review_case_id: &str,
    ) -> Result<ReviewStatusReadModelSnapshot, OperatorReviewError> {
        let subject_account_id = parse_uuid(subject_account_id, "subject account id")?;
        let review_case_id = parse_uuid(review_case_id, "review case id")?;
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "
                SELECT
                    review_case_id,
                    subject_account_id,
                    related_promise_intent_id,
                    related_settlement_case_id,
                    related_realm_id,
                    user_facing_status,
                    user_facing_reason_code,
                    appeal_status,
                    evidence_requested,
                    appeal_available,
                    latest_decision_fact_id,
                    source_watermark_at,
                    source_fact_count,
                    last_projected_at
                FROM projection.review_status_views
                WHERE review_case_id = $1
                  AND subject_account_id = $2
                ",
                &[&review_case_id, &subject_account_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(|| {
                OperatorReviewError::NotFound("review status was not found".to_owned())
            })?;
        Ok(review_status_read_model_from_row(&row))
    }
}

async fn read_review_case_tx<C: GenericClient + Sync>(
    client: &C,
    review_case_id: &Uuid,
) -> Result<ReadReviewCaseSnapshot, OperatorReviewError> {
    let review_case = select_review_case_tx(client, review_case_id).await?;
    let evidence_bundles = client
        .query(
            "
            SELECT
                evidence_bundle_id,
                review_case_id,
                evidence_visibility,
                summary_json,
                retention_class,
                created_by_operator_id,
                created_at
            FROM dao.evidence_bundles
            WHERE review_case_id = $1
            ORDER BY created_at ASC
            ",
            &[review_case_id],
        )
        .await
        .map_err(db_error)?
        .iter()
        .map(evidence_bundle_from_row)
        .collect();
    let evidence_access_grants = client
        .query(
            "
            SELECT
                access_grant_id,
                review_case_id,
                evidence_bundle_id,
                grantee_operator_id,
                access_scope,
                grant_reason,
                approved_by_operator_id,
                expires_at,
                created_at
            FROM dao.evidence_access_grants
            WHERE review_case_id = $1
            ORDER BY created_at ASC
            ",
            &[review_case_id],
        )
        .await
        .map_err(db_error)?
        .iter()
        .map(evidence_access_grant_from_row)
        .collect();
    let operator_decision_facts = client
        .query(
            "
            SELECT
                operator_decision_fact_id,
                review_case_id,
                appeal_case_id,
                decision_kind,
                user_facing_reason_code,
                decided_by_operator_id,
                recorded_at
            FROM dao.operator_decision_facts
            WHERE review_case_id = $1
            ORDER BY recorded_at ASC
            ",
            &[review_case_id],
        )
        .await
        .map_err(db_error)?
        .iter()
        .map(operator_decision_fact_from_row)
        .collect();
    let appeal_cases = client
        .query(
            "
            SELECT
                appeal_case_id,
                source_review_case_id,
                source_decision_fact_id,
                appeal_status,
                submitted_by_account_id,
                submitted_reason_code,
                created_at,
                updated_at
            FROM dao.appeal_cases
            WHERE source_review_case_id = $1
            ORDER BY created_at ASC
            ",
            &[review_case_id],
        )
        .await
        .map_err(db_error)?
        .iter()
        .map(appeal_case_from_row)
        .collect();

    Ok(ReadReviewCaseSnapshot {
        review_case,
        evidence_bundles,
        evidence_access_grants,
        operator_decision_facts,
        appeal_cases,
    })
}

async fn find_existing_review_case_by_idempotency<C: GenericClient + Sync>(
    client: &C,
    operator_id: &Uuid,
    request_idempotency_key: &Option<String>,
) -> Result<Option<Row>, OperatorReviewError> {
    let Some(request_idempotency_key) = request_idempotency_key else {
        return Ok(None);
    };
    client
        .query_opt(
            "
            SELECT
                review_case_id,
                case_type,
                severity,
                review_status,
                subject_account_id,
                related_promise_intent_id,
                related_settlement_case_id,
                related_realm_id,
                opened_reason_code,
                source_fact_kind,
                source_fact_id,
                assigned_operator_id,
                opened_at,
                updated_at,
                request_payload_hash
            FROM dao.review_cases
            WHERE opened_by_operator_id = $1
              AND request_idempotency_key = $2
            ",
            &[operator_id, request_idempotency_key],
        )
        .await
        .map_err(db_error)
}

async fn find_existing_decision_by_idempotency<C: GenericClient + Sync>(
    client: &C,
    review_case_id: &Uuid,
    operator_id: &Uuid,
    decision_idempotency_key: &Option<String>,
) -> Result<Option<Row>, OperatorReviewError> {
    let Some(decision_idempotency_key) = decision_idempotency_key else {
        return Ok(None);
    };
    client
        .query_opt(
            "
            SELECT
                operator_decision_fact_id,
                review_case_id,
                appeal_case_id,
                decision_kind,
                user_facing_reason_code,
                decided_by_operator_id,
                recorded_at,
                decision_payload_hash
            FROM dao.operator_decision_facts
            WHERE review_case_id = $1
              AND decided_by_operator_id = $2
              AND decision_idempotency_key = $3
            ",
            &[review_case_id, operator_id, decision_idempotency_key],
        )
        .await
        .map_err(db_error)
}

async fn find_existing_appeal_by_idempotency<C: GenericClient + Sync>(
    client: &C,
    review_case_id: &Uuid,
    submitted_by_account_id: &Uuid,
    appeal_idempotency_key: &Option<String>,
) -> Result<Option<Row>, OperatorReviewError> {
    let Some(appeal_idempotency_key) = appeal_idempotency_key else {
        return Ok(None);
    };
    client
        .query_opt(
            "
            SELECT
                appeal_case_id,
                source_review_case_id,
                source_decision_fact_id,
                appeal_status,
                submitted_by_account_id,
                submitted_reason_code,
                created_at,
                updated_at,
                appeal_payload_hash
            FROM dao.appeal_cases
            WHERE source_review_case_id = $1
              AND submitted_by_account_id = $2
              AND appeal_idempotency_key = $3
            ",
            &[
                review_case_id,
                submitted_by_account_id,
                appeal_idempotency_key,
            ],
        )
        .await
        .map_err(db_error)
}

async fn ensure_operator_has_any_role<C: GenericClient + Sync>(
    client: &C,
    operator_id: &Uuid,
    roles: &[OperatorRole],
) -> Result<(), OperatorReviewError> {
    if roles.is_empty() {
        return Err(OperatorReviewError::Unauthorized(
            "operator role is not allowed for this action".to_owned(),
        ));
    }
    let role_names = roles.iter().map(|role| role.as_str()).collect::<Vec<_>>();
    let row = client
        .query_one(
            "
            SELECT EXISTS (
                SELECT 1
                FROM core.operator_role_assignments
                JOIN core.accounts
                  ON core.accounts.account_id = core.operator_role_assignments.operator_account_id
                WHERE operator_account_id = $1
                  AND operator_role = ANY($2::text[])
                  AND revoked_at IS NULL
                  AND core.accounts.account_class = 'Controlled Exceptional Account'
                  AND core.accounts.account_state = 'active'
            ) AS has_role
            ",
            &[operator_id, &role_names],
        )
        .await
        .map_err(db_error)?;
    if row.get("has_role") {
        return Ok(());
    }
    Err(OperatorReviewError::Unauthorized(
        "operator role is not allowed for this action".to_owned(),
    ))
}

async fn ensure_operator_has_role<C: GenericClient + Sync>(
    client: &C,
    operator_id: &Uuid,
    role: OperatorRole,
) -> Result<(), OperatorReviewError> {
    if operator_has_role(client, operator_id, role).await? {
        return Ok(());
    }
    Err(OperatorReviewError::Unauthorized(
        "operator role is not allowed for this action".to_owned(),
    ))
}

async fn operator_has_role<C: GenericClient + Sync>(
    client: &C,
    operator_id: &Uuid,
    role: OperatorRole,
) -> Result<bool, OperatorReviewError> {
    let row = client
        .query_one(
            "
            SELECT EXISTS (
                SELECT 1
                FROM core.operator_role_assignments
                JOIN core.accounts
                  ON core.accounts.account_id = core.operator_role_assignments.operator_account_id
                WHERE operator_account_id = $1
                  AND operator_role = $2
                  AND revoked_at IS NULL
                  AND core.accounts.account_class = 'Controlled Exceptional Account'
                  AND core.accounts.account_state = 'active'
            ) AS has_role
            ",
            &[operator_id, &role.as_str()],
        )
        .await
        .map_err(db_error)?;
    Ok(row.get("has_role"))
}

async fn ensure_grantee_allowed_for_scope(
    tx: &Transaction<'_>,
    grantee_operator_id: &Uuid,
    access_scope: &str,
) -> Result<(), OperatorReviewError> {
    let allowed_roles: &[OperatorRole] = match access_scope {
        "summary_only" => READ_ROLES,
        "redacted_raw" => &[
            OperatorRole::Reviewer,
            OperatorRole::Approver,
            OperatorRole::Steward,
            OperatorRole::Auditor,
        ],
        "full_raw" => &[OperatorRole::Approver, OperatorRole::Auditor],
        _ => {
            return Err(OperatorReviewError::BadRequest(
                "unsupported access_scope".to_owned(),
            ));
        }
    };
    ensure_operator_has_any_role(tx, grantee_operator_id, allowed_roles).await
}

async fn ensure_review_case_exists_tx<C: GenericClient + Sync>(
    client: &C,
    review_case_id: &Uuid,
) -> Result<(), OperatorReviewError> {
    let exists: bool = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM dao.review_cases WHERE review_case_id = $1) AS exists",
            &[review_case_id],
        )
        .await
        .map_err(db_error)?
        .get("exists");
    if exists {
        Ok(())
    } else {
        Err(OperatorReviewError::NotFound(
            "review case was not found".to_owned(),
        ))
    }
}

async fn ensure_evidence_bundle_belongs_to_case_tx<C: GenericClient + Sync>(
    client: &C,
    evidence_bundle_id: &Uuid,
    review_case_id: &Uuid,
) -> Result<(), OperatorReviewError> {
    let exists: bool = client
        .query_one(
            "
            SELECT EXISTS (
                SELECT 1
                FROM dao.evidence_bundles
                WHERE evidence_bundle_id = $1
                  AND review_case_id = $2
            ) AS exists
            ",
            &[evidence_bundle_id, review_case_id],
        )
        .await
        .map_err(db_error)?
        .get("exists");
    if exists {
        Ok(())
    } else {
        Err(OperatorReviewError::BadRequest(
            "evidence_bundle_id must belong to the review case".to_owned(),
        ))
    }
}

async fn ensure_subject_can_access_case_tx<C: GenericClient + Sync>(
    client: &C,
    review_case_id: &Uuid,
    subject_account_id: &Uuid,
) -> Result<(), OperatorReviewError> {
    let exists: bool = client
        .query_one(
            "
            SELECT EXISTS (
                SELECT 1
                FROM dao.review_cases
                WHERE review_case_id = $1
                  AND subject_account_id = $2
            ) AS exists
            ",
            &[review_case_id, subject_account_id],
        )
        .await
        .map_err(db_error)?
        .get("exists");
    if exists {
        Ok(())
    } else {
        Err(OperatorReviewError::NotFound(
            "review case was not found".to_owned(),
        ))
    }
}

async fn ensure_appeal_source_tx<C: GenericClient + Sync>(
    client: &C,
    review_case_id: &Uuid,
    source_decision_fact_id: Option<&Uuid>,
) -> Result<(), OperatorReviewError> {
    if let Some(source_decision_fact_id) = source_decision_fact_id {
        let source_decision = client
            .query_opt(
                "
                SELECT decision_kind
                FROM dao.operator_decision_facts
                WHERE operator_decision_fact_id = $1
                  AND review_case_id = $2
                ",
                &[source_decision_fact_id, review_case_id],
            )
            .await
            .map_err(db_error)?;
        let Some(source_decision) = source_decision else {
            return Err(OperatorReviewError::BadRequest(
                "source_decision_fact_id must belong to the review case".to_owned(),
            ));
        };
        let decision_kind: String = source_decision.get("decision_kind");
        if decision_kind == "request_more_evidence" {
            return Err(OperatorReviewError::BadRequest(
                "appeal cannot reference a non-final decision".to_owned(),
            ));
        }
        return Ok(());
    }

    let status: String = client
        .query_one(
            "
            SELECT review_status
            FROM dao.review_cases
            WHERE review_case_id = $1
            ",
            &[review_case_id],
        )
        .await
        .map_err(db_error)?
        .get("review_status");
    if matches!(status.as_str(), "decided" | "appealed" | "closed") {
        Ok(())
    } else {
        Err(OperatorReviewError::BadRequest(
            "appeal must reference an original review decision or a decided review case".to_owned(),
        ))
    }
}

async fn select_review_case_tx<C: GenericClient + Sync>(
    client: &C,
    review_case_id: &Uuid,
) -> Result<ReviewCaseSnapshot, OperatorReviewError> {
    let row = client
        .query_opt(
            "
            SELECT
                review_case_id,
                case_type,
                severity,
                review_status,
                subject_account_id,
                related_promise_intent_id,
                related_settlement_case_id,
                related_realm_id,
                opened_reason_code,
                source_fact_kind,
                source_fact_id,
                assigned_operator_id,
                opened_at,
                updated_at
            FROM dao.review_cases
            WHERE review_case_id = $1
            ",
            &[review_case_id],
        )
        .await
        .map_err(db_error)?
        .ok_or_else(|| OperatorReviewError::NotFound("review case was not found".to_owned()))?;
    Ok(review_case_from_row(&row))
}

async fn refresh_review_status_view_tx<C: GenericClient + Sync>(
    client: &C,
    review_case_id: &Uuid,
) -> Result<(), OperatorReviewError> {
    client
        .execute(
            "
            INSERT INTO projection.review_status_views (
                review_case_id,
                subject_account_id,
                related_promise_intent_id,
                related_settlement_case_id,
                related_realm_id,
                user_facing_status,
                user_facing_reason_code,
                appeal_status,
                latest_decision_fact_id,
                evidence_requested,
                appeal_available,
                source_watermark_at,
                source_fact_count,
                last_projected_at
            )
            WITH latest_decision AS (
                SELECT
                    operator_decision_fact_id,
                    decision_kind,
                    user_facing_reason_code,
                    recorded_at
                FROM dao.operator_decision_facts
                WHERE review_case_id = $1
                ORDER BY recorded_at DESC, operator_decision_fact_id DESC
                LIMIT 1
            ),
            latest_appeal AS (
                SELECT
                    appeal_case_id,
                    appeal_status,
                    submitted_reason_code,
                    updated_at
                FROM dao.appeal_cases
                WHERE source_review_case_id = $1
                ORDER BY updated_at DESC, appeal_case_id DESC
                LIMIT 1
            ),
            latest_evidence AS (
                SELECT created_at
                FROM dao.evidence_bundles
                WHERE review_case_id = $1
                ORDER BY created_at DESC, evidence_bundle_id DESC
                LIMIT 1
            ),
            latest_grant AS (
                SELECT created_at
                FROM dao.evidence_access_grants
                WHERE review_case_id = $1
                ORDER BY created_at DESC, access_grant_id DESC
                LIMIT 1
            ),
            fact_counts AS (
                SELECT
                    (1
                     + (SELECT count(*) FROM dao.operator_decision_facts WHERE review_case_id = $1)
                     + (SELECT count(*) FROM dao.appeal_cases WHERE source_review_case_id = $1)
                     + (SELECT count(*) FROM dao.evidence_bundles WHERE review_case_id = $1)
                     + (SELECT count(*) FROM dao.evidence_access_grants WHERE review_case_id = $1)
                    )::bigint AS source_fact_count
            ),
            shaped AS (
                SELECT
                    review.review_case_id,
                    review.subject_account_id,
                    review.related_promise_intent_id,
                    review.related_settlement_case_id,
                    review.related_realm_id,
                    latest_decision.operator_decision_fact_id AS latest_decision_fact_id,
                    CASE
                        WHEN review.review_status = 'closed' THEN 'closed'
                        WHEN latest_appeal.appeal_status IN ('submitted', 'accepted', 'under_review')
                            OR review.review_status = 'appealed'
                            THEN 'appeal_submitted'
                        WHEN latest_decision.decision_kind IN ('restrict', 'escalate')
                            THEN 'sealed_or_restricted'
                        WHEN latest_decision.decision_kind = 'request_more_evidence'
                            OR (
                                review.review_status = 'awaiting_evidence'
                                AND latest_decision.operator_decision_fact_id IS NULL
                            )
                            THEN 'evidence_requested'
                        WHEN review.review_status IN ('open', 'triaged') THEN 'pending_review'
                        WHEN review.review_status = 'under_review' THEN 'under_review'
                        WHEN latest_decision.operator_decision_fact_id IS NOT NULL
                            AND latest_decision.decision_kind <> 'request_more_evidence'
                            AND latest_appeal.appeal_case_id IS NULL
                            THEN 'appeal_available'
                        WHEN review.review_status = 'decided' THEN 'decided'
                        ELSE 'pending_review'
                    END AS user_facing_status,
                    COALESCE(
                        latest_appeal.submitted_reason_code,
                        latest_decision.user_facing_reason_code,
                        review.opened_reason_code
                    ) AS user_facing_reason_code,
                    CASE
                        WHEN latest_appeal.appeal_status = 'submitted' THEN 'submitted'
                        WHEN latest_appeal.appeal_status = 'accepted' THEN 'under_review'
                        WHEN latest_appeal.appeal_status = 'under_review' THEN 'under_review'
                        WHEN latest_appeal.appeal_status = 'decided' THEN 'decided'
                        WHEN latest_appeal.appeal_status = 'closed' THEN 'closed'
                        WHEN latest_decision.operator_decision_fact_id IS NOT NULL
                            AND latest_decision.decision_kind <> 'request_more_evidence'
                            AND latest_appeal.appeal_case_id IS NULL
                            THEN 'appeal_available'
                        ELSE 'none'
                    END AS appeal_status,
                    (
                        COALESCE(latest_decision.decision_kind = 'request_more_evidence', FALSE)
                        OR (
                            review.review_status = 'awaiting_evidence'
                            AND latest_decision.operator_decision_fact_id IS NULL
                        )
                    ) AS evidence_requested,
                    (
                        review.review_status <> 'closed'
                        AND latest_decision.operator_decision_fact_id IS NOT NULL
                        AND latest_decision.decision_kind <> 'request_more_evidence'
                        AND latest_appeal.appeal_case_id IS NULL
                    ) AS appeal_available,
                    GREATEST(
                        review.updated_at,
                        COALESCE(latest_decision.recorded_at, review.updated_at),
                        COALESCE(latest_appeal.updated_at, review.updated_at),
                        COALESCE(latest_evidence.created_at, review.updated_at),
                        COALESCE(latest_grant.created_at, review.updated_at)
                    ) AS source_watermark_at,
                    fact_counts.source_fact_count
                FROM dao.review_cases review
                LEFT JOIN latest_decision ON TRUE
                LEFT JOIN latest_appeal ON TRUE
                LEFT JOIN latest_evidence ON TRUE
                LEFT JOIN latest_grant ON TRUE
                CROSS JOIN fact_counts
                WHERE review.review_case_id = $1
            )
            SELECT
                review_case_id,
                subject_account_id,
                related_promise_intent_id,
                related_settlement_case_id,
                related_realm_id,
                user_facing_status,
                user_facing_reason_code,
                appeal_status,
                latest_decision_fact_id,
                evidence_requested,
                appeal_available,
                source_watermark_at,
                source_fact_count,
                CURRENT_TIMESTAMP
            FROM shaped
            ON CONFLICT (review_case_id) DO UPDATE
            SET subject_account_id = EXCLUDED.subject_account_id,
                related_promise_intent_id = EXCLUDED.related_promise_intent_id,
                related_settlement_case_id = EXCLUDED.related_settlement_case_id,
                related_realm_id = EXCLUDED.related_realm_id,
                user_facing_status = EXCLUDED.user_facing_status,
                user_facing_reason_code = EXCLUDED.user_facing_reason_code,
                appeal_status = EXCLUDED.appeal_status,
                latest_decision_fact_id = EXCLUDED.latest_decision_fact_id,
                evidence_requested = EXCLUDED.evidence_requested,
                appeal_available = EXCLUDED.appeal_available,
                source_watermark_at = EXCLUDED.source_watermark_at,
                source_fact_count = EXCLUDED.source_fact_count,
                last_projected_at = EXCLUDED.last_projected_at
            ",
            &[review_case_id],
        )
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn sync_review_case_status_tx<C: GenericClient + Sync>(
    client: &C,
    review_case_id: &Uuid,
) -> Result<(), OperatorReviewError> {
    client
        .execute(
            "
            WITH latest_decision AS (
                SELECT decision_kind
                FROM dao.operator_decision_facts
                WHERE review_case_id = $1
                ORDER BY recorded_at DESC, operator_decision_fact_id DESC
                LIMIT 1
            ),
            latest_appeal AS (
                SELECT appeal_status
                FROM dao.appeal_cases
                WHERE source_review_case_id = $1
                ORDER BY updated_at DESC, appeal_case_id DESC
                LIMIT 1
            ),
            next_status AS (
                SELECT
                    CASE
                        WHEN review.review_status = 'closed' THEN 'closed'
                        WHEN EXISTS (
                            SELECT 1
                            FROM latest_appeal
                            WHERE appeal_status IN ('submitted', 'accepted', 'under_review')
                        ) THEN 'appealed'
                        WHEN (SELECT decision_kind FROM latest_decision) = 'request_more_evidence'
                            THEN 'awaiting_evidence'
                        WHEN EXISTS (SELECT 1 FROM latest_decision) THEN 'decided'
                        ELSE review.review_status
                    END AS review_status
                FROM dao.review_cases review
                WHERE review.review_case_id = $1
            )
            UPDATE dao.review_cases review
            SET review_status = next_status.review_status,
                updated_at = CASE
                    WHEN review.review_status IS DISTINCT FROM next_status.review_status
                        THEN CURRENT_TIMESTAMP
                    ELSE review.updated_at
                END
            FROM next_status
            WHERE review.review_case_id = $1
            ",
            &[review_case_id],
        )
        .await
        .map_err(db_error)?;
    Ok(())
}

fn review_case_payload_hash(
    input: &CreateReviewCaseInput,
    subject_account_id: &Option<Uuid>,
    related_promise_intent_id: &Option<Uuid>,
    related_settlement_case_id: &Option<Uuid>,
    assigned_operator_id: &Option<Uuid>,
) -> String {
    hash_json_value(&json!({
        "schema_version": 1,
        "case_type": &input.case_type,
        "severity": &input.severity,
        "subject_account_id": optional_uuid_hash_value(subject_account_id),
        "related_promise_intent_id": optional_uuid_hash_value(related_promise_intent_id),
        "related_settlement_case_id": optional_uuid_hash_value(related_settlement_case_id),
        "related_realm_id": normalize_optional(&input.related_realm_id),
        "opened_reason_code": &input.opened_reason_code,
        "source_fact_kind": &input.source_fact_kind,
        "source_fact_id": &input.source_fact_id,
        "source_snapshot_json": &input.source_snapshot_json,
        "assigned_operator_id": optional_uuid_hash_value(assigned_operator_id),
    }))
}

fn operator_decision_payload_hash(input: &RecordOperatorDecisionInput) -> String {
    hash_json_value(&json!({
        "schema_version": 1,
        "decision_kind": &input.decision_kind,
        "user_facing_reason_code": &input.user_facing_reason_code,
        "operator_note_internal": &input.operator_note_internal,
        "decision_payload_json": &input.decision_payload_json,
    }))
}

fn appeal_payload_hash(
    input: &CreateAppealCaseInput,
    source_decision_fact_id: Option<&Uuid>,
) -> String {
    hash_json_value(&json!({
        "schema_version": 1,
        "source_decision_fact_id": source_decision_fact_id.map(|value| value.to_string()),
        "submitted_reason_code": &input.submitted_reason_code,
        "appellant_statement": &input.appellant_statement,
        "new_evidence_summary_json": &input.new_evidence_summary_json,
    }))
}

fn optional_uuid_hash_value(value: &Option<Uuid>) -> Option<String> {
    value.map(|value| value.to_string())
}

fn hash_json_value(value: &Value) -> String {
    let digest = Sha256::digest(value.to_string().as_bytes());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn ensure_review_case_matches_payload_hash(
    row: &Row,
    request_payload_hash: &str,
) -> Result<(), OperatorReviewError> {
    let existing_hash: Option<String> = row.get("request_payload_hash");
    if existing_hash.as_deref() != Some(request_payload_hash) {
        return Err(OperatorReviewError::BadRequest(
            "request_idempotency_key was already used with a different review case payload"
                .to_owned(),
        ));
    }
    Ok(())
}

fn ensure_operator_decision_matches_payload_hash(
    row: &Row,
    decision_payload_hash: &str,
) -> Result<(), OperatorReviewError> {
    let existing_hash: Option<String> = row.get("decision_payload_hash");
    if existing_hash.as_deref() != Some(decision_payload_hash) {
        return Err(OperatorReviewError::BadRequest(
            "decision_idempotency_key was already used with a different operator decision payload"
                .to_owned(),
        ));
    }
    Ok(())
}

fn ensure_appeal_matches_payload_hash(
    row: &Row,
    appeal_payload_hash: &str,
) -> Result<(), OperatorReviewError> {
    let existing_hash: Option<String> = row.get("appeal_payload_hash");
    if existing_hash.as_deref() != Some(appeal_payload_hash) {
        return Err(OperatorReviewError::BadRequest(
            "appeal_idempotency_key was already used with a different appeal payload".to_owned(),
        ));
    }
    Ok(())
}

fn review_case_from_row(row: &Row) -> ReviewCaseSnapshot {
    ReviewCaseSnapshot {
        review_case_id: row.get::<_, Uuid>("review_case_id").to_string(),
        case_type: row.get("case_type"),
        severity: row.get("severity"),
        review_status: row.get("review_status"),
        subject_account_id: optional_uuid_to_string(row.get("subject_account_id")),
        related_promise_intent_id: optional_uuid_to_string(row.get("related_promise_intent_id")),
        related_settlement_case_id: optional_uuid_to_string(row.get("related_settlement_case_id")),
        related_realm_id: row.get("related_realm_id"),
        opened_reason_code: row.get("opened_reason_code"),
        source_fact_kind: row.get("source_fact_kind"),
        source_fact_id: row.get("source_fact_id"),
        assigned_operator_id: optional_uuid_to_string(row.get("assigned_operator_id")),
        opened_at: row.get("opened_at"),
        updated_at: row.get("updated_at"),
    }
}

fn evidence_bundle_from_row(row: &Row) -> EvidenceBundleSnapshot {
    EvidenceBundleSnapshot {
        evidence_bundle_id: row.get::<_, Uuid>("evidence_bundle_id").to_string(),
        review_case_id: row.get::<_, Uuid>("review_case_id").to_string(),
        evidence_visibility: row.get("evidence_visibility"),
        summary_json: row.get("summary_json"),
        retention_class: row.get("retention_class"),
        created_by_operator_id: optional_uuid_to_string(row.get("created_by_operator_id")),
        created_at: row.get("created_at"),
    }
}

fn evidence_access_grant_from_row(row: &Row) -> EvidenceAccessGrantSnapshot {
    EvidenceAccessGrantSnapshot {
        access_grant_id: row.get::<_, Uuid>("access_grant_id").to_string(),
        review_case_id: row.get::<_, Uuid>("review_case_id").to_string(),
        evidence_bundle_id: optional_uuid_to_string(row.get("evidence_bundle_id")),
        grantee_operator_id: row.get::<_, Uuid>("grantee_operator_id").to_string(),
        access_scope: row.get("access_scope"),
        grant_reason: row.get("grant_reason"),
        approved_by_operator_id: row.get::<_, Uuid>("approved_by_operator_id").to_string(),
        expires_at: row.get("expires_at"),
        created_at: row.get("created_at"),
    }
}

fn operator_decision_fact_from_row(row: &Row) -> OperatorDecisionFactSnapshot {
    OperatorDecisionFactSnapshot {
        operator_decision_fact_id: row.get::<_, Uuid>("operator_decision_fact_id").to_string(),
        review_case_id: row.get::<_, Uuid>("review_case_id").to_string(),
        appeal_case_id: optional_uuid_to_string(row.get("appeal_case_id")),
        decision_kind: row.get("decision_kind"),
        user_facing_reason_code: row.get("user_facing_reason_code"),
        decided_by_operator_id: row.get::<_, Uuid>("decided_by_operator_id").to_string(),
        recorded_at: row.get("recorded_at"),
    }
}

fn appeal_case_from_row(row: &Row) -> AppealCaseSnapshot {
    AppealCaseSnapshot {
        appeal_case_id: row.get::<_, Uuid>("appeal_case_id").to_string(),
        source_review_case_id: row.get::<_, Uuid>("source_review_case_id").to_string(),
        source_decision_fact_id: optional_uuid_to_string(row.get("source_decision_fact_id")),
        appeal_status: row.get("appeal_status"),
        submitted_by_account_id: row.get::<_, Uuid>("submitted_by_account_id").to_string(),
        submitted_reason_code: row.get("submitted_reason_code"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn review_status_read_model_from_row(row: &Row) -> ReviewStatusReadModelSnapshot {
    ReviewStatusReadModelSnapshot {
        review_case_id: row.get::<_, Uuid>("review_case_id").to_string(),
        subject_account_id: optional_uuid_to_string(row.get("subject_account_id")),
        related_promise_intent_id: optional_uuid_to_string(row.get("related_promise_intent_id")),
        related_settlement_case_id: optional_uuid_to_string(row.get("related_settlement_case_id")),
        related_realm_id: row.get("related_realm_id"),
        user_facing_status: row.get("user_facing_status"),
        user_facing_reason_code: row.get("user_facing_reason_code"),
        appeal_status: row.get("appeal_status"),
        evidence_requested: row.get("evidence_requested"),
        appeal_available: row.get("appeal_available"),
        latest_decision_fact_id: optional_uuid_to_string(row.get("latest_decision_fact_id")),
        source_watermark_at: row.get("source_watermark_at"),
        source_fact_count: row.get("source_fact_count"),
        last_projected_at: row.get("last_projected_at"),
    }
}

fn parse_uuid(value: &str, label: &str) -> Result<Uuid, OperatorReviewError> {
    Uuid::parse_str(value.trim())
        .map_err(|_| OperatorReviewError::BadRequest(format!("{label} must be a valid UUID")))
}

fn parse_optional_uuid(
    value: &Option<String>,
    label: &str,
) -> Result<Option<Uuid>, OperatorReviewError> {
    value
        .as_ref()
        .map(|value| parse_uuid(value, label))
        .transpose()
}

fn optional_uuid_to_string(value: Option<Uuid>) -> Option<String> {
    value.map(|value| value.to_string())
}

fn validate_allowed(label: &str, value: &str, allowed: &[&str]) -> Result<(), OperatorReviewError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(OperatorReviewError::BadRequest(format!(
            "{label} is not supported"
        )))
    }
}

fn require_non_empty(label: &str, value: &str) -> Result<(), OperatorReviewError> {
    if value.trim().is_empty() {
        Err(OperatorReviewError::BadRequest(format!(
            "{label} is required"
        )))
    } else {
        Ok(())
    }
}

fn normalize_optional(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn db_error(error: tokio_postgres::Error) -> OperatorReviewError {
    let code = error.code().map(SqlState::code).map(str::to_owned);
    let retryable = matches!(
        error.code(),
        Some(&SqlState::T_R_SERIALIZATION_FAILURE)
            | Some(&SqlState::T_R_DEADLOCK_DETECTED)
            | Some(&SqlState::CONNECTION_EXCEPTION)
            | Some(&SqlState::CONNECTION_DOES_NOT_EXIST)
            | Some(&SqlState::CONNECTION_FAILURE)
            | Some(&SqlState::SQLCLIENT_UNABLE_TO_ESTABLISH_SQLCONNECTION)
            | Some(&SqlState::SQLSERVER_REJECTED_ESTABLISHMENT_OF_SQLCONNECTION)
            | Some(&SqlState::TRANSACTION_RESOLUTION_UNKNOWN)
            | Some(&SqlState::PROTOCOL_VIOLATION)
    );
    OperatorReviewError::Database {
        message: error.to_string(),
        code,
        constraint: error
            .as_db_error()
            .and_then(|db_error| db_error.constraint())
            .map(str::to_owned),
        retryable,
    }
}
