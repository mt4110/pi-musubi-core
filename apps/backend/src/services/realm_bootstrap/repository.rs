use std::{fmt::Write as _, sync::Arc};

use chrono::{DateTime, Utc};
use musubi_db_runtime::{DbConfig, connect_writer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_postgres::{Client, GenericClient, Row, error::SqlState};
use uuid::Uuid;

use super::types::{
    CreateRealmAdmissionInput, CreateRealmRequestInput, CreateRealmSponsorRecordInput,
    RealmAdmissionSnapshot, RealmAdmissionViewSnapshot, RealmBootstrapError,
    RealmBootstrapRebuildSnapshot, RealmBootstrapSummarySnapshot, RealmBootstrapViewSnapshot,
    RealmRequestSnapshot, RealmReviewSummarySnapshot, RealmReviewTriggerSnapshot, RealmSnapshot,
    RealmSponsorRecordSnapshot, RejectRealmRequestInput, ReviewRealmRequestInput,
};

const SPONSOR_STATUSES: &[&str] = &["proposed", "approved", "active", "rate_limited", "revoked"];
const REVIEW_TRIGGER_KINDS: &[&str] = &[
    "sponsor_concentration",
    "duplicate_venue_context",
    "suspicious_member_overlap",
    "proof_failure_rate",
    "safety_case_concentration",
    "operator_restriction",
    "quota_exceeded",
    "quota_abuse",
    "corridor_cap_pressure",
    "revoked_sponsor_lineage",
    "repeated_rejected_requests",
];
const REASON_CODES: &[&str] = &[
    "request_received",
    "review_required",
    "limited_bootstrap_active",
    "active_after_review",
    "request_rejected",
    "duplicate_or_invalid",
    "sponsor_required",
    "bootstrap_capacity_reached",
    "bootstrap_expired",
    "sponsor_rate_limited",
    "sponsor_revoked",
    "restricted_after_review",
    "suspended_after_review",
    "operator_restriction",
];
const OPERATOR_READ_ROLES: &[&str] = &["reviewer", "approver", "steward", "auditor", "support"];
const OPERATOR_WRITE_ROLES: &[&str] = &["approver", "steward"];

#[derive(Clone)]
pub struct RealmBootstrapStore {
    client: Arc<Mutex<Client>>,
}

impl RealmBootstrapStore {
    pub(crate) async fn connect(config: &DbConfig) -> musubi_db_runtime::Result<Self> {
        let client = connect_writer(config, "musubi-backend realm-bootstrap").await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }

    pub(crate) async fn reset_for_test(&self) -> Result<(), RealmBootstrapError> {
        let client = self.client.lock().await;
        client
            .batch_execute(
                "
                DELETE FROM projection.realm_admission_views;
                DELETE FROM projection.realm_review_summaries;
                DELETE FROM projection.realm_bootstrap_views;
                DELETE FROM dao.realm_review_triggers;
                DELETE FROM dao.realm_admissions;
                DELETE FROM dao.bootstrap_corridors;
                DELETE FROM dao.realm_sponsor_records;
                DELETE FROM dao.realms;
                DELETE FROM dao.realm_requests;
                ",
            )
            .await
            .map_err(db_error)?;
        Ok(())
    }

    pub async fn create_realm_request(
        &self,
        requester_account_id: &str,
        input: CreateRealmRequestInput,
    ) -> Result<RealmRequestSnapshot, RealmBootstrapError> {
        let requester_account_id = parse_uuid(requester_account_id, "requester account id")?;
        ensure_non_empty("display_name", &input.display_name)?;
        let slug_candidate = normalize_slug_candidate(&input.slug_candidate)?;
        ensure_non_empty("purpose_text", &input.purpose_text)?;
        ensure_non_empty_json_object("venue_context_json", &input.venue_context_json)?;
        ensure_non_empty_json_object(
            "expected_member_shape_json",
            &input.expected_member_shape_json,
        )?;
        ensure_non_empty("bootstrap_rationale_text", &input.bootstrap_rationale_text)?;
        let proposed_sponsor_account_id = parse_optional_uuid(
            &input.proposed_sponsor_account_id,
            "proposed sponsor account id",
        )?;
        let proposed_steward_account_id = parse_optional_uuid(
            &input.proposed_steward_account_id,
            "proposed steward account id",
        )?;
        let request_idempotency_key = normalize_optional(input.request_idempotency_key.as_deref())
            .ok_or_else(|| {
                RealmBootstrapError::BadRequest(
                    "realm request requires request_idempotency_key".to_owned(),
                )
            })?;
        let request_payload_hash = create_realm_request_payload_hash(
            &input,
            &slug_candidate,
            &proposed_sponsor_account_id,
            &proposed_steward_account_id,
        );

        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        ensure_active_account_exists_tx(&tx, &requester_account_id).await?;
        if let Some(sponsor_account_id) = proposed_sponsor_account_id.as_ref() {
            ensure_active_account_exists_tx(&tx, sponsor_account_id).await?;
        }
        if let Some(steward_account_id) = proposed_steward_account_id.as_ref() {
            ensure_active_account_exists_tx(&tx, steward_account_id).await?;
        }

        let row = if let Some(existing) = find_realm_request_by_idempotency_tx(
            &tx,
            &requester_account_id,
            &request_idempotency_key,
        )
        .await?
        {
            ensure_request_payload_hash_matches(&existing, &request_payload_hash)?;
            existing
        } else {
            let request_row = tx
                .query_one(
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
                        proposed_sponsor_account_id,
                        proposed_steward_account_id,
                        request_state,
                        review_reason_code,
                        request_idempotency_key,
                        request_payload_hash
                    )
                    VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                        'requested',
                        'request_received',
                        $11, $12
                    )
                    RETURNING *
                    ",
                    &[
                        &Uuid::new_v4(),
                        &requester_account_id,
                        &normalize_required(&input.display_name, "display_name")?,
                        &slug_candidate,
                        &normalize_required(&input.purpose_text, "purpose_text")?,
                        &input.venue_context_json,
                        &input.expected_member_shape_json,
                        &normalize_required(
                            &input.bootstrap_rationale_text,
                            "bootstrap_rationale_text",
                        )?,
                        &proposed_sponsor_account_id,
                        &proposed_steward_account_id,
                        &Some(request_idempotency_key.clone()),
                        &request_payload_hash,
                    ],
                )
                .await
                .map_err(db_error)?;
            let realm_request_id: Uuid = request_row.get("realm_request_id");
            let requires_review = maybe_open_realm_request_triggers_tx(
                &tx,
                &realm_request_id,
                &requester_account_id,
                &input.venue_context_json,
            )
            .await?;
            if requires_review {
                tx.execute(
                    "
                    UPDATE dao.realm_requests
                    SET request_state = 'pending_review',
                        review_reason_code = 'review_required',
                        updated_at = CURRENT_TIMESTAMP
                    WHERE realm_request_id = $1
                    ",
                    &[&realm_request_id],
                )
                .await
                .map_err(db_error)?;
                lock_realm_request_tx(&tx, &realm_request_id).await?
            } else {
                request_row
            }
        };

        tx.commit().await.map_err(db_error)?;
        realm_request_from_row(&row)
    }

    pub async fn get_realm_request_for_requester(
        &self,
        requester_account_id: &str,
        realm_request_id: &str,
    ) -> Result<RealmRequestSnapshot, RealmBootstrapError> {
        let requester_account_id = parse_uuid(requester_account_id, "requester account id")?;
        let realm_request_id = parse_uuid(realm_request_id, "realm request id")?;
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "
                SELECT
                    request.*,
                    realm.realm_id AS created_realm_id
                FROM dao.realm_requests request
                LEFT JOIN dao.realms realm
                  ON realm.created_from_realm_request_id = request.realm_request_id
                WHERE request.realm_request_id = $1
                  AND request.requested_by_account_id = $2
                ",
                &[&realm_request_id, &requester_account_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(|| {
                RealmBootstrapError::NotFound("realm request was not found".to_owned())
            })?;
        realm_request_from_row(&row)
    }

    pub async fn list_realm_requests_for_operator(
        &self,
        operator_id: &str,
    ) -> Result<Vec<RealmRequestSnapshot>, RealmBootstrapError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        let client = self.client.lock().await;
        ensure_operator_role_tx(&*client, &operator_id, OPERATOR_READ_ROLES).await?;
        let rows = client
            .query(
                "
                SELECT
                    request.*,
                    realm.realm_id AS created_realm_id
                FROM dao.realm_requests request
                LEFT JOIN dao.realms realm
                  ON realm.created_from_realm_request_id = request.realm_request_id
                ORDER BY request.created_at DESC, request.realm_request_id DESC
                ",
                &[],
            )
            .await
            .map_err(db_error)?;
        rows.iter().map(realm_request_from_row).collect()
    }

    pub async fn read_realm_request_for_operator(
        &self,
        operator_id: &str,
        realm_request_id: &str,
    ) -> Result<RealmRequestSnapshot, RealmBootstrapError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        let realm_request_id = parse_uuid(realm_request_id, "realm request id")?;
        let client = self.client.lock().await;
        ensure_operator_role_tx(&*client, &operator_id, OPERATOR_READ_ROLES).await?;
        let row = client
            .query_opt(
                "
                SELECT
                    request.*,
                    realm.realm_id AS created_realm_id
                FROM dao.realm_requests request
                LEFT JOIN dao.realms realm
                  ON realm.created_from_realm_request_id = request.realm_request_id
                WHERE request.realm_request_id = $1
                ",
                &[&realm_request_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(|| {
                RealmBootstrapError::NotFound("realm request was not found".to_owned())
            })?;
        realm_request_from_row(&row)
    }

    pub async fn approve_realm_request(
        &self,
        operator_id: &str,
        realm_request_id: &str,
        input: ReviewRealmRequestInput,
    ) -> Result<RealmSnapshot, RealmBootstrapError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        let realm_request_id = parse_uuid(realm_request_id, "realm request id")?;
        validate_allowed(
            "target_realm_status",
            &input.target_realm_status,
            &["limited_bootstrap", "active"],
        )?;
        validate_allowed(
            "review_reason_code",
            &input.review_reason_code,
            REASON_CODES,
        )?;
        let review_decision_idempotency_key = normalize_optional(
            input.review_decision_idempotency_key.as_deref(),
        )
        .ok_or_else(|| {
            RealmBootstrapError::BadRequest(
                "approve realm request requires review_decision_idempotency_key".to_owned(),
            )
        })?;
        let steward_account_id =
            parse_optional_uuid(&input.steward_account_id, "steward account id")?;
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        ensure_operator_role_tx(&tx, &operator_id, OPERATOR_WRITE_ROLES).await?;

        let request = lock_realm_request_tx(&tx, &realm_request_id).await?;
        let existing_realm_id: Option<String> = request.get("created_realm_id");
        let approved_slug = input
            .approved_slug
            .as_deref()
            .map(normalize_slug_candidate)
            .transpose()?
            .unwrap_or_else(|| request.get::<_, String>("slug_candidate"));
        let approved_display_name = input
            .approved_display_name
            .as_deref()
            .map(|value| normalize_required(value, "approved_display_name"))
            .transpose()?
            .unwrap_or_else(|| request.get::<_, String>("display_name"));
        let decision_payload_hash = approve_request_payload_hash(
            &input,
            &approved_slug,
            &approved_display_name,
            &steward_account_id,
        );

        match request.get::<_, String>("request_state").as_str() {
            "approved" => {
                ensure_request_review_replay_matches(
                    &request,
                    &review_decision_idempotency_key,
                    &decision_payload_hash,
                )?;
                let existing_realm_id = existing_realm_id.ok_or_else(|| {
                    RealmBootstrapError::Internal(
                        "approved realm request is missing created realm linkage".to_owned(),
                    )
                })?;
                refresh_realm_projection_bundle_tx(&tx, &existing_realm_id, None).await?;
                let realm_row = lock_realm_tx(&tx, &existing_realm_id).await?;
                tx.commit().await.map_err(db_error)?;
                return realm_from_row(&realm_row);
            }
            "rejected" => {
                return Err(RealmBootstrapError::BadRequest(
                    "rejected realm requests cannot be approved".to_owned(),
                ));
            }
            "requested" | "pending_review" => {}
            other => {
                return Err(RealmBootstrapError::BadRequest(format!(
                    "realm request cannot be approved from state {other}"
                )));
            }
        }

        if input.target_realm_status == "limited_bootstrap" {
            require_corridor_fields(&input)?;
        }
        if let Some(steward_account_id) = steward_account_id.as_ref() {
            ensure_active_account_exists_tx(&tx, steward_account_id).await?;
        }
        ensure_slug_available_for_approval_tx(&tx, &approved_slug, &realm_request_id).await?;

        let created_realm_id = format!("realm-{}", Uuid::new_v4());
        let realm_row = tx
            .query_one(
                "
                INSERT INTO dao.realms (
                    realm_id,
                    slug,
                    display_name,
                    realm_status,
                    public_reason_code,
                    created_from_realm_request_id,
                    steward_account_id,
                    created_by_operator_id
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                RETURNING *
                ",
                &[
                    &created_realm_id,
                    &approved_slug,
                    &approved_display_name,
                    &input.target_realm_status,
                    &input.review_reason_code,
                    &realm_request_id,
                    &steward_account_id,
                    &operator_id,
                ],
            )
            .await
            .map_err(db_error)?;

        let proposed_sponsor_account_id: Option<Uuid> = request.get("proposed_sponsor_account_id");
        if let Some(sponsor_account_id) = proposed_sponsor_account_id.as_ref() {
            ensure_active_account_exists_tx(&tx, sponsor_account_id).await?;
            let quota_total = input.sponsor_quota_total.ok_or_else(|| {
                RealmBootstrapError::BadRequest(
                    "approval with a proposed sponsor requires sponsor_quota_total".to_owned(),
                )
            })?;
            let sponsor_payload_hash = create_sponsor_record_payload_hash(
                &CreateRealmSponsorRecordInput {
                    sponsor_account_id: sponsor_account_id.to_string(),
                    sponsor_status: "active".to_owned(),
                    quota_total,
                    status_reason_code: input.review_reason_code.clone(),
                    request_idempotency_key: None,
                },
                sponsor_account_id,
            );
            insert_sponsor_record_tx(
                &tx,
                &created_realm_id,
                sponsor_account_id,
                "active",
                quota_total,
                &input.review_reason_code,
                &operator_id,
                None,
                Some(sponsor_payload_hash),
            )
            .await?;
        }

        if input.target_realm_status == "limited_bootstrap" {
            insert_bootstrap_corridor_tx(&tx, &created_realm_id, &input, &operator_id).await?;
        }

        tx.execute(
            "
            UPDATE dao.realm_requests
            SET request_state = 'approved',
                slug_candidate = $6,
                review_reason_code = $2,
                reviewed_by_operator_id = $3,
                review_decision_idempotency_key = $4,
                review_decision_payload_hash = $5,
                reviewed_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            WHERE realm_request_id = $1
            ",
            &[
                &realm_request_id,
                &input.review_reason_code,
                &operator_id,
                &Some(review_decision_idempotency_key),
                &Some(decision_payload_hash),
                &approved_slug,
            ],
        )
        .await
        .map_err(db_error)?;

        refresh_realm_projection_bundle_tx(&tx, &created_realm_id, None).await?;
        tx.commit().await.map_err(db_error)?;
        realm_from_row(&realm_row)
    }

    pub async fn reject_realm_request(
        &self,
        operator_id: &str,
        realm_request_id: &str,
        input: RejectRealmRequestInput,
    ) -> Result<RealmRequestSnapshot, RealmBootstrapError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        let realm_request_id = parse_uuid(realm_request_id, "realm request id")?;
        validate_allowed(
            "review_reason_code",
            &input.review_reason_code,
            REASON_CODES,
        )?;
        let review_decision_idempotency_key = normalize_optional(
            input.review_decision_idempotency_key.as_deref(),
        )
        .ok_or_else(|| {
            RealmBootstrapError::BadRequest(
                "reject realm request requires review_decision_idempotency_key".to_owned(),
            )
        })?;
        let decision_payload_hash = reject_request_payload_hash(&input);
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        ensure_operator_role_tx(&tx, &operator_id, OPERATOR_WRITE_ROLES).await?;
        let request = lock_realm_request_tx(&tx, &realm_request_id).await?;

        match request.get::<_, String>("request_state").as_str() {
            "rejected" => {
                ensure_request_review_replay_matches(
                    &request,
                    &review_decision_idempotency_key,
                    &decision_payload_hash,
                )?;
                tx.commit().await.map_err(db_error)?;
                return realm_request_from_row(&request);
            }
            "approved" => {
                return Err(RealmBootstrapError::BadRequest(
                    "approved realm requests cannot be rejected".to_owned(),
                ));
            }
            "requested" | "pending_review" => {}
            other => {
                return Err(RealmBootstrapError::BadRequest(format!(
                    "realm request cannot be rejected from state {other}"
                )));
            }
        }

        tx.execute(
            "
            UPDATE dao.realm_requests
            SET request_state = 'rejected',
                review_reason_code = $2,
                reviewed_by_operator_id = $3,
                review_decision_idempotency_key = $4,
                review_decision_payload_hash = $5,
                reviewed_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            WHERE realm_request_id = $1
            ",
            &[
                &realm_request_id,
                &input.review_reason_code,
                &operator_id,
                &Some(review_decision_idempotency_key),
                &Some(decision_payload_hash),
            ],
        )
        .await
        .map_err(db_error)?;
        maybe_open_repeated_rejection_trigger_tx(&tx, &realm_request_id).await?;
        let refreshed = lock_realm_request_tx(&tx, &realm_request_id).await?;
        tx.commit().await.map_err(db_error)?;
        realm_request_from_row(&refreshed)
    }

    pub async fn create_realm_sponsor_record(
        &self,
        operator_id: &str,
        realm_id: &str,
        input: CreateRealmSponsorRecordInput,
    ) -> Result<RealmSponsorRecordSnapshot, RealmBootstrapError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        let realm_id = normalize_required(realm_id, "realm id")?;
        validate_allowed("sponsor_status", &input.sponsor_status, SPONSOR_STATUSES)?;
        validate_allowed(
            "status_reason_code",
            &input.status_reason_code,
            REASON_CODES,
        )?;
        let sponsor_account_id = parse_uuid(&input.sponsor_account_id, "sponsor account id")?;
        if input.quota_total <= 0 {
            return Err(RealmBootstrapError::BadRequest(
                "quota_total must be positive".to_owned(),
            ));
        }
        let request_idempotency_key = normalize_optional(input.request_idempotency_key.as_deref())
            .ok_or_else(|| {
                RealmBootstrapError::BadRequest(
                    "sponsor record creation requires request_idempotency_key".to_owned(),
                )
            })?;
        let payload_hash = create_sponsor_record_payload_hash(&input, &sponsor_account_id);
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        ensure_operator_role_tx(&tx, &operator_id, OPERATOR_WRITE_ROLES).await?;
        ensure_active_account_exists_tx(&tx, &sponsor_account_id).await?;
        lock_realm_tx(&tx, &realm_id).await?;
        update_expired_corridors_tx(&tx, Some(&realm_id)).await?;

        let row = if let Some(existing) = find_sponsor_record_by_idempotency_tx(
            &tx,
            &realm_id,
            &operator_id,
            &request_idempotency_key,
        )
        .await?
        {
            ensure_sponsor_record_payload_hash_matches(&existing, &payload_hash)?;
            existing
        } else {
            let row = insert_sponsor_record_tx(
                &tx,
                &realm_id,
                &sponsor_account_id,
                &input.sponsor_status,
                input.quota_total,
                &input.status_reason_code,
                &operator_id,
                Some(request_idempotency_key),
                Some(payload_hash),
            )
            .await?;
            maybe_open_sponsor_concentration_trigger_tx(&tx, &realm_id, &sponsor_account_id)
                .await?;
            row
        };

        refresh_realm_projection_bundle_tx(&tx, &realm_id, None).await?;
        tx.commit().await.map_err(db_error)?;
        realm_sponsor_record_from_row(&row)
    }

    pub async fn create_realm_admission(
        &self,
        operator_id: &str,
        realm_id: &str,
        input: CreateRealmAdmissionInput,
    ) -> Result<RealmAdmissionSnapshot, RealmBootstrapError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        let realm_id = normalize_required(realm_id, "realm id")?;
        let account_id = parse_uuid(&input.account_id, "account id")?;
        ensure_non_empty("source_fact_kind", &input.source_fact_kind)?;
        ensure_non_empty("source_fact_id", &input.source_fact_id)?;
        let sponsor_record_id = parse_optional_uuid(&input.sponsor_record_id, "sponsor record id")?;
        let request_idempotency_key = normalize_optional(input.request_idempotency_key.as_deref())
            .ok_or_else(|| {
                RealmBootstrapError::BadRequest(
                    "admission creation requires request_idempotency_key".to_owned(),
                )
            })?;

        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        ensure_operator_role_tx(&tx, &operator_id, OPERATOR_WRITE_ROLES).await?;
        ensure_active_account_exists_tx(&tx, &account_id).await?;
        update_expired_corridors_tx(&tx, Some(&realm_id)).await?;
        let granted_by_actor_kind = operator_actor_kind_tx(&tx, &operator_id).await?;
        let payload_hash = create_admission_payload_hash(
            &input,
            &account_id,
            &sponsor_record_id,
            &granted_by_actor_kind,
        );

        let row = if let Some(existing) =
            find_admission_by_idempotency_tx(&tx, &realm_id, &operator_id, &request_idempotency_key)
                .await?
        {
            ensure_admission_payload_hash_matches(&existing, &payload_hash)?;
            existing
        } else {
            let realm_row = lock_realm_tx(&tx, &realm_id).await?;
            let realm_status: String = realm_row.get("realm_status");
            if matches!(realm_status.as_str(), "restricted" | "suspended") {
                return Err(RealmBootstrapError::BadRequest(
                    "restricted or suspended realms cannot admit new members".to_owned(),
                ));
            }
            let active_corridor = find_current_corridor_tx(&tx, &realm_id).await?;
            let latest_corridor_status = find_latest_corridor_status_tx(&tx, &realm_id).await?;
            let admission_context = derive_admission_context_tx(
                &tx,
                &realm_id,
                &realm_status,
                sponsor_record_id.as_ref(),
                active_corridor.as_ref(),
                latest_corridor_status.as_deref(),
            )
            .await?;

            if let Some(trigger) = admission_context.open_trigger.as_ref() {
                open_trigger_tx(
                    &tx,
                    Some(&realm_id),
                    trigger.kind,
                    trigger.reason_code,
                    Some(&account_id),
                    None,
                    admission_context.sponsor_record_id.as_ref(),
                    &trigger.context_json,
                    &trigger.fingerprint,
                )
                .await?;
            }

            let row = tx
                .query_one(
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
                        $1, $2, $3, $4, $5, $6, $7,
                        $8, $9, $10, $11, $12, $13, $14, $15
                    )
                    RETURNING *
                    ",
                    &[
                        &Uuid::new_v4(),
                        &realm_id,
                        &account_id,
                        &admission_context.admission_kind,
                        &admission_context.admission_status,
                        &admission_context.sponsor_record_id,
                        &admission_context.bootstrap_corridor_id,
                        &granted_by_actor_kind,
                        &operator_id,
                        &admission_context.reason_code,
                        &input.source_fact_kind,
                        &input.source_fact_id,
                        &input.source_snapshot_json,
                        &Some(request_idempotency_key),
                        &payload_hash,
                    ],
                )
                .await
                .map_err(db_error)?;
            maybe_open_member_overlap_trigger_tx(&tx, &realm_id, &account_id).await?;
            row
        };

        let refreshed_admission: RealmAdmissionSnapshot = realm_admission_from_row(&row)?;
        refresh_realm_projection_bundle_tx(
            &tx,
            &realm_id,
            Some(&parse_uuid(&refreshed_admission.account_id, "account id")?),
        )
        .await?;
        tx.commit().await.map_err(db_error)?;
        Ok(refreshed_admission)
    }

    pub async fn get_bootstrap_summary_for_viewer(
        &self,
        viewer_account_id: &str,
        realm_id: &str,
    ) -> Result<RealmBootstrapSummarySnapshot, RealmBootstrapError> {
        let viewer_account_id = parse_uuid(viewer_account_id, "viewer account id")?;
        let realm_id = normalize_required(realm_id, "realm id")?;
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        ensure_realm_exists_tx(&tx, &realm_id).await?;
        update_expired_corridors_tx(&tx, Some(&realm_id)).await?;
        refresh_realm_projection_bundle_tx(&tx, &realm_id, Some(&viewer_account_id)).await?;

        let bootstrap_row = tx
            .query_opt(
                "
                SELECT *
                FROM projection.realm_bootstrap_views
                WHERE realm_id = $1
                ",
                &[&realm_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(|| {
                RealmBootstrapError::NotFound("realm bootstrap summary was not found".to_owned())
            })?;

        let request_row = tx
            .query_opt(
                "
                SELECT
                    request.*,
                    realm.realm_id AS created_realm_id
                FROM dao.realms realm
                JOIN dao.realm_requests request
                  ON request.realm_request_id = realm.created_from_realm_request_id
                WHERE realm.realm_id = $1
                  AND request.requested_by_account_id = $2
                ",
                &[&realm_id, &viewer_account_id],
            )
            .await
            .map_err(db_error)?;
        let admission_row = tx
            .query_opt(
                "
                SELECT *
                FROM projection.realm_admission_views
                WHERE realm_id = $1
                  AND account_id = $2
                ",
                &[&realm_id, &viewer_account_id],
            )
            .await
            .map_err(db_error)?;

        if request_row.is_none() && admission_row.is_none() {
            return Err(RealmBootstrapError::NotFound(
                "realm bootstrap summary was not found".to_owned(),
            ));
        }

        let snapshot = RealmBootstrapSummarySnapshot {
            realm_request: request_row
                .as_ref()
                .map(realm_request_from_row)
                .transpose()?,
            bootstrap_view: realm_bootstrap_view_from_row(&bootstrap_row)?,
            admission_view: admission_row
                .as_ref()
                .map(realm_admission_view_from_row)
                .transpose()?,
        };
        tx.commit().await.map_err(db_error)?;
        Ok(snapshot)
    }

    pub async fn get_review_summary_for_operator(
        &self,
        operator_id: &str,
        realm_id: &str,
    ) -> Result<RealmReviewSummarySnapshot, RealmBootstrapError> {
        let operator_id = parse_uuid(operator_id, "operator id")?;
        let realm_id = normalize_required(realm_id, "realm id")?;
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        ensure_operator_role_tx(&tx, &operator_id, OPERATOR_READ_ROLES).await?;
        ensure_realm_exists_tx(&tx, &realm_id).await?;
        update_expired_corridors_tx(&tx, Some(&realm_id)).await?;
        refresh_realm_projection_bundle_tx(&tx, &realm_id, None).await?;
        let row = tx
            .query_opt(
                "
                SELECT *
                FROM projection.realm_review_summaries
                WHERE realm_id = $1
                ",
                &[&realm_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(|| {
                RealmBootstrapError::NotFound("realm review summary was not found".to_owned())
            })?;
        let trigger_rows = tx
            .query(
                "
                SELECT *
                FROM dao.realm_review_triggers
                WHERE realm_id = $1
                  AND trigger_state = 'open'
                ORDER BY created_at DESC, realm_review_trigger_id DESC
                ",
                &[&realm_id],
            )
            .await
            .map_err(db_error)?;
        let snapshot = realm_review_summary_from_row(&row, &trigger_rows)?;
        tx.commit().await.map_err(db_error)?;
        Ok(snapshot)
    }

    pub async fn rebuild_realm_bootstrap_views(
        &self,
    ) -> Result<RealmBootstrapRebuildSnapshot, RealmBootstrapError> {
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        tx.query_one(
            "
            SELECT pg_advisory_xact_lock(
                hashtext('projection.realm_bootstrap_views.rebuild')::bigint
            )
            ",
            &[],
        )
        .await
        .map_err(db_error)?;
        update_expired_corridors_tx(&tx, None).await?;

        let realm_rows = tx
            .query(
                "
                SELECT realm_id
                FROM dao.realms
                ORDER BY realm_id
                ",
                &[],
            )
            .await
            .map_err(db_error)?;
        let rebuild_generation = current_rebuild_generation_tx(&tx).await?;
        tx.execute("DELETE FROM projection.realm_admission_views", &[])
            .await
            .map_err(db_error)?;
        tx.execute("DELETE FROM projection.realm_review_summaries", &[])
            .await
            .map_err(db_error)?;
        tx.execute("DELETE FROM projection.realm_bootstrap_views", &[])
            .await
            .map_err(db_error)?;

        for row in &realm_rows {
            let realm_id: String = row.get("realm_id");
            refresh_realm_bootstrap_view_tx(&tx, &realm_id, Some(rebuild_generation)).await?;
            refresh_realm_review_summary_tx(&tx, &realm_id, Some(rebuild_generation)).await?;
        }

        let admission_rows = tx
            .query(
                "
                SELECT DISTINCT realm_id, account_id
                FROM dao.realm_admissions
                ORDER BY realm_id, account_id
                ",
                &[],
            )
            .await
            .map_err(db_error)?;
        for row in &admission_rows {
            let realm_id: String = row.get("realm_id");
            let account_id: Uuid = row.get("account_id");
            refresh_realm_admission_view_tx(&tx, &realm_id, &account_id, Some(rebuild_generation))
                .await?;
        }

        let bootstrap_view_count = tx
            .query_one(
                "SELECT COUNT(*) AS count FROM projection.realm_bootstrap_views",
                &[],
            )
            .await
            .map_err(db_error)?
            .get::<_, i64>("count");
        let admission_view_count = tx
            .query_one(
                "SELECT COUNT(*) AS count FROM projection.realm_admission_views",
                &[],
            )
            .await
            .map_err(db_error)?
            .get::<_, i64>("count");
        let review_summary_count = tx
            .query_one(
                "SELECT COUNT(*) AS count FROM projection.realm_review_summaries",
                &[],
            )
            .await
            .map_err(db_error)?
            .get::<_, i64>("count");

        tx.commit().await.map_err(db_error)?;
        Ok(RealmBootstrapRebuildSnapshot {
            bootstrap_view_count,
            admission_view_count,
            review_summary_count,
        })
    }
}

#[derive(Debug)]
struct TriggerIntent {
    kind: &'static str,
    reason_code: &'static str,
    context_json: Value,
    fingerprint: String,
}

#[derive(Debug)]
struct AdmissionContext {
    admission_kind: &'static str,
    admission_status: &'static str,
    reason_code: &'static str,
    sponsor_record_id: Option<Uuid>,
    bootstrap_corridor_id: Option<Uuid>,
    open_trigger: Option<TriggerIntent>,
}

fn limited_bootstrap_without_active_corridor_context(
    latest_corridor_status: Option<&str>,
) -> AdmissionContext {
    let reason_code = match latest_corridor_status {
        Some("expired") => "bootstrap_expired",
        Some("disabled_by_operator") => "operator_restriction",
        _ => "review_required",
    };
    AdmissionContext {
        admission_kind: "review_required",
        admission_status: "pending",
        reason_code,
        sponsor_record_id: None,
        bootstrap_corridor_id: None,
        open_trigger: None,
    }
}

async fn maybe_open_realm_request_triggers_tx<C: GenericClient + Sync>(
    client: &C,
    realm_request_id: &Uuid,
    requester_account_id: &Uuid,
    venue_context_json: &Value,
) -> Result<bool, RealmBootstrapError> {
    let mut requires_review = false;
    let rejected_count = client
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM dao.realm_requests
            WHERE requested_by_account_id = $1
              AND request_state = 'rejected'
            ",
            &[requester_account_id],
        )
        .await
        .map_err(db_error)?
        .get::<_, i64>("count");
    if rejected_count >= 2 {
        requires_review = true;
        open_trigger_tx(
            client,
            None,
            "repeated_rejected_requests",
            "review_required",
            Some(requester_account_id),
            Some(realm_request_id),
            None,
            &json!({ "rejected_request_count": rejected_count }),
            &format!("repeated-rejected-requests:{requester_account_id}"),
        )
        .await?;
    }

    let duplicate_count = client
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM dao.realm_requests
            WHERE realm_request_id <> $1
              AND venue_context_json = $2
              AND request_state IN ('requested', 'pending_review', 'approved')
            ",
            &[realm_request_id, venue_context_json],
        )
        .await
        .map_err(db_error)?
        .get::<_, i64>("count");
    if duplicate_count > 0 {
        requires_review = true;
        open_trigger_tx(
            client,
            None,
            "duplicate_venue_context",
            "duplicate_or_invalid",
            Some(requester_account_id),
            Some(realm_request_id),
            None,
            &json!({ "matching_request_count": duplicate_count }),
            &format!(
                "duplicate-venue-context:{}",
                hash_json_value(venue_context_json)
            ),
        )
        .await?;
    }

    Ok(requires_review)
}

async fn maybe_open_repeated_rejection_trigger_tx<C: GenericClient + Sync>(
    client: &C,
    realm_request_id: &Uuid,
) -> Result<(), RealmBootstrapError> {
    let row = client
        .query_one(
            "
            SELECT requested_by_account_id
            FROM dao.realm_requests
            WHERE realm_request_id = $1
            ",
            &[realm_request_id],
        )
        .await
        .map_err(db_error)?;
    let requester_account_id: Uuid = row.get("requested_by_account_id");
    let rejected_count = client
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM dao.realm_requests
            WHERE requested_by_account_id = $1
              AND request_state = 'rejected'
            ",
            &[&requester_account_id],
        )
        .await
        .map_err(db_error)?
        .get::<_, i64>("count");
    if rejected_count >= 2 {
        open_trigger_tx(
            client,
            None,
            "repeated_rejected_requests",
            "review_required",
            Some(&requester_account_id),
            Some(realm_request_id),
            None,
            &json!({ "rejected_request_count": rejected_count }),
            &format!("repeated-rejected-requests:{requester_account_id}"),
        )
        .await?;
    }
    Ok(())
}

async fn maybe_open_member_overlap_trigger_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
    account_id: &Uuid,
) -> Result<(), RealmBootstrapError> {
    let count = client
        .query_one(
            "
            SELECT COUNT(DISTINCT realm_id) AS count
            FROM dao.realm_admissions
            WHERE account_id = $1
              AND admission_status IN ('pending', 'admitted')
            ",
            &[account_id],
        )
        .await
        .map_err(db_error)?
        .get::<_, i64>("count");
    if count >= 3 {
        open_trigger_tx(
            client,
            Some(realm_id),
            "suspicious_member_overlap",
            "review_required",
            Some(account_id),
            None,
            None,
            &json!({ "active_realm_count": count }),
            &format!("member-overlap:{realm_id}:{account_id}"),
        )
        .await?;
    }
    Ok(())
}

async fn maybe_open_sponsor_concentration_trigger_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
    sponsor_account_id: &Uuid,
) -> Result<(), RealmBootstrapError> {
    let count = client
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM dao.realm_sponsor_records
            WHERE sponsor_account_id = $1
              AND sponsor_status IN ('approved', 'active', 'rate_limited')
            ",
            &[sponsor_account_id],
        )
        .await
        .map_err(db_error)?
        .get::<_, i64>("count");
    if count >= 3 {
        open_trigger_tx(
            client,
            Some(realm_id),
            "sponsor_concentration",
            "review_required",
            Some(sponsor_account_id),
            None,
            None,
            &json!({ "active_sponsor_record_count": count }),
            &format!("sponsor-concentration:{realm_id}:{sponsor_account_id}"),
        )
        .await?;
    }
    Ok(())
}

async fn derive_admission_context_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
    realm_status: &str,
    sponsor_record_id: Option<&Uuid>,
    active_corridor: Option<&Row>,
    latest_corridor_status: Option<&str>,
) -> Result<AdmissionContext, RealmBootstrapError> {
    let active_corridor_id = active_corridor.map(|row| row.get::<_, Uuid>("bootstrap_corridor_id"));
    let corridor_status = latest_corridor_status.unwrap_or("none");
    let active_corridor_member_count = if let Some(corridor_id) = active_corridor_id.as_ref() {
        count_corridor_admissions_tx(client, corridor_id).await?
    } else {
        0
    };

    if let Some(sponsor_record_id) = sponsor_record_id {
        let requested_sponsor_row = lock_sponsor_record_tx(client, sponsor_record_id).await?;
        let sponsor_realm_id: String = requested_sponsor_row.get("realm_id");
        if sponsor_realm_id != realm_id {
            return Err(RealmBootstrapError::BadRequest(
                "sponsor record does not belong to this realm".to_owned(),
            ));
        }
        let sponsor_account_id: Uuid = requested_sponsor_row.get("sponsor_account_id");
        let sponsor_row =
            lock_current_sponsor_record_tx(client, realm_id, &sponsor_account_id).await?;
        let current_sponsor_record_id: Uuid = sponsor_row.get("realm_sponsor_record_id");
        if realm_status == "limited_bootstrap" && active_corridor.is_none() {
            let mut context =
                limited_bootstrap_without_active_corridor_context(latest_corridor_status);
            context.sponsor_record_id = Some(current_sponsor_record_id);
            return Ok(context);
        }
        let sponsor_status: String = sponsor_row.get("sponsor_status");
        let sponsor_quota_total: i64 = sponsor_row.get("quota_total");
        let sponsor_used_count =
            count_sponsor_backed_admissions_tx(client, &current_sponsor_record_id).await?;
        if sponsor_status == "revoked" {
            return Ok(AdmissionContext {
                admission_kind: "review_required",
                admission_status: "pending",
                reason_code: "sponsor_revoked",
                sponsor_record_id: Some(current_sponsor_record_id),
                bootstrap_corridor_id: active_corridor_id,
                open_trigger: Some(trigger_intent(
                    "revoked_sponsor_lineage",
                    "sponsor_revoked",
                    json!({ "sponsor_record_id": current_sponsor_record_id.to_string() }),
                    format!("revoked-sponsor:{current_sponsor_record_id}"),
                )),
            });
        }
        if sponsor_status == "rate_limited" {
            return Ok(AdmissionContext {
                admission_kind: "review_required",
                admission_status: "pending",
                reason_code: "sponsor_rate_limited",
                sponsor_record_id: Some(current_sponsor_record_id),
                bootstrap_corridor_id: active_corridor_id,
                open_trigger: Some(trigger_intent(
                    "quota_abuse",
                    "sponsor_rate_limited",
                    json!({ "sponsor_record_id": current_sponsor_record_id.to_string() }),
                    format!("quota-abuse:{current_sponsor_record_id}"),
                )),
            });
        }
        if !matches!(sponsor_status.as_str(), "approved" | "active") {
            return Ok(AdmissionContext {
                admission_kind: "review_required",
                admission_status: "pending",
                reason_code: "review_required",
                sponsor_record_id: Some(current_sponsor_record_id),
                bootstrap_corridor_id: active_corridor_id,
                open_trigger: None,
            });
        }
        if sponsor_used_count >= sponsor_quota_total {
            return Ok(AdmissionContext {
                admission_kind: "review_required",
                admission_status: "pending",
                reason_code: "bootstrap_capacity_reached",
                sponsor_record_id: Some(current_sponsor_record_id),
                bootstrap_corridor_id: active_corridor_id,
                open_trigger: Some(trigger_intent(
                    "quota_exceeded",
                    "bootstrap_capacity_reached",
                    json!({
                        "sponsor_record_id": current_sponsor_record_id.to_string(),
                        "quota_total": sponsor_quota_total,
                        "used_count": sponsor_used_count,
                    }),
                    format!("quota-exceeded:{current_sponsor_record_id}"),
                )),
            });
        }
        if let Some(corridor_row) = active_corridor {
            let member_cap: i64 = corridor_row.get("member_cap");
            if active_corridor_member_count >= member_cap {
                return Ok(AdmissionContext {
                    admission_kind: "review_required",
                    admission_status: "pending",
                    reason_code: "bootstrap_capacity_reached",
                    sponsor_record_id: Some(current_sponsor_record_id),
                    bootstrap_corridor_id: active_corridor_id,
                    open_trigger: Some(trigger_intent(
                        "corridor_cap_pressure",
                        "bootstrap_capacity_reached",
                        json!({
                            "bootstrap_corridor_id": corridor_row.get::<_, Uuid>("bootstrap_corridor_id").to_string(),
                            "member_cap": member_cap,
                            "active_corridor_member_count": active_corridor_member_count,
                        }),
                        format!(
                            "corridor-member-cap:{}",
                            corridor_row.get::<_, Uuid>("bootstrap_corridor_id")
                        ),
                    )),
                });
            }
            let sponsor_cap: i64 = corridor_row.get("sponsor_cap");
            let sponsor_slots = count_distinct_corridor_sponsors_tx(
                client,
                &current_sponsor_record_id,
                corridor_row,
            )
            .await?;
            let sponsor_already_present = corridor_contains_sponsor_record_tx(
                client,
                &current_sponsor_record_id,
                corridor_row,
            )
            .await?;
            if !sponsor_already_present && sponsor_slots >= sponsor_cap {
                return Ok(AdmissionContext {
                    admission_kind: "review_required",
                    admission_status: "pending",
                    reason_code: "bootstrap_capacity_reached",
                    sponsor_record_id: Some(current_sponsor_record_id),
                    bootstrap_corridor_id: active_corridor_id,
                    open_trigger: Some(trigger_intent(
                        "corridor_cap_pressure",
                        "bootstrap_capacity_reached",
                        json!({
                            "bootstrap_corridor_id": corridor_row.get::<_, Uuid>("bootstrap_corridor_id").to_string(),
                            "sponsor_cap": sponsor_cap,
                            "distinct_sponsor_count": sponsor_slots,
                        }),
                        format!(
                            "corridor-sponsor-cap:{}",
                            corridor_row.get::<_, Uuid>("bootstrap_corridor_id")
                        ),
                    )),
                });
            }
        }
        let reason_code = if realm_status == "active" {
            "active_after_review"
        } else {
            "limited_bootstrap_active"
        };
        return Ok(AdmissionContext {
            admission_kind: "sponsor_backed",
            admission_status: "admitted",
            reason_code,
            sponsor_record_id: Some(current_sponsor_record_id),
            bootstrap_corridor_id: active_corridor_id,
            open_trigger: None,
        });
    }

    if let Some(corridor_row) = active_corridor {
        let member_cap: i64 = corridor_row.get("member_cap");
        if active_corridor_member_count >= member_cap {
            return Ok(AdmissionContext {
                admission_kind: "review_required",
                admission_status: "pending",
                reason_code: "bootstrap_capacity_reached",
                sponsor_record_id: None,
                bootstrap_corridor_id: active_corridor_id,
                open_trigger: Some(trigger_intent(
                    "corridor_cap_pressure",
                    "bootstrap_capacity_reached",
                    json!({
                        "bootstrap_corridor_id": corridor_row.get::<_, Uuid>("bootstrap_corridor_id").to_string(),
                        "member_cap": member_cap,
                        "active_corridor_member_count": active_corridor_member_count,
                    }),
                    format!(
                        "corridor-member-cap:{}",
                        corridor_row.get::<_, Uuid>("bootstrap_corridor_id")
                    ),
                )),
            });
        }
        return Ok(AdmissionContext {
            admission_kind: "corridor",
            admission_status: "admitted",
            reason_code: "limited_bootstrap_active",
            sponsor_record_id: None,
            bootstrap_corridor_id: active_corridor_id,
            open_trigger: None,
        });
    }

    let (admission_kind, reason_code) = match realm_status {
        "active" => ("normal", "active_after_review"),
        "limited_bootstrap" if corridor_status == "expired" => {
            ("review_required", "bootstrap_expired")
        }
        "limited_bootstrap" if corridor_status == "disabled_by_operator" => {
            ("review_required", "operator_restriction")
        }
        "limited_bootstrap" => ("review_required", "review_required"),
        _ => ("review_required", "review_required"),
    };
    Ok(AdmissionContext {
        admission_kind,
        admission_status: if realm_status == "active" {
            "admitted"
        } else {
            "pending"
        },
        reason_code,
        sponsor_record_id: None,
        bootstrap_corridor_id: None,
        open_trigger: None,
    })
}

async fn insert_sponsor_record_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
    sponsor_account_id: &Uuid,
    sponsor_status: &str,
    quota_total: i64,
    status_reason_code: &str,
    approved_by_operator_id: &Uuid,
    request_idempotency_key: Option<String>,
    request_payload_hash: Option<String>,
) -> Result<Row, RealmBootstrapError> {
    client
        .query_one(
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
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, COALESCE($9, repeat('0', 64)))
            RETURNING *
            ",
            &[
                &Uuid::new_v4(),
                &realm_id,
                sponsor_account_id,
                &sponsor_status,
                &quota_total,
                &status_reason_code,
                approved_by_operator_id,
                &request_idempotency_key,
                &request_payload_hash,
            ],
        )
        .await
        .map_err(db_error)
}

async fn insert_bootstrap_corridor_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
    input: &ReviewRealmRequestInput,
    operator_id: &Uuid,
) -> Result<(), RealmBootstrapError> {
    let starts_at = input.corridor_starts_at.ok_or_else(|| {
        RealmBootstrapError::BadRequest("corridor_starts_at is required".to_owned())
    })?;
    let ends_at = input.corridor_ends_at.ok_or_else(|| {
        RealmBootstrapError::BadRequest("corridor_ends_at is required".to_owned())
    })?;
    let member_cap = input.corridor_member_cap.ok_or_else(|| {
        RealmBootstrapError::BadRequest("corridor_member_cap is required".to_owned())
    })?;
    let sponsor_cap = input.corridor_sponsor_cap.ok_or_else(|| {
        RealmBootstrapError::BadRequest("corridor_sponsor_cap is required".to_owned())
    })?;
    if member_cap <= 0 || sponsor_cap <= 0 {
        return Err(RealmBootstrapError::BadRequest(
            "corridor caps must be positive".to_owned(),
        ));
    }
    if starts_at >= ends_at {
        return Err(RealmBootstrapError::BadRequest(
            "corridor_starts_at must be earlier than corridor_ends_at".to_owned(),
        ));
    }
    if starts_at > Utc::now() {
        return Err(RealmBootstrapError::BadRequest(
            "corridor_starts_at must not be in the future for Day 1 bootstrap activation"
                .to_owned(),
        ));
    }
    if ends_at <= Utc::now() {
        return Err(RealmBootstrapError::BadRequest(
            "corridor_ends_at must be in the future for Day 1 bootstrap activation".to_owned(),
        ));
    }
    client
        .execute(
            "
            INSERT INTO dao.bootstrap_corridors (
                bootstrap_corridor_id,
                realm_id,
                corridor_status,
                starts_at,
                ends_at,
                member_cap,
                sponsor_cap,
                review_threshold_json,
                created_by_operator_id
            )
            VALUES ($1, $2, 'active', $3, $4, $5, $6, $7, $8)
            ",
            &[
                &Uuid::new_v4(),
                &realm_id,
                &starts_at,
                &ends_at,
                &member_cap,
                &sponsor_cap,
                &input.review_threshold_json,
                operator_id,
            ],
        )
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn refresh_realm_projection_bundle_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
    account_id: Option<&Uuid>,
) -> Result<(), RealmBootstrapError> {
    refresh_realm_bootstrap_view_tx(client, realm_id, None).await?;
    refresh_realm_review_summary_tx(client, realm_id, None).await?;
    if let Some(account_id) = account_id {
        refresh_realm_admission_view_tx(client, realm_id, account_id, None).await?;
    }
    Ok(())
}

async fn refresh_realm_bootstrap_view_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
    rebuild_generation: Option<i64>,
) -> Result<(), RealmBootstrapError> {
    let row = client
        .query_one(
            "
            WITH latest_corridor AS (
                SELECT *
                FROM dao.bootstrap_corridors
                WHERE realm_id = $1
                ORDER BY updated_at DESC, bootstrap_corridor_id DESC
                LIMIT 1
            ),
            sponsor_counts AS (
                SELECT
                    COUNT(*) FILTER (WHERE sponsor_status IN ('approved', 'active', 'rate_limited')) AS active_sponsor_count
                FROM dao.realm_sponsor_records
                WHERE realm_id = $1
            ),
            source_counts AS (
                SELECT
                    1
                    + COALESCE((SELECT COUNT(*) FROM dao.realm_sponsor_records WHERE realm_id = $1), 0)
                    + COALESCE((SELECT COUNT(*) FROM dao.realm_admissions WHERE realm_id = $1), 0)
                    + COALESCE((SELECT COUNT(*) FROM dao.bootstrap_corridors WHERE realm_id = $1), 0)
                    + COALESCE((SELECT COUNT(*) FROM dao.realm_review_triggers WHERE realm_id = $1), 0)
                    AS source_fact_count
            ),
            watermarks AS (
                SELECT GREATEST(
                    realm.updated_at,
                    COALESCE((SELECT MAX(updated_at) FROM dao.realm_sponsor_records WHERE realm_id = $1), realm.updated_at),
                    COALESCE((SELECT MAX(updated_at) FROM dao.realm_admissions WHERE realm_id = $1), realm.updated_at),
                    COALESCE((SELECT MAX(updated_at) FROM dao.bootstrap_corridors WHERE realm_id = $1), realm.updated_at),
                    COALESCE((SELECT MAX(updated_at) FROM dao.realm_review_triggers WHERE realm_id = $1), realm.updated_at)
                ) AS source_watermark_at
                FROM dao.realms realm
                WHERE realm.realm_id = $1
            )
            SELECT
                realm.slug,
                realm.display_name,
                realm.realm_status,
                realm.public_reason_code,
                COALESCE((SELECT corridor_status FROM latest_corridor), 'none') AS corridor_status,
                CASE
                    WHEN realm.realm_status IN ('restricted', 'suspended') THEN 'closed'
                    WHEN realm.realm_status = 'active' THEN 'open'
                    WHEN realm.realm_status = 'limited_bootstrap' AND EXISTS (
                        SELECT 1
                        FROM latest_corridor
                        WHERE corridor_status = 'active'
                          AND starts_at <= CURRENT_TIMESTAMP
                          AND ends_at > CURRENT_TIMESTAMP
                    ) THEN 'limited'
                    ELSE 'review_required'
                END AS admission_posture,
                CASE
                    WHEN COALESCE((SELECT active_sponsor_count FROM sponsor_counts), 0) > 0
                         AND realm.steward_account_id IS NOT NULL THEN 'sponsor_and_steward'
                    WHEN COALESCE((SELECT active_sponsor_count FROM sponsor_counts), 0) > 0 THEN 'sponsor_backed'
                    WHEN realm.steward_account_id IS NOT NULL THEN 'steward_present'
                    ELSE 'none'
                END AS sponsor_display_state,
                (SELECT source_watermark_at FROM watermarks) AS source_watermark_at,
                (SELECT source_fact_count FROM source_counts) AS source_fact_count
            FROM dao.realms realm
            WHERE realm.realm_id = $1
            ",
            &[&realm_id],
        )
        .await
        .map_err(db_error)?;

    let source_watermark_at: DateTime<Utc> = row.get("source_watermark_at");
    let projection_lag_ms = (Utc::now() - source_watermark_at).num_milliseconds().max(0);
    client
        .execute(
            "
            INSERT INTO projection.realm_bootstrap_views (
                realm_id,
                slug,
                display_name,
                realm_status,
                admission_posture,
                corridor_status,
                public_reason_code,
                sponsor_display_state,
                source_watermark_at,
                source_fact_count,
                projection_lag_ms,
                rebuild_generation,
                last_projected_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, COALESCE($12, 1::bigint), CURRENT_TIMESTAMP)
            ON CONFLICT (realm_id) DO UPDATE
            SET slug = EXCLUDED.slug,
                display_name = EXCLUDED.display_name,
                realm_status = EXCLUDED.realm_status,
                admission_posture = EXCLUDED.admission_posture,
                corridor_status = EXCLUDED.corridor_status,
                public_reason_code = EXCLUDED.public_reason_code,
                sponsor_display_state = EXCLUDED.sponsor_display_state,
                source_watermark_at = EXCLUDED.source_watermark_at,
                source_fact_count = EXCLUDED.source_fact_count,
                projection_lag_ms = EXCLUDED.projection_lag_ms,
                rebuild_generation = COALESCE($12, projection.realm_bootstrap_views.rebuild_generation, 1::bigint),
                last_projected_at = CURRENT_TIMESTAMP
            ",
            &[
                &realm_id,
                &row.get::<_, String>("slug"),
                &row.get::<_, String>("display_name"),
                &row.get::<_, String>("realm_status"),
                &row.get::<_, String>("admission_posture"),
                &row.get::<_, String>("corridor_status"),
                &row.get::<_, String>("public_reason_code"),
                &row.get::<_, String>("sponsor_display_state"),
                &source_watermark_at,
                &row.get::<_, i64>("source_fact_count"),
                &projection_lag_ms,
                &rebuild_generation,
            ],
        )
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn refresh_realm_admission_view_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
    account_id: &Uuid,
    rebuild_generation: Option<i64>,
) -> Result<(), RealmBootstrapError> {
    let row = client
        .query_opt(
            "
            WITH latest_admission AS (
                SELECT *
                FROM dao.realm_admissions
                WHERE realm_id = $1
                  AND account_id = $2
                ORDER BY updated_at DESC, realm_admission_id DESC
                LIMIT 1
            ),
            source_counts AS (
                SELECT COUNT(*) AS source_fact_count
                FROM dao.realm_admissions
                WHERE realm_id = $1
                  AND account_id = $2
            )
            SELECT
                latest_admission.admission_status,
                latest_admission.admission_kind,
                latest_admission.review_reason_code AS public_reason_code,
                latest_admission.updated_at AS source_watermark_at,
                (SELECT source_fact_count FROM source_counts) AS source_fact_count
            FROM latest_admission
            ",
            &[&realm_id, account_id],
        )
        .await
        .map_err(db_error)?;

    let Some(row) = row else {
        client
            .execute(
                "
                DELETE FROM projection.realm_admission_views
                WHERE realm_id = $1
                  AND account_id = $2
                ",
                &[&realm_id, account_id],
            )
            .await
            .map_err(db_error)?;
        return Ok(());
    };

    let source_watermark_at: DateTime<Utc> = row.get("source_watermark_at");
    let projection_lag_ms = (Utc::now() - source_watermark_at).num_milliseconds().max(0);
    client
        .execute(
            "
            INSERT INTO projection.realm_admission_views (
                realm_id,
                account_id,
                admission_status,
                admission_kind,
                public_reason_code,
                source_watermark_at,
                source_fact_count,
                projection_lag_ms,
                rebuild_generation,
                last_projected_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, COALESCE($9, 1::bigint), CURRENT_TIMESTAMP)
            ON CONFLICT (realm_id, account_id) DO UPDATE
            SET admission_status = EXCLUDED.admission_status,
                admission_kind = EXCLUDED.admission_kind,
                public_reason_code = EXCLUDED.public_reason_code,
                source_watermark_at = EXCLUDED.source_watermark_at,
                source_fact_count = EXCLUDED.source_fact_count,
                projection_lag_ms = EXCLUDED.projection_lag_ms,
                rebuild_generation = COALESCE($9, projection.realm_admission_views.rebuild_generation, 1::bigint),
                last_projected_at = CURRENT_TIMESTAMP
            ",
            &[
                &realm_id,
                account_id,
                &row.get::<_, String>("admission_status"),
                &row.get::<_, String>("admission_kind"),
                &row.get::<_, String>("public_reason_code"),
                &source_watermark_at,
                &row.get::<_, i64>("source_fact_count"),
                &projection_lag_ms,
                &rebuild_generation,
            ],
        )
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn refresh_realm_review_summary_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
    rebuild_generation: Option<i64>,
) -> Result<(), RealmBootstrapError> {
    let row = client
        .query_one(
            "
            WITH latest_corridor AS (
                SELECT *
                FROM dao.bootstrap_corridors
                WHERE realm_id = $1
                ORDER BY updated_at DESC, bootstrap_corridor_id DESC
                LIMIT 1
            ),
            active_sponsors AS (
                SELECT COUNT(*) AS active_sponsor_count
                FROM dao.realm_sponsor_records
                WHERE realm_id = $1
                  AND sponsor_status IN ('approved', 'active', 'rate_limited')
            ),
            sponsor_backed_admissions AS (
                SELECT COUNT(*) AS sponsor_backed_admission_count
                FROM dao.realm_admissions
                WHERE realm_id = $1
                  AND admission_kind = 'sponsor_backed'
                  AND admission_status IN ('pending', 'admitted')
            ),
            recent_admissions AS (
                SELECT COUNT(*) AS recent_admission_count_7d
                FROM dao.realm_admissions
                WHERE realm_id = $1
                  AND created_at >= (CURRENT_TIMESTAMP - interval '7 days')
            ),
            open_triggers AS (
                SELECT COUNT(*) AS open_review_trigger_count
                FROM dao.realm_review_triggers
                WHERE realm_id = $1
                  AND trigger_state = 'open'
            ),
            open_cases AS (
                SELECT COUNT(*) AS open_review_case_count
                FROM dao.review_cases
                WHERE related_realm_id = $1
                  AND review_status <> 'closed'
            ),
            source_counts AS (
                SELECT
                    1
                    + COALESCE((SELECT COUNT(*) FROM dao.realm_sponsor_records WHERE realm_id = $1), 0)
                    + COALESCE((SELECT COUNT(*) FROM dao.realm_admissions WHERE realm_id = $1), 0)
                    + COALESCE((SELECT COUNT(*) FROM dao.bootstrap_corridors WHERE realm_id = $1), 0)
                    + COALESCE((SELECT COUNT(*) FROM dao.realm_review_triggers WHERE realm_id = $1), 0)
                    + COALESCE((SELECT COUNT(*) FROM dao.review_cases WHERE related_realm_id = $1), 0)
                    AS source_fact_count
            ),
            latest_reason AS (
                SELECT redacted_reason_code
                FROM dao.realm_review_triggers
                WHERE realm_id = $1
                ORDER BY updated_at DESC, realm_review_trigger_id DESC
                LIMIT 1
            ),
            watermarks AS (
                SELECT GREATEST(
                    realm.updated_at,
                    COALESCE((SELECT MAX(updated_at) FROM dao.realm_sponsor_records WHERE realm_id = $1), realm.updated_at),
                    COALESCE((SELECT MAX(updated_at) FROM dao.realm_admissions WHERE realm_id = $1), realm.updated_at),
                    COALESCE((SELECT MAX(updated_at) FROM dao.bootstrap_corridors WHERE realm_id = $1), realm.updated_at),
                    COALESCE((SELECT MAX(updated_at) FROM dao.realm_review_triggers WHERE realm_id = $1), realm.updated_at),
                    COALESCE((SELECT MAX(updated_at) FROM dao.review_cases WHERE related_realm_id = $1), realm.updated_at)
                ) AS source_watermark_at
                FROM dao.realms realm
                WHERE realm.realm_id = $1
            )
            SELECT
                realm.realm_status,
                COALESCE((SELECT corridor_status FROM latest_corridor), 'none') AS corridor_status,
                CASE
                    WHEN EXISTS (SELECT 1 FROM latest_corridor)
                        THEN GREATEST(EXTRACT(EPOCH FROM ((SELECT ends_at FROM latest_corridor) - CURRENT_TIMESTAMP))::bigint, 0)
                    ELSE 0
                END AS corridor_remaining_seconds,
                (SELECT active_sponsor_count FROM active_sponsors) AS active_sponsor_count,
                (SELECT sponsor_backed_admission_count FROM sponsor_backed_admissions) AS sponsor_backed_admission_count,
                (SELECT recent_admission_count_7d FROM recent_admissions) AS recent_admission_count_7d,
                (SELECT open_review_trigger_count FROM open_triggers) AS open_review_trigger_count,
                (SELECT open_review_case_count FROM open_cases) AS open_review_case_count,
                COALESCE((SELECT redacted_reason_code FROM latest_reason), realm.public_reason_code) AS latest_redacted_reason_code,
                (SELECT source_watermark_at FROM watermarks) AS source_watermark_at,
                (SELECT source_fact_count FROM source_counts) AS source_fact_count
            FROM dao.realms realm
            WHERE realm.realm_id = $1
            ",
            &[&realm_id],
        )
        .await
        .map_err(db_error)?;
    let source_watermark_at: DateTime<Utc> = row.get("source_watermark_at");
    let projection_lag_ms = (Utc::now() - source_watermark_at).num_milliseconds().max(0);
    client
        .execute(
            "
            INSERT INTO projection.realm_review_summaries (
                realm_id,
                realm_status,
                corridor_status,
                corridor_remaining_seconds,
                active_sponsor_count,
                sponsor_backed_admission_count,
                recent_admission_count_7d,
                open_review_trigger_count,
                open_review_case_count,
                latest_redacted_reason_code,
                source_watermark_at,
                source_fact_count,
                projection_lag_ms,
                rebuild_generation,
                last_projected_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, COALESCE($14, 1::bigint), CURRENT_TIMESTAMP)
            ON CONFLICT (realm_id) DO UPDATE
            SET realm_status = EXCLUDED.realm_status,
                corridor_status = EXCLUDED.corridor_status,
                corridor_remaining_seconds = EXCLUDED.corridor_remaining_seconds,
                active_sponsor_count = EXCLUDED.active_sponsor_count,
                sponsor_backed_admission_count = EXCLUDED.sponsor_backed_admission_count,
                recent_admission_count_7d = EXCLUDED.recent_admission_count_7d,
                open_review_trigger_count = EXCLUDED.open_review_trigger_count,
                open_review_case_count = EXCLUDED.open_review_case_count,
                latest_redacted_reason_code = EXCLUDED.latest_redacted_reason_code,
                source_watermark_at = EXCLUDED.source_watermark_at,
                source_fact_count = EXCLUDED.source_fact_count,
                projection_lag_ms = EXCLUDED.projection_lag_ms,
                rebuild_generation = COALESCE($14, projection.realm_review_summaries.rebuild_generation, 1::bigint),
                last_projected_at = CURRENT_TIMESTAMP
            ",
            &[
                &realm_id,
                &row.get::<_, String>("realm_status"),
                &row.get::<_, String>("corridor_status"),
                &row.get::<_, i64>("corridor_remaining_seconds"),
                &row.get::<_, i64>("active_sponsor_count"),
                &row.get::<_, i64>("sponsor_backed_admission_count"),
                &row.get::<_, i64>("recent_admission_count_7d"),
                &row.get::<_, i64>("open_review_trigger_count"),
                &row.get::<_, i64>("open_review_case_count"),
                &row.get::<_, String>("latest_redacted_reason_code"),
                &source_watermark_at,
                &row.get::<_, i64>("source_fact_count"),
                &projection_lag_ms,
                &rebuild_generation,
            ],
        )
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn count_corridor_admissions_tx<C: GenericClient + Sync>(
    client: &C,
    corridor_id: &Uuid,
) -> Result<i64, RealmBootstrapError> {
    Ok(client
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM dao.realm_admissions
            WHERE bootstrap_corridor_id = $1
              AND admission_status IN ('pending', 'admitted')
            ",
            &[corridor_id],
        )
        .await
        .map_err(db_error)?
        .get("count"))
}

async fn count_sponsor_backed_admissions_tx<C: GenericClient + Sync>(
    client: &C,
    sponsor_record_id: &Uuid,
) -> Result<i64, RealmBootstrapError> {
    Ok(client
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM dao.realm_admissions
            WHERE sponsor_record_id = $1
              AND admission_status IN ('pending', 'admitted')
            ",
            &[sponsor_record_id],
        )
        .await
        .map_err(db_error)?
        .get("count"))
}

async fn count_distinct_corridor_sponsors_tx<C: GenericClient + Sync>(
    client: &C,
    sponsor_record_id: &Uuid,
    corridor_row: &Row,
) -> Result<i64, RealmBootstrapError> {
    let corridor_id: Uuid = corridor_row.get("bootstrap_corridor_id");
    let sponsor_realm_id: String = corridor_row.get("realm_id");
    let current_sponsor_realm_id = client
        .query_one(
            "
            SELECT realm_id
            FROM dao.realm_sponsor_records
            WHERE realm_sponsor_record_id = $1
            ",
            &[sponsor_record_id],
        )
        .await
        .map_err(db_error)?
        .get::<_, String>("realm_id");
    if current_sponsor_realm_id != sponsor_realm_id {
        return Err(RealmBootstrapError::BadRequest(
            "sponsor record does not belong to the same realm".to_owned(),
        ));
    }
    Ok(client
        .query_one(
            "
            SELECT COUNT(DISTINCT sponsor_record_id) AS count
            FROM dao.realm_admissions
            WHERE bootstrap_corridor_id = $1
              AND sponsor_record_id IS NOT NULL
              AND admission_status IN ('pending', 'admitted')
            ",
            &[&corridor_id],
        )
        .await
        .map_err(db_error)?
        .get("count"))
}

async fn corridor_contains_sponsor_record_tx<C: GenericClient + Sync>(
    client: &C,
    sponsor_record_id: &Uuid,
    corridor_row: &Row,
) -> Result<bool, RealmBootstrapError> {
    let corridor_id: Uuid = corridor_row.get("bootstrap_corridor_id");
    Ok(client
        .query_one(
            "
            SELECT EXISTS (
                SELECT 1
                FROM dao.realm_admissions
                WHERE bootstrap_corridor_id = $1
                  AND sponsor_record_id = $2
                  AND admission_status IN ('pending', 'admitted')
            ) AS sponsor_present
            ",
            &[&corridor_id, sponsor_record_id],
        )
        .await
        .map_err(db_error)?
        .get("sponsor_present"))
}

async fn update_expired_corridors_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: Option<&str>,
) -> Result<(), RealmBootstrapError> {
    if let Some(realm_id) = realm_id {
        client
            .execute(
                "
                UPDATE dao.bootstrap_corridors
                SET corridor_status = 'expired',
                    updated_at = CURRENT_TIMESTAMP
                WHERE realm_id = $1
                  AND corridor_status IN ('active', 'cooling_down')
                  AND ends_at <= CURRENT_TIMESTAMP
                ",
                &[&realm_id],
            )
            .await
            .map_err(db_error)?;
    } else {
        client
            .execute(
                "
                UPDATE dao.bootstrap_corridors
                SET corridor_status = 'expired',
                    updated_at = CURRENT_TIMESTAMP
                WHERE corridor_status IN ('active', 'cooling_down')
                  AND ends_at <= CURRENT_TIMESTAMP
                ",
                &[],
            )
            .await
            .map_err(db_error)?;
    }
    Ok(())
}

async fn find_current_corridor_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
) -> Result<Option<Row>, RealmBootstrapError> {
    client
        .query_opt(
            "
            SELECT *
            FROM dao.bootstrap_corridors
            WHERE realm_id = $1
              AND corridor_status = 'active'
              AND starts_at <= CURRENT_TIMESTAMP
              AND ends_at > CURRENT_TIMESTAMP
            ORDER BY updated_at DESC, bootstrap_corridor_id DESC
            LIMIT 1
            FOR UPDATE
            ",
            &[&realm_id],
        )
        .await
        .map_err(db_error)
}

async fn find_latest_corridor_status_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
) -> Result<Option<String>, RealmBootstrapError> {
    client
        .query_opt(
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
        .map_err(db_error)
        .map(|row| row.map(|value| value.get("corridor_status")))
}

async fn find_realm_request_by_idempotency_tx<C: GenericClient + Sync>(
    client: &C,
    requester_account_id: &Uuid,
    request_idempotency_key: &str,
) -> Result<Option<Row>, RealmBootstrapError> {
    client
        .query_opt(
            "
            SELECT
                request.*,
                realm.realm_id AS created_realm_id
            FROM dao.realm_requests request
            LEFT JOIN dao.realms realm
              ON realm.created_from_realm_request_id = request.realm_request_id
            WHERE request.requested_by_account_id = $1
              AND request.request_idempotency_key = $2
            ",
            &[requester_account_id, &request_idempotency_key],
        )
        .await
        .map_err(db_error)
}

async fn find_sponsor_record_by_idempotency_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
    operator_id: &Uuid,
    request_idempotency_key: &str,
) -> Result<Option<Row>, RealmBootstrapError> {
    client
        .query_opt(
            "
            SELECT *
            FROM dao.realm_sponsor_records
            WHERE realm_id = $1
              AND approved_by_operator_id = $2
              AND request_idempotency_key = $3
            ",
            &[&realm_id, operator_id, &request_idempotency_key],
        )
        .await
        .map_err(db_error)
}

async fn find_admission_by_idempotency_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
    operator_id: &Uuid,
    request_idempotency_key: &str,
) -> Result<Option<Row>, RealmBootstrapError> {
    client
        .query_opt(
            "
            SELECT *
            FROM dao.realm_admissions
            WHERE realm_id = $1
              AND granted_by_actor_id = $2
              AND request_idempotency_key = $3
            ",
            &[&realm_id, operator_id, &request_idempotency_key],
        )
        .await
        .map_err(db_error)
}

async fn lock_realm_request_tx<C: GenericClient + Sync>(
    client: &C,
    realm_request_id: &Uuid,
) -> Result<Row, RealmBootstrapError> {
    client
        .query_opt(
            "
            SELECT
                request.*,
                realm.realm_id AS created_realm_id
            FROM dao.realm_requests request
            LEFT JOIN dao.realms realm
              ON realm.created_from_realm_request_id = request.realm_request_id
            WHERE request.realm_request_id = $1
            FOR UPDATE OF request
            ",
            &[realm_request_id],
        )
        .await
        .map_err(db_error)?
        .ok_or_else(|| RealmBootstrapError::NotFound("realm request was not found".to_owned()))
}

async fn lock_realm_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
) -> Result<Row, RealmBootstrapError> {
    client
        .query_opt(
            "
            SELECT *
            FROM dao.realms
            WHERE realm_id = $1
            FOR UPDATE
            ",
            &[&realm_id],
        )
        .await
        .map_err(db_error)?
        .ok_or_else(|| RealmBootstrapError::NotFound("realm was not found".to_owned()))
}

async fn ensure_realm_exists_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
) -> Result<(), RealmBootstrapError> {
    if client
        .query_opt("SELECT 1 FROM dao.realms WHERE realm_id = $1", &[&realm_id])
        .await
        .map_err(db_error)?
        .is_some()
    {
        Ok(())
    } else {
        Err(RealmBootstrapError::NotFound(
            "realm was not found".to_owned(),
        ))
    }
}

async fn lock_sponsor_record_tx<C: GenericClient + Sync>(
    client: &C,
    sponsor_record_id: &Uuid,
) -> Result<Row, RealmBootstrapError> {
    client
        .query_opt(
            "
            SELECT *
            FROM dao.realm_sponsor_records
            WHERE realm_sponsor_record_id = $1
            FOR UPDATE
            ",
            &[sponsor_record_id],
        )
        .await
        .map_err(db_error)?
        .ok_or_else(|| {
            RealmBootstrapError::NotFound("realm sponsor record was not found".to_owned())
        })
}

async fn lock_current_sponsor_record_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: &str,
    sponsor_account_id: &Uuid,
) -> Result<Row, RealmBootstrapError> {
    client
        .query_opt(
            "
            SELECT *
            FROM dao.realm_sponsor_records
            WHERE realm_id = $1
              AND sponsor_account_id = $2
            ORDER BY updated_at DESC, created_at DESC, realm_sponsor_record_id DESC
            LIMIT 1
            FOR UPDATE
            ",
            &[&realm_id, sponsor_account_id],
        )
        .await
        .map_err(db_error)?
        .ok_or_else(|| {
            RealmBootstrapError::NotFound("realm sponsor record was not found".to_owned())
        })
}

async fn ensure_slug_available_for_approval_tx<C: GenericClient + Sync>(
    client: &C,
    slug: &str,
    realm_request_id: &Uuid,
) -> Result<(), RealmBootstrapError> {
    let realm_exists = client
        .query_opt("SELECT 1 FROM dao.realms WHERE slug = $1", &[&slug])
        .await
        .map_err(db_error)?;
    if realm_exists.is_some() {
        return Err(RealmBootstrapError::BadRequest(
            "approved slug is already in use".to_owned(),
        ));
    }
    let request_exists = client
        .query_opt(
            "
            SELECT 1
            FROM dao.realm_requests
            WHERE slug_candidate = $1
              AND realm_request_id <> $2
              AND request_state IN ('requested', 'pending_review', 'approved')
            ",
            &[&slug, realm_request_id],
        )
        .await
        .map_err(db_error)?;
    if request_exists.is_some() {
        return Err(RealmBootstrapError::BadRequest(
            "approved slug is already reserved by another realm request".to_owned(),
        ));
    }
    Ok(())
}

async fn ensure_active_account_exists_tx<C: GenericClient + Sync>(
    client: &C,
    account_id: &Uuid,
) -> Result<(), RealmBootstrapError> {
    let row = client
        .query_one(
            "
            SELECT EXISTS (
                SELECT 1
                FROM core.accounts
                WHERE account_id = $1
                  AND account_state = 'active'
            ) AS exists
            ",
            &[account_id],
        )
        .await
        .map_err(db_error)?;
    if row.get::<_, bool>("exists") {
        Ok(())
    } else {
        Err(RealmBootstrapError::NotFound(
            "account was not found".to_owned(),
        ))
    }
}

async fn ensure_operator_role_tx<C: GenericClient + Sync>(
    client: &C,
    operator_id: &Uuid,
    allowed_roles: &[&str],
) -> Result<(), RealmBootstrapError> {
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
            &[operator_id, &allowed_roles],
        )
        .await
        .map_err(db_error)?;
    if row.get::<_, bool>("has_role") {
        Ok(())
    } else {
        Err(RealmBootstrapError::Unauthorized(
            "operator role is not allowed for realm bootstrap actions".to_owned(),
        ))
    }
}

fn create_realm_request_payload_hash(
    input: &CreateRealmRequestInput,
    slug_candidate: &str,
    proposed_sponsor_account_id: &Option<Uuid>,
    proposed_steward_account_id: &Option<Uuid>,
) -> String {
    hash_json_value(&json!({
        "schema_version": 1,
        "display_name": normalize_optional(Some(input.display_name.as_str())),
        "slug_candidate": slug_candidate,
        "purpose_text": normalize_optional(Some(input.purpose_text.as_str())),
        "venue_context_json": &input.venue_context_json,
        "expected_member_shape_json": &input.expected_member_shape_json,
        "bootstrap_rationale_text": normalize_optional(Some(input.bootstrap_rationale_text.as_str())),
        "proposed_sponsor_account_id": optional_uuid_hash_value(proposed_sponsor_account_id),
        "proposed_steward_account_id": optional_uuid_hash_value(proposed_steward_account_id),
    }))
}

fn approve_request_payload_hash(
    input: &ReviewRealmRequestInput,
    approved_slug: &str,
    approved_display_name: &str,
    steward_account_id: &Option<Uuid>,
) -> String {
    hash_json_value(&json!({
        "schema_version": 1,
        "target_realm_status": &input.target_realm_status,
        "approved_slug": approved_slug,
        "approved_display_name": approved_display_name,
        "review_reason_code": &input.review_reason_code,
        "steward_account_id": optional_uuid_hash_value(steward_account_id),
        "sponsor_quota_total": input.sponsor_quota_total,
        "corridor_starts_at": input.corridor_starts_at.as_ref().map(|value| value.to_rfc3339()),
        "corridor_ends_at": input.corridor_ends_at.as_ref().map(|value| value.to_rfc3339()),
        "corridor_member_cap": input.corridor_member_cap,
        "corridor_sponsor_cap": input.corridor_sponsor_cap,
        "review_threshold_json": &input.review_threshold_json,
    }))
}

fn reject_request_payload_hash(input: &RejectRealmRequestInput) -> String {
    hash_json_value(&json!({
        "schema_version": 1,
        "review_reason_code": &input.review_reason_code,
    }))
}

fn create_sponsor_record_payload_hash(
    input: &CreateRealmSponsorRecordInput,
    sponsor_account_id: &Uuid,
) -> String {
    hash_json_value(&json!({
        "schema_version": 1,
        "sponsor_account_id": sponsor_account_id.to_string(),
        "sponsor_status": &input.sponsor_status,
        "quota_total": input.quota_total,
        "status_reason_code": &input.status_reason_code,
    }))
}

fn create_admission_payload_hash(
    input: &CreateRealmAdmissionInput,
    account_id: &Uuid,
    sponsor_record_id: &Option<Uuid>,
    granted_by_actor_kind: &str,
) -> String {
    hash_json_value(&json!({
        "schema_version": 1,
        "account_id": account_id.to_string(),
        "sponsor_record_id": optional_uuid_hash_value(sponsor_record_id),
        "source_fact_kind": &input.source_fact_kind,
        "source_fact_id": &input.source_fact_id,
        "source_snapshot_json": &input.source_snapshot_json,
        "granted_by_actor_kind": granted_by_actor_kind,
    }))
}

fn ensure_request_payload_hash_matches(
    row: &Row,
    request_payload_hash: &str,
) -> Result<(), RealmBootstrapError> {
    let existing_hash: String = row.get("request_payload_hash");
    if existing_hash == request_payload_hash {
        Ok(())
    } else {
        Err(RealmBootstrapError::BadRequest(
            "request_idempotency_key was already used with a different realm request payload"
                .to_owned(),
        ))
    }
}

fn ensure_sponsor_record_payload_hash_matches(
    row: &Row,
    payload_hash: &str,
) -> Result<(), RealmBootstrapError> {
    let existing_hash: String = row.get("request_payload_hash");
    if existing_hash == payload_hash {
        Ok(())
    } else {
        Err(RealmBootstrapError::BadRequest(
            "request_idempotency_key was already used with a different sponsor record payload"
                .to_owned(),
        ))
    }
}

fn ensure_admission_payload_hash_matches(
    row: &Row,
    payload_hash: &str,
) -> Result<(), RealmBootstrapError> {
    let existing_hash: String = row.get("request_payload_hash");
    if existing_hash == payload_hash {
        Ok(())
    } else {
        Err(RealmBootstrapError::BadRequest(
            "request_idempotency_key was already used with a different realm admission payload"
                .to_owned(),
        ))
    }
}

fn ensure_request_review_replay_matches(
    row: &Row,
    review_decision_idempotency_key: &str,
    payload_hash: &str,
) -> Result<(), RealmBootstrapError> {
    let existing_key: Option<String> = row.get("review_decision_idempotency_key");
    let existing_hash: Option<String> = row.get("review_decision_payload_hash");
    if existing_key.as_deref() == Some(review_decision_idempotency_key)
        && existing_hash.as_deref() == Some(payload_hash)
    {
        Ok(())
    } else {
        Err(RealmBootstrapError::BadRequest(
            "review_decision_idempotency_key was already used with a different realm review payload"
                .to_owned(),
        ))
    }
}

fn hash_json_value(value: &Value) -> String {
    let digest = Sha256::digest(canonical_json_text(value).as_bytes());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn canonical_json_text(value: &Value) -> String {
    let mut output = String::new();
    write_canonical_json(value, &mut output);
    output
}

fn write_canonical_json(value: &Value, output: &mut String) {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(boolean) => output.push_str(if *boolean { "true" } else { "false" }),
        Value::Number(number) => {
            let _ = write!(output, "{number}");
        }
        Value::String(string) => {
            output.push_str(
                &serde_json::to_string(string).expect("serializing a JSON string should not fail"),
            );
        }
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical_json(value, output);
            }
            output.push(']');
        }
        Value::Object(map) => {
            output.push('{');
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(
                    &serde_json::to_string(key)
                        .expect("serializing a JSON object key should not fail"),
                );
                output.push(':');
                write_canonical_json(value, output);
            }
            output.push('}');
        }
    }
}

fn parse_uuid(value: &str, field_name: &str) -> Result<Uuid, RealmBootstrapError> {
    Uuid::parse_str(value.trim())
        .map_err(|_| RealmBootstrapError::BadRequest(format!("{field_name} must be a UUID")))
}

fn parse_optional_uuid(
    value: &Option<String>,
    field_name: &str,
) -> Result<Option<Uuid>, RealmBootstrapError> {
    match normalize_optional(value.as_deref()) {
        Some(value) => Ok(Some(parse_uuid(&value, field_name)?)),
        None => Ok(None),
    }
}

fn normalize_required(value: &str, field_name: &str) -> Result<String, RealmBootstrapError> {
    let value = value.trim();
    if value.is_empty() {
        Err(RealmBootstrapError::BadRequest(format!(
            "{field_name} is required"
        )))
    } else {
        Ok(value.to_owned())
    }
}

fn ensure_non_empty(field_name: &str, value: &str) -> Result<(), RealmBootstrapError> {
    let _ = normalize_required(value, field_name)?;
    Ok(())
}

fn ensure_non_empty_json_object(
    field_name: &str,
    value: &Value,
) -> Result<(), RealmBootstrapError> {
    match value {
        Value::Object(map) if !map.is_empty() => Ok(()),
        _ => Err(RealmBootstrapError::BadRequest(format!(
            "{field_name} must be a non-empty JSON object"
        ))),
    }
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn normalize_slug_candidate(value: &str) -> Result<String, RealmBootstrapError> {
    let raw = value.trim().to_ascii_lowercase().replace('_', "-");
    let raw = raw.split_whitespace().collect::<Vec<_>>().join("-");
    if raw.is_empty() || raw.len() > 64 {
        return Err(RealmBootstrapError::BadRequest(
            "slug_candidate must be between 1 and 64 characters".to_owned(),
        ));
    }
    if !raw.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    }) || raw.starts_with('-')
        || raw.ends_with('-')
        || raw.contains("--")
    {
        return Err(RealmBootstrapError::BadRequest(
            "slug_candidate must use lowercase letters, digits, and single hyphens".to_owned(),
        ));
    }
    Ok(raw)
}

fn validate_allowed(
    field_name: &str,
    value: &str,
    allowed: &[&str],
) -> Result<(), RealmBootstrapError> {
    if allowed.iter().any(|candidate| candidate == &value) {
        Ok(())
    } else {
        Err(RealmBootstrapError::BadRequest(format!(
            "{field_name} must be one of: {}",
            allowed.join(", ")
        )))
    }
}

fn optional_uuid_hash_value(value: &Option<Uuid>) -> Option<String> {
    value.map(|value| value.to_string())
}

fn require_corridor_fields(input: &ReviewRealmRequestInput) -> Result<(), RealmBootstrapError> {
    if input.corridor_starts_at.is_none()
        || input.corridor_ends_at.is_none()
        || input.corridor_member_cap.is_none()
        || input.corridor_sponsor_cap.is_none()
    {
        return Err(RealmBootstrapError::BadRequest(
            "limited bootstrap approval requires corridor fields".to_owned(),
        ));
    }
    Ok(())
}

fn trigger_intent(
    kind: &'static str,
    reason_code: &'static str,
    context_json: Value,
    fingerprint: String,
) -> TriggerIntent {
    TriggerIntent {
        kind,
        reason_code,
        context_json,
        fingerprint,
    }
}

async fn open_trigger_tx<C: GenericClient + Sync>(
    client: &C,
    realm_id: Option<&str>,
    trigger_kind: &str,
    redacted_reason_code: &str,
    related_account_id: Option<&Uuid>,
    related_realm_request_id: Option<&Uuid>,
    related_sponsor_record_id: Option<&Uuid>,
    context_json: &Value,
    trigger_fingerprint: &str,
) -> Result<(), RealmBootstrapError> {
    validate_allowed("trigger_kind", trigger_kind, REVIEW_TRIGGER_KINDS)?;
    validate_allowed("redacted_reason_code", redacted_reason_code, REASON_CODES)?;
    let realm_id_param = realm_id.map(str::to_owned);
    client
        .execute(
            "
            INSERT INTO dao.realm_review_triggers (
                realm_review_trigger_id,
                realm_id,
                trigger_kind,
                trigger_state,
                redacted_reason_code,
                related_account_id,
                related_realm_request_id,
                related_sponsor_record_id,
                context_json,
                trigger_fingerprint
            )
            VALUES ($1, $2, $3, 'open', $4, $5, $6, $7, $8, $9)
            ON CONFLICT (trigger_fingerprint)
                WHERE trigger_state = 'open'
            DO UPDATE SET
                updated_at = CURRENT_TIMESTAMP
            ",
            &[
                &Uuid::new_v4(),
                &realm_id_param,
                &trigger_kind,
                &redacted_reason_code,
                &related_account_id,
                &related_realm_request_id,
                &related_sponsor_record_id,
                context_json,
                &trigger_fingerprint,
            ],
        )
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn current_rebuild_generation_tx<C: GenericClient + Sync>(
    client: &C,
) -> Result<i64, RealmBootstrapError> {
    Ok(client
        .query_one(
            "
            SELECT GREATEST(
                COALESCE((SELECT MAX(rebuild_generation) FROM projection.realm_bootstrap_views), 0),
                COALESCE((SELECT MAX(rebuild_generation) FROM projection.realm_admission_views), 0),
                COALESCE((SELECT MAX(rebuild_generation) FROM projection.realm_review_summaries), 0)
            ) + 1 AS rebuild_generation
            ",
            &[],
        )
        .await
        .map_err(db_error)?
        .get("rebuild_generation"))
}

async fn operator_actor_kind_tx<C: GenericClient + Sync>(
    client: &C,
    operator_id: &Uuid,
) -> Result<String, RealmBootstrapError> {
    let row = client
        .query_one(
            "
            SELECT EXISTS (
                SELECT 1
                FROM core.operator_role_assignments
                WHERE operator_account_id = $1
                  AND operator_role = 'steward'
                  AND revoked_at IS NULL
            ) AS is_steward
            ",
            &[operator_id],
        )
        .await
        .map_err(db_error)?;
    if row.get::<_, bool>("is_steward") {
        Ok("steward".to_owned())
    } else {
        Ok("operator".to_owned())
    }
}

fn realm_request_from_row(row: &Row) -> Result<RealmRequestSnapshot, RealmBootstrapError> {
    let created_realm_id = if row
        .columns()
        .iter()
        .any(|column| column.name() == "created_realm_id")
    {
        row.get("created_realm_id")
    } else {
        None
    };
    Ok(RealmRequestSnapshot {
        realm_request_id: row.get::<_, Uuid>("realm_request_id").to_string(),
        requested_by_account_id: row.get::<_, Uuid>("requested_by_account_id").to_string(),
        display_name: row.get("display_name"),
        slug_candidate: row.get("slug_candidate"),
        purpose_text: row.get("purpose_text"),
        venue_context_json: row.get("venue_context_json"),
        expected_member_shape_json: row.get("expected_member_shape_json"),
        bootstrap_rationale_text: row.get("bootstrap_rationale_text"),
        proposed_sponsor_account_id: row
            .get::<_, Option<Uuid>>("proposed_sponsor_account_id")
            .map(|value| value.to_string()),
        proposed_steward_account_id: row
            .get::<_, Option<Uuid>>("proposed_steward_account_id")
            .map(|value| value.to_string()),
        request_state: row.get("request_state"),
        review_reason_code: row.get("review_reason_code"),
        reviewed_by_operator_id: row
            .get::<_, Option<Uuid>>("reviewed_by_operator_id")
            .map(|value| value.to_string()),
        reviewed_at: row.get("reviewed_at"),
        created_realm_id,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn realm_from_row(row: &Row) -> Result<RealmSnapshot, RealmBootstrapError> {
    Ok(RealmSnapshot {
        realm_id: row.get("realm_id"),
        slug: row.get("slug"),
        display_name: row.get("display_name"),
        realm_status: row.get("realm_status"),
        public_reason_code: row.get("public_reason_code"),
        created_from_realm_request_id: row
            .get::<_, Uuid>("created_from_realm_request_id")
            .to_string(),
        steward_account_id: row
            .get::<_, Option<Uuid>>("steward_account_id")
            .map(|value| value.to_string()),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn realm_sponsor_record_from_row(
    row: &Row,
) -> Result<RealmSponsorRecordSnapshot, RealmBootstrapError> {
    Ok(RealmSponsorRecordSnapshot {
        realm_sponsor_record_id: row.get::<_, Uuid>("realm_sponsor_record_id").to_string(),
        realm_id: row.get("realm_id"),
        sponsor_account_id: row.get::<_, Uuid>("sponsor_account_id").to_string(),
        sponsor_status: row.get("sponsor_status"),
        quota_total: row.get("quota_total"),
        status_reason_code: row.get("status_reason_code"),
        approved_by_operator_id: row.get::<_, Uuid>("approved_by_operator_id").to_string(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn realm_admission_from_row(row: &Row) -> Result<RealmAdmissionSnapshot, RealmBootstrapError> {
    Ok(RealmAdmissionSnapshot {
        realm_admission_id: row.get::<_, Uuid>("realm_admission_id").to_string(),
        realm_id: row.get("realm_id"),
        account_id: row.get::<_, Uuid>("account_id").to_string(),
        admission_kind: row.get("admission_kind"),
        admission_status: row.get("admission_status"),
        sponsor_record_id: row
            .get::<_, Option<Uuid>>("sponsor_record_id")
            .map(|value| value.to_string()),
        bootstrap_corridor_id: row
            .get::<_, Option<Uuid>>("bootstrap_corridor_id")
            .map(|value| value.to_string()),
        granted_by_actor_kind: row.get("granted_by_actor_kind"),
        granted_by_actor_id: row.get::<_, Uuid>("granted_by_actor_id").to_string(),
        review_reason_code: row.get("review_reason_code"),
        source_fact_kind: row.get("source_fact_kind"),
        source_fact_id: row.get("source_fact_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn realm_bootstrap_view_from_row(
    row: &Row,
) -> Result<RealmBootstrapViewSnapshot, RealmBootstrapError> {
    Ok(RealmBootstrapViewSnapshot {
        realm_id: row.get("realm_id"),
        slug: row.get("slug"),
        display_name: row.get("display_name"),
        realm_status: row.get("realm_status"),
        admission_posture: row.get("admission_posture"),
        corridor_status: row.get("corridor_status"),
        public_reason_code: row.get("public_reason_code"),
        sponsor_display_state: row.get("sponsor_display_state"),
        source_watermark_at: row.get("source_watermark_at"),
        source_fact_count: row.get("source_fact_count"),
        projection_lag_ms: row.get("projection_lag_ms"),
        rebuild_generation: row.get("rebuild_generation"),
        last_projected_at: row.get("last_projected_at"),
    })
}

fn realm_admission_view_from_row(
    row: &Row,
) -> Result<RealmAdmissionViewSnapshot, RealmBootstrapError> {
    Ok(RealmAdmissionViewSnapshot {
        realm_id: row.get("realm_id"),
        account_id: row.get::<_, Uuid>("account_id").to_string(),
        admission_status: row.get("admission_status"),
        admission_kind: row.get("admission_kind"),
        public_reason_code: row.get("public_reason_code"),
        source_watermark_at: row.get("source_watermark_at"),
        source_fact_count: row.get("source_fact_count"),
        projection_lag_ms: row.get("projection_lag_ms"),
        rebuild_generation: row.get("rebuild_generation"),
        last_projected_at: row.get("last_projected_at"),
    })
}

fn realm_review_trigger_from_row(
    row: &Row,
) -> Result<RealmReviewTriggerSnapshot, RealmBootstrapError> {
    Ok(RealmReviewTriggerSnapshot {
        realm_review_trigger_id: row.get::<_, Uuid>("realm_review_trigger_id").to_string(),
        realm_id: row.get("realm_id"),
        trigger_kind: row.get("trigger_kind"),
        trigger_state: row.get("trigger_state"),
        redacted_reason_code: row.get("redacted_reason_code"),
        related_account_id: row
            .get::<_, Option<Uuid>>("related_account_id")
            .map(|value| value.to_string()),
        related_realm_request_id: row
            .get::<_, Option<Uuid>>("related_realm_request_id")
            .map(|value| value.to_string()),
        related_sponsor_record_id: row
            .get::<_, Option<Uuid>>("related_sponsor_record_id")
            .map(|value| value.to_string()),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        resolved_at: row.get("resolved_at"),
    })
}

fn realm_review_summary_from_row(
    row: &Row,
    trigger_rows: &[Row],
) -> Result<RealmReviewSummarySnapshot, RealmBootstrapError> {
    Ok(RealmReviewSummarySnapshot {
        realm_id: row.get("realm_id"),
        realm_status: row.get("realm_status"),
        corridor_status: row.get("corridor_status"),
        corridor_remaining_seconds: row.get("corridor_remaining_seconds"),
        active_sponsor_count: row.get("active_sponsor_count"),
        sponsor_backed_admission_count: row.get("sponsor_backed_admission_count"),
        recent_admission_count_7d: row.get("recent_admission_count_7d"),
        open_review_trigger_count: row.get("open_review_trigger_count"),
        open_review_case_count: row.get("open_review_case_count"),
        latest_redacted_reason_code: row.get("latest_redacted_reason_code"),
        source_watermark_at: row.get("source_watermark_at"),
        source_fact_count: row.get("source_fact_count"),
        projection_lag_ms: row.get("projection_lag_ms"),
        rebuild_generation: row.get("rebuild_generation"),
        last_projected_at: row.get("last_projected_at"),
        open_review_triggers: trigger_rows
            .iter()
            .map(realm_review_trigger_from_row)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn db_error(error: tokio_postgres::Error) -> RealmBootstrapError {
    let code = error.code().map(|code| code.code().to_owned());
    let constraint = error
        .as_db_error()
        .and_then(|db_error| db_error.constraint().map(str::to_owned));
    if matches!(error.code(), Some(&SqlState::UNIQUE_VIOLATION)) {
        match constraint.as_deref() {
            Some("realm_requests_open_slug_candidate_unique") => {
                return RealmBootstrapError::BadRequest(
                    "slug_candidate already has an open realm request".to_owned(),
                );
            }
            Some("realm_requests_request_idempotency_unique") => {
                return RealmBootstrapError::BadRequest(
                    "request_idempotency_key was already used by this requester".to_owned(),
                );
            }
            Some("realms_slug_key") => {
                return RealmBootstrapError::BadRequest(
                    "approved slug is already in use".to_owned(),
                );
            }
            Some("realm_sponsor_records_active_unique") => {
                return RealmBootstrapError::BadRequest(
                    "sponsor account already has an open sponsor record for this realm".to_owned(),
                );
            }
            Some("realm_sponsor_records_idempotency_unique") => {
                return RealmBootstrapError::BadRequest(
                    "request_idempotency_key was already used by this operator for this realm"
                        .to_owned(),
                );
            }
            Some("realm_admissions_active_unique") => {
                return RealmBootstrapError::BadRequest(
                    "account already has a pending or admitted realm admission".to_owned(),
                );
            }
            Some("realm_admissions_idempotency_unique") => {
                return RealmBootstrapError::BadRequest(
                    "request_idempotency_key was already used by this operator for this realm"
                        .to_owned(),
                );
            }
            _ => {}
        }
    }
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
    RealmBootstrapError::Database {
        message: error.to_string(),
        code,
        constraint,
        retryable,
    }
}
