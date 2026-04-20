use std::{fmt::Write as _, sync::Arc};

use musubi_db_runtime::{DbConfig, connect_writer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_postgres::{Client, GenericClient, Row, Transaction, error::SqlState};
use uuid::Uuid;

use super::types::{
    AppendRoomProgressionFactInput, CreateRoomProgressionInput, RoomProgressionError,
    RoomProgressionFactSnapshot, RoomProgressionRebuildSnapshot, RoomProgressionTrackSnapshot,
    RoomProgressionViewSnapshot,
};

const ROOM_STAGES: &[&str] = &["intent", "coordination", "relationship", "sealed"];
const TRANSITION_KINDS: &[&str] = &[
    "advance_to_coordination",
    "advance_to_relationship",
    "seal",
    "restore",
    "mute",
    "block",
    "withdraw",
];
const TRIGGERED_BY_KINDS: &[&str] = &["system", "participant", "operator"];
const OPERATOR_TRANSITION_ROLES: &[&str] = &["reviewer", "approver", "steward"];
const USER_FACING_REASON_CODES: &[&str] = &[
    "room_created",
    "mutual_intent_acknowledged",
    "promise_draft_created",
    "bounded_coordination_accepted",
    "coordination_completed",
    "qualifying_promise_completed",
    "safety_review",
    "policy_review",
    "manual_hold_safety_review",
    "appeal_received",
    "proof_missing",
    "proof_inconclusive",
    "duplicate_or_invalid",
    "resolved_no_action",
    "restricted_after_review",
    "restored_after_review",
    "user_withdrew",
    "user_blocked",
    "user_muted",
];

#[derive(Clone)]
pub struct RoomProgressionStore {
    client: Arc<Mutex<Client>>,
}

impl RoomProgressionStore {
    pub(crate) async fn connect(config: &DbConfig) -> musubi_db_runtime::Result<Self> {
        let client = connect_writer(config, "musubi-backend room-progression").await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }

    pub(crate) async fn reset_for_test(&self) -> Result<(), RoomProgressionError> {
        let client = self.client.lock().await;
        client
            .batch_execute(
                "
                DELETE FROM projection.room_progression_views;
                DELETE FROM dao.room_progression_facts;
                DELETE FROM dao.room_progression_tracks;
                ",
            )
            .await
            .map_err(db_error)?;
        Ok(())
    }

    pub async fn create_room_progression(
        &self,
        input: CreateRoomProgressionInput,
    ) -> Result<RoomProgressionTrackSnapshot, RoomProgressionError> {
        let realm_id = normalize_required(&input.realm_id, "realm_id")?;
        validate_allowed(
            "user_facing_reason_code",
            &input.user_facing_reason_code,
            USER_FACING_REASON_CODES,
        )?;
        require_non_empty("source_fact_kind", &input.source_fact_kind)?;
        require_non_empty("source_fact_id", &input.source_fact_id)?;
        let participants = parse_participant_pair(&input.participant_account_ids)?;
        let related_promise_intent_id = parse_optional_uuid(
            &input.related_promise_intent_id,
            "related promise intent id",
        )?;
        let related_settlement_case_id = parse_optional_uuid(
            &input.related_settlement_case_id,
            "related settlement case id",
        )?;
        let request_idempotency_key = normalize_optional(&input.request_idempotency_key)
            .ok_or_else(|| {
                RoomProgressionError::BadRequest(
                    "room progression creation requires request_idempotency_key".to_owned(),
                )
            })?;
        let request_payload_hash = create_room_progression_payload_hash(
            &input,
            &realm_id,
            &participants,
            &related_promise_intent_id,
            &related_settlement_case_id,
        );

        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;

        let row = if let Some(existing) =
            find_existing_track_by_idempotency(&tx, &request_idempotency_key).await?
        {
            ensure_track_matches_payload_hash(&existing, &request_payload_hash)?;
            let room_progression_id: Uuid = existing.get("room_progression_id");
            refresh_room_progression_view_tx(&tx, &room_progression_id, None).await?;
            existing
        } else {
            ensure_account_exists_tx(&tx, &participants.0).await?;
            ensure_account_exists_tx(&tx, &participants.1).await?;
            let room_progression_id = Uuid::new_v4();
            let maybe_row = tx
                .query_opt(
                    "
                    INSERT INTO dao.room_progression_tracks (
                        room_progression_id,
                        realm_id,
                        participant_a_account_id,
                        participant_b_account_id,
                        related_promise_intent_id,
                        related_settlement_case_id,
                        current_stage,
                        current_status_code,
                        current_user_facing_reason_code,
                        source_fact_kind,
                        source_fact_id,
                        source_snapshot_json,
                        request_idempotency_key,
                        request_payload_hash
                    )
                    VALUES (
                        $1, $2, $3, $4, $5, $6,
                        'intent',
                        'intent_open',
                        $7,
                        $8, $9, $10, $11, $12
                    )
                    ON CONFLICT (request_idempotency_key)
                        WHERE request_idempotency_key IS NOT NULL
                    DO NOTHING
                    RETURNING
                        room_progression_id,
                        realm_id,
                        participant_a_account_id,
                        participant_b_account_id,
                        related_promise_intent_id,
                        related_settlement_case_id,
                        current_stage,
                        current_status_code,
                        current_user_facing_reason_code,
                        current_review_case_id,
                        source_fact_kind,
                        source_fact_id,
                        created_at,
                        updated_at
                    ",
                    &[
                        &room_progression_id,
                        &realm_id,
                        &participants.0,
                        &participants.1,
                        &related_promise_intent_id,
                        &related_settlement_case_id,
                        &input.user_facing_reason_code,
                        &input.source_fact_kind,
                        &input.source_fact_id,
                        &input.source_snapshot_json,
                        &Some(request_idempotency_key.clone()),
                        &request_payload_hash,
                    ],
                )
                .await
                .map_err(db_error)?;

            if let Some(row) = maybe_row {
                let fact_payload_hash = create_room_progression_fact_payload_hash(
                    "create",
                    "intent",
                    "intent",
                    "intent_open",
                    &input.user_facing_reason_code,
                    "system",
                    &None,
                    &input.source_fact_kind,
                    &input.source_fact_id,
                    &input.source_snapshot_json,
                    &None,
                );
                insert_room_progression_fact_tx(
                    &tx,
                    &room_progression_id,
                    "intent",
                    "intent",
                    "create",
                    "intent_open",
                    &input.user_facing_reason_code,
                    "system",
                    &None,
                    &input.source_fact_kind,
                    &input.source_fact_id,
                    &input.source_snapshot_json,
                    &None,
                    &None,
                    &fact_payload_hash,
                )
                .await?;
                refresh_room_progression_view_tx(&tx, &room_progression_id, None).await?;
                row
            } else {
                let existing =
                    find_existing_track_by_idempotency(&tx, &request_idempotency_key).await?;
                let existing = existing.ok_or_else(|| {
                    RoomProgressionError::Internal(
                        "room progression idempotency conflict could not be reloaded".to_owned(),
                    )
                })?;
                ensure_track_matches_payload_hash(&existing, &request_payload_hash)?;
                let room_progression_id: Uuid = existing.get("room_progression_id");
                refresh_room_progression_view_tx(&tx, &room_progression_id, None).await?;
                existing
            }
        };

        tx.commit().await.map_err(db_error)?;
        Ok(room_progression_track_from_row(&row))
    }

    pub async fn append_room_progression_fact(
        &self,
        room_progression_id: &str,
        input: AppendRoomProgressionFactInput,
    ) -> Result<RoomProgressionFactSnapshot, RoomProgressionError> {
        let room_progression_id = parse_uuid(room_progression_id, "room progression id")?;
        validate_allowed("transition_kind", &input.transition_kind, TRANSITION_KINDS)?;
        validate_allowed("to_stage", &input.to_stage, ROOM_STAGES)?;
        validate_allowed(
            "user_facing_reason_code",
            &input.user_facing_reason_code,
            USER_FACING_REASON_CODES,
        )?;
        validate_allowed(
            "triggered_by_kind",
            &input.triggered_by_kind,
            TRIGGERED_BY_KINDS,
        )?;
        require_non_empty("source_fact_kind", &input.source_fact_kind)?;
        require_non_empty("source_fact_id", &input.source_fact_id)?;
        let triggered_by_account_id =
            parse_optional_uuid(&input.triggered_by_account_id, "triggered by account id")?;
        let review_case_id = parse_optional_uuid(&input.review_case_id, "review case id")?;
        let fact_idempotency_key =
            normalize_optional(&input.fact_idempotency_key).ok_or_else(|| {
                RoomProgressionError::BadRequest(
                    "room progression fact appends require fact_idempotency_key".to_owned(),
                )
            })?;
        let fact_idempotency_key_param = Some(fact_idempotency_key.clone());

        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        // Serialize fact appends per room so same-room idempotent retries cannot race past the
        // existing-fact lookup and collide at the INSERT boundary.
        let track = select_track_for_update_tx(&tx, &room_progression_id).await?;
        if let Some(existing) =
            find_existing_fact_by_idempotency(&tx, &room_progression_id, &fact_idempotency_key)
                .await?
        {
            let expected_hash = room_progression_fact_payload_hash_from_input(&existing, &input)?;
            ensure_fact_matches_payload_hash(&existing, &expected_hash)?;
            refresh_room_progression_view_tx(&tx, &room_progression_id, None).await?;
            tx.commit().await.map_err(db_error)?;
            return Ok(room_progression_fact_from_row(&existing));
        }

        let from_stage: String = track.get("current_stage");
        let current_status_code: String = track.get("current_status_code");
        validate_triggered_by_tx(
            &tx,
            &track,
            &input.transition_kind,
            &input.triggered_by_kind,
            &triggered_by_account_id,
        )
        .await?;
        if let Some(review_case_id) = review_case_id.as_ref() {
            ensure_review_case_matches_room_scope_tx(&tx, &track, review_case_id).await?;
        }

        let status_code = next_status_code(
            &input.transition_kind,
            &from_stage,
            &input.to_stage,
            &input.user_facing_reason_code,
        )?;
        validate_transition_tx(
            &tx,
            &track,
            &from_stage,
            &current_status_code,
            &input.transition_kind,
            &input.triggered_by_kind,
            &input.to_stage,
            &status_code,
            review_case_id.as_ref(),
        )
        .await?;

        let fact_payload_hash = create_room_progression_fact_payload_hash(
            &input.transition_kind,
            &from_stage,
            &input.to_stage,
            &status_code,
            &input.user_facing_reason_code,
            &input.triggered_by_kind,
            &triggered_by_account_id,
            &input.source_fact_kind,
            &input.source_fact_id,
            &input.source_snapshot_json,
            &review_case_id,
        );
        let row = insert_room_progression_fact_tx(
            &tx,
            &room_progression_id,
            &from_stage,
            &input.to_stage,
            &input.transition_kind,
            &status_code,
            &input.user_facing_reason_code,
            &input.triggered_by_kind,
            &triggered_by_account_id,
            &input.source_fact_kind,
            &input.source_fact_id,
            &input.source_snapshot_json,
            &review_case_id,
            &fact_idempotency_key_param,
            &fact_payload_hash,
        )
        .await?;
        let current_review_case_id: Option<Uuid> = track.get("current_review_case_id");
        let next_review_case_id = if input.to_stage == "sealed" {
            current_review_case_id.or(review_case_id)
        } else {
            None
        };

        tx.execute(
            "
            UPDATE dao.room_progression_tracks
            SET current_stage = $2,
                current_status_code = $3,
                current_user_facing_reason_code = $4,
                current_review_case_id = $5,
                updated_at = CURRENT_TIMESTAMP
            WHERE room_progression_id = $1
            ",
            &[
                &room_progression_id,
                &input.to_stage,
                &status_code,
                &input.user_facing_reason_code,
                &next_review_case_id,
            ],
        )
        .await
        .map_err(db_error)?;
        refresh_room_progression_view_tx(&tx, &room_progression_id, None).await?;
        tx.commit().await.map_err(db_error)?;

        Ok(room_progression_fact_from_row(&row))
    }

    pub async fn get_room_progression_view_for_participant(
        &self,
        account_id: &str,
        room_progression_id: &str,
    ) -> Result<RoomProgressionViewSnapshot, RoomProgressionError> {
        let account_id = parse_uuid(account_id, "account id")?;
        let room_progression_id = parse_uuid(room_progression_id, "room progression id")?;
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "
                SELECT
                    room_progression_id,
                    realm_id,
                    participant_a_account_id,
                    participant_b_account_id,
                    visible_stage,
                    status_code,
                    user_facing_reason_code,
                    review_case_id,
                    review_pending,
                    review_status,
                    appeal_available,
                    evidence_requested,
                    source_watermark_at,
                    source_fact_count,
                    projection_lag_ms,
                    rebuild_generation,
                    last_projected_at
                FROM projection.room_progression_views
                WHERE room_progression_id = $1
                  AND $2 IN (participant_a_account_id, participant_b_account_id)
                ",
                &[&room_progression_id, &account_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(|| {
                RoomProgressionError::NotFound("room progression view was not found".to_owned())
            })?;

        Ok(room_progression_view_from_row(&row))
    }

    pub async fn rebuild_room_progression_views(
        &self,
    ) -> Result<RoomProgressionRebuildSnapshot, RoomProgressionError> {
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        tx.query_one(
            "
            SELECT pg_advisory_xact_lock(
                hashtext('projection.room_progression_views.rebuild')::bigint
            )
            ",
            &[],
        )
        .await
        .map_err(db_error)?;
        let rows = tx
            .query(
                "
                SELECT room_progression_id
                FROM dao.room_progression_tracks
                ORDER BY created_at ASC
                ",
                &[],
            )
            .await
            .map_err(db_error)?;
        let generation_row = tx
            .query_one(
                "
                SELECT COALESCE(MAX(rebuild_generation), 0)::bigint + 1 AS rebuild_generation
                FROM projection.room_progression_views
                ",
                &[],
            )
            .await
            .map_err(db_error)?;
        let rebuild_generation: i64 = generation_row.get("rebuild_generation");
        for row in &rows {
            let room_progression_id: Uuid = row.get("room_progression_id");
            refresh_room_progression_view_tx(&tx, &room_progression_id, Some(rebuild_generation))
                .await?;
        }
        tx.commit().await.map_err(db_error)?;

        Ok(RoomProgressionRebuildSnapshot {
            rebuilt_count: rows.len() as i64,
        })
    }
}

async fn insert_room_progression_fact_tx<C: GenericClient + Sync>(
    client: &C,
    room_progression_id: &Uuid,
    from_stage: &str,
    to_stage: &str,
    transition_kind: &str,
    status_code: &str,
    user_facing_reason_code: &str,
    triggered_by_kind: &str,
    triggered_by_account_id: &Option<Uuid>,
    source_fact_kind: &str,
    source_fact_id: &str,
    source_snapshot_json: &Value,
    review_case_id: &Option<Uuid>,
    fact_idempotency_key: &Option<String>,
    fact_payload_hash: &str,
) -> Result<Row, RoomProgressionError> {
    client
        .query_one(
            "
            INSERT INTO dao.room_progression_facts (
                room_progression_fact_id,
                room_progression_id,
                from_stage,
                to_stage,
                transition_kind,
                status_code,
                user_facing_reason_code,
                triggered_by_kind,
                triggered_by_account_id,
                source_fact_kind,
                source_fact_id,
                source_snapshot_json,
                review_case_id,
                fact_idempotency_key,
                fact_payload_hash
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING
                room_progression_fact_id,
                room_progression_id,
                from_stage,
                to_stage,
                transition_kind,
                status_code,
                user_facing_reason_code,
                triggered_by_kind,
                triggered_by_account_id,
                source_fact_kind,
                source_fact_id,
                review_case_id,
                recorded_at,
                fact_payload_hash
            ",
            &[
                &Uuid::new_v4(),
                room_progression_id,
                &from_stage,
                &to_stage,
                &transition_kind,
                &status_code,
                &user_facing_reason_code,
                &triggered_by_kind,
                triggered_by_account_id,
                &source_fact_kind,
                &source_fact_id,
                source_snapshot_json,
                review_case_id,
                fact_idempotency_key,
                &fact_payload_hash,
            ],
        )
        .await
        .map_err(db_error)
}

async fn refresh_room_progression_view_tx<C: GenericClient + Sync>(
    client: &C,
    room_progression_id: &Uuid,
    rebuild_generation: Option<i64>,
) -> Result<(), RoomProgressionError> {
    client
        .execute(
            "
            WITH fact_stats AS (
                SELECT
                    room_progression_id,
                    count(*)::bigint AS source_fact_count,
                    max(recorded_at) AS latest_fact_at
                FROM dao.room_progression_facts
                WHERE room_progression_id = $1
                GROUP BY room_progression_id
            ),
            shaped AS (
                SELECT
                    track.room_progression_id,
                    track.realm_id,
                    track.participant_a_account_id,
                    track.participant_b_account_id,
                    track.current_stage AS visible_stage,
                    track.current_status_code AS status_code,
                    track.current_user_facing_reason_code AS user_facing_reason_code,
                    track.current_review_case_id AS review_case_id,
                    (
                        track.current_review_case_id IS NOT NULL
                        AND COALESCE(
                            review_view.user_facing_status IN (
                                'pending_review',
                                'under_review',
                                'evidence_requested',
                                'appeal_submitted'
                            ),
                            true
                        )
                    ) AS review_pending,
                    CASE
                        WHEN track.current_review_case_id IS NULL THEN NULL
                        ELSE COALESCE(review_view.user_facing_status, 'pending_review')
                    END AS review_status,
                    COALESCE(review_view.appeal_available, false) AS appeal_available,
                    COALESCE(review_view.evidence_requested, false) AS evidence_requested,
                    GREATEST(
                        track.updated_at,
                        COALESCE(fact_stats.latest_fact_at, track.updated_at),
                        COALESCE(review_view.source_watermark_at, track.updated_at)
                    ) AS source_watermark_at,
                    COALESCE(fact_stats.source_fact_count, 0)::bigint
                        + CASE WHEN track.current_review_case_id IS NULL THEN 0 ELSE 1 END
                        AS source_fact_count
                FROM dao.room_progression_tracks track
                LEFT JOIN fact_stats
                    ON fact_stats.room_progression_id = track.room_progression_id
                LEFT JOIN projection.review_status_views review_view
                    ON review_view.review_case_id = track.current_review_case_id
                WHERE track.room_progression_id = $1
            )
            INSERT INTO projection.room_progression_views (
                room_progression_id,
                realm_id,
                participant_a_account_id,
                participant_b_account_id,
                visible_stage,
                status_code,
                user_facing_reason_code,
                review_case_id,
                review_pending,
                review_status,
                appeal_available,
                evidence_requested,
                source_watermark_at,
                source_fact_count,
                projection_lag_ms,
                rebuild_generation,
                last_projected_at
            )
            SELECT
                shaped.room_progression_id,
                shaped.realm_id,
                shaped.participant_a_account_id,
                shaped.participant_b_account_id,
                shaped.visible_stage,
                shaped.status_code,
                shaped.user_facing_reason_code,
                shaped.review_case_id,
                shaped.review_pending,
                shaped.review_status,
                shaped.appeal_available,
                shaped.evidence_requested,
                shaped.source_watermark_at,
                shaped.source_fact_count,
                GREATEST(
                    0,
                    (EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - shaped.source_watermark_at)) * 1000)::bigint
                ),
                COALESCE($2::bigint, existing.rebuild_generation, 1),
                CURRENT_TIMESTAMP
            FROM shaped
            LEFT JOIN projection.room_progression_views existing
                ON existing.room_progression_id = shaped.room_progression_id
            ON CONFLICT (room_progression_id)
            DO UPDATE SET
                realm_id = EXCLUDED.realm_id,
                participant_a_account_id = EXCLUDED.participant_a_account_id,
                participant_b_account_id = EXCLUDED.participant_b_account_id,
                visible_stage = EXCLUDED.visible_stage,
                status_code = EXCLUDED.status_code,
                user_facing_reason_code = EXCLUDED.user_facing_reason_code,
                review_case_id = EXCLUDED.review_case_id,
                review_pending = EXCLUDED.review_pending,
                review_status = EXCLUDED.review_status,
                appeal_available = EXCLUDED.appeal_available,
                evidence_requested = EXCLUDED.evidence_requested,
                source_watermark_at = EXCLUDED.source_watermark_at,
                source_fact_count = EXCLUDED.source_fact_count,
                projection_lag_ms = EXCLUDED.projection_lag_ms,
                rebuild_generation = EXCLUDED.rebuild_generation,
                last_projected_at = EXCLUDED.last_projected_at
            ",
            &[room_progression_id, &rebuild_generation],
        )
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn find_existing_track_by_idempotency<C: GenericClient + Sync>(
    client: &C,
    request_idempotency_key: &str,
) -> Result<Option<Row>, RoomProgressionError> {
    client
        .query_opt(
            "
            SELECT
                room_progression_id,
                realm_id,
                participant_a_account_id,
                participant_b_account_id,
                related_promise_intent_id,
                related_settlement_case_id,
                current_stage,
                current_status_code,
                current_user_facing_reason_code,
                current_review_case_id,
                source_fact_kind,
                source_fact_id,
                created_at,
                updated_at,
                request_payload_hash
            FROM dao.room_progression_tracks
            WHERE request_idempotency_key = $1
            ",
            &[&request_idempotency_key],
        )
        .await
        .map_err(db_error)
}

async fn find_existing_fact_by_idempotency<C: GenericClient + Sync>(
    client: &C,
    room_progression_id: &Uuid,
    fact_idempotency_key: &str,
) -> Result<Option<Row>, RoomProgressionError> {
    client
        .query_opt(
            "
            SELECT
                room_progression_fact_id,
                room_progression_id,
                from_stage,
                to_stage,
                transition_kind,
                status_code,
                user_facing_reason_code,
                triggered_by_kind,
                triggered_by_account_id,
                source_fact_kind,
                source_fact_id,
                review_case_id,
                recorded_at,
                fact_payload_hash
            FROM dao.room_progression_facts
            WHERE room_progression_id = $1
              AND fact_idempotency_key = $2
            ",
            &[room_progression_id, &fact_idempotency_key],
        )
        .await
        .map_err(db_error)
}

async fn select_track_for_update_tx(
    tx: &Transaction<'_>,
    room_progression_id: &Uuid,
) -> Result<Row, RoomProgressionError> {
    tx.query_opt(
        "
        SELECT
            room_progression_id,
            realm_id,
            participant_a_account_id,
            participant_b_account_id,
            related_promise_intent_id,
            related_settlement_case_id,
            current_stage,
            current_status_code,
            current_user_facing_reason_code,
            current_review_case_id,
            source_fact_kind,
            source_fact_id,
            created_at,
            updated_at
        FROM dao.room_progression_tracks
        WHERE room_progression_id = $1
        FOR UPDATE
        ",
        &[room_progression_id],
    )
    .await
    .map_err(db_error)?
    .ok_or_else(|| RoomProgressionError::NotFound("room progression was not found".to_owned()))
}

async fn validate_triggered_by_tx<C: GenericClient + Sync>(
    client: &C,
    track: &Row,
    transition_kind: &str,
    triggered_by_kind: &str,
    triggered_by_account_id: &Option<Uuid>,
) -> Result<(), RoomProgressionError> {
    if transition_kind == "restore" && triggered_by_kind != "operator" {
        return Err(RoomProgressionError::BadRequest(
            "restore transitions must be operator-triggered".to_owned(),
        ));
    }
    match triggered_by_kind {
        "participant" => {
            let Some(account_id) = triggered_by_account_id else {
                return Err(RoomProgressionError::BadRequest(
                    "participant-triggered transitions require triggered_by_account_id".to_owned(),
                ));
            };
            let participant_a: Uuid = track.get("participant_a_account_id");
            let participant_b: Uuid = track.get("participant_b_account_id");
            if *account_id != participant_a && *account_id != participant_b {
                return Err(RoomProgressionError::Unauthorized(
                    "participant is not a room member".to_owned(),
                ));
            }
            ensure_account_exists_tx(client, account_id).await?;
            Ok(())
        }
        "operator" => {
            let Some(account_id) = triggered_by_account_id else {
                return Err(RoomProgressionError::BadRequest(
                    "operator-triggered transitions require triggered_by_account_id".to_owned(),
                ));
            };
            ensure_operator_transition_role_tx(client, account_id).await
        }
        "system" => {
            if triggered_by_account_id.is_some() {
                return Err(RoomProgressionError::BadRequest(
                    "system-triggered transitions must not include triggered_by_account_id"
                        .to_owned(),
                ));
            }
            Ok(())
        }
        _ => Err(RoomProgressionError::BadRequest(
            "triggered_by_kind is not supported".to_owned(),
        )),
    }
}

async fn validate_transition_tx<C: GenericClient + Sync>(
    client: &C,
    track: &Row,
    from_stage: &str,
    current_status_code: &str,
    transition_kind: &str,
    triggered_by_kind: &str,
    to_stage: &str,
    status_code: &str,
    review_case_id: Option<&Uuid>,
) -> Result<(), RoomProgressionError> {
    if matches!(current_status_code, "blocked" | "withdrawn") {
        return Err(RoomProgressionError::BadRequest(
            "blocked or withdrawn rooms cannot progress".to_owned(),
        ));
    }

    match transition_kind {
        "advance_to_coordination" => {
            require_transition(from_stage, to_stage, "intent", "coordination")
        }
        "advance_to_relationship" => {
            require_transition(from_stage, to_stage, "coordination", "relationship")
        }
        "seal" => {
            if to_stage != "sealed" {
                return Err(RoomProgressionError::BadRequest(
                    "seal transitions must target sealed".to_owned(),
                ));
            }
            let current_review_case_id: Option<Uuid> = track.get("current_review_case_id");
            if let Some(review_case_id) = review_case_id {
                if let Some(current_review_case_id) = current_review_case_id.as_ref() {
                    if current_review_case_id != review_case_id {
                        return Err(RoomProgressionError::BadRequest(
                            "seal review_case_id must match the room's current review case"
                                .to_owned(),
                        ));
                    }
                }
            }
            if from_stage != "sealed" && review_case_id.is_none() {
                return Err(RoomProgressionError::BadRequest(
                    "seal transitions from live rooms require review_case_id".to_owned(),
                ));
            }
            if from_stage != "sealed" {
                let Some(review_case_id) = review_case_id else {
                    return Err(RoomProgressionError::BadRequest(
                        "seal transitions from live rooms require review_case_id".to_owned(),
                    ));
                };
                ensure_review_case_is_active_for_live_seal_tx(client, review_case_id).await?;
            }
            if current_status_code == "sealed_restricted" && status_code != "sealed_restricted" {
                return Err(RoomProgressionError::BadRequest(
                    "restricted sealed rooms cannot downgrade via seal follow-up".to_owned(),
                ));
            }
            if status_code == "sealed_restricted" {
                let Some(review_case_id) = review_case_id else {
                    return Err(RoomProgressionError::BadRequest(
                        "restricted seal transitions require review_case_id".to_owned(),
                    ));
                };
                if !matches!(triggered_by_kind, "operator" | "system") {
                    return Err(RoomProgressionError::BadRequest(
                        "restricted seal transitions must be operator- or system-triggered"
                            .to_owned(),
                    ));
                }
                if let Some(current_review_case_id) = current_review_case_id.as_ref() {
                    if current_review_case_id != review_case_id {
                        return Err(RoomProgressionError::BadRequest(
                            "restricted seal review_case_id must match the room's current review case"
                                .to_owned(),
                        ));
                    }
                }
                ensure_review_supports_restriction_tx(client, review_case_id).await?;
            }
            Ok(())
        }
        "restore" => {
            if from_stage != "sealed" {
                return Err(RoomProgressionError::BadRequest(
                    "restore transitions must move sealed rooms back to their prior live stage"
                        .to_owned(),
                ));
            }
            let Some(review_case_id) = review_case_id else {
                return Err(RoomProgressionError::BadRequest(
                    "restore transitions require review_case_id".to_owned(),
                ));
            };
            let current_review_case_id: Option<Uuid> = track.get("current_review_case_id");
            if current_review_case_id.as_ref() != Some(review_case_id) {
                return Err(RoomProgressionError::BadRequest(
                    "restore review_case_id must match the room's current review case".to_owned(),
                ));
            }
            let restore_target_stage =
                current_restore_target_stage_tx(client, track, review_case_id).await?;
            if to_stage != restore_target_stage {
                return Err(RoomProgressionError::BadRequest(format!(
                    "restore transitions must return this sealed room to {restore_target_stage}"
                )));
            }
            ensure_review_supports_restore_tx(client, review_case_id).await
        }
        "mute" | "block" | "withdraw" => {
            if to_stage != from_stage {
                return Err(RoomProgressionError::BadRequest(
                    "mute, block, and withdraw keep the visible room stage unchanged".to_owned(),
                ));
            }
            let participant_a: Uuid = track.get("participant_a_account_id");
            let participant_b: Uuid = track.get("participant_b_account_id");
            if participant_a == participant_b {
                return Err(RoomProgressionError::Internal(
                    "room participant invariant was violated".to_owned(),
                ));
            }
            Ok(())
        }
        _ => Err(RoomProgressionError::BadRequest(
            "transition_kind is not supported".to_owned(),
        )),
    }
}

fn require_transition(
    from_stage: &str,
    to_stage: &str,
    expected_from: &str,
    expected_to: &str,
) -> Result<(), RoomProgressionError> {
    if from_stage == expected_from && to_stage == expected_to {
        Ok(())
    } else {
        Err(RoomProgressionError::BadRequest(format!(
            "invalid room transition: expected {expected_from} -> {expected_to}"
        )))
    }
}

fn next_status_code(
    transition_kind: &str,
    from_stage: &str,
    to_stage: &str,
    user_facing_reason_code: &str,
) -> Result<String, RoomProgressionError> {
    let status = match transition_kind {
        "advance_to_coordination" => "coordination_open",
        "advance_to_relationship" => "relationship_open",
        "seal" => {
            if user_facing_reason_code == "restricted_after_review" {
                "sealed_restricted"
            } else {
                "sealed_under_review"
            }
        }
        "restore" => match to_stage {
            "intent" => "intent_open",
            "coordination" => "coordination_open",
            "relationship" => "relationship_open",
            _ => {
                return Err(RoomProgressionError::BadRequest(
                    "restore target is not supported".to_owned(),
                ));
            }
        },
        "mute" => "muted",
        "block" => "blocked",
        "withdraw" => "withdrawn",
        _ => {
            return Err(RoomProgressionError::BadRequest(
                "transition_kind is not supported".to_owned(),
            ));
        }
    };
    if matches!(transition_kind, "mute" | "block" | "withdraw") && from_stage != to_stage {
        return Err(RoomProgressionError::BadRequest(
            "local safety controls must preserve visible_stage".to_owned(),
        ));
    }
    Ok(status.to_owned())
}

async fn ensure_review_case_matches_room_scope_tx<C: GenericClient + Sync>(
    client: &C,
    track: &Row,
    review_case_id: &Uuid,
) -> Result<(), RoomProgressionError> {
    let room_progression_id = track.get::<_, Uuid>("room_progression_id").to_string();
    let realm_id = track.get::<_, String>("realm_id");
    let matches_scope = client
        .query_opt(
            "
            SELECT 1
            FROM dao.review_cases
            WHERE review_case_id = $1
              AND case_type = 'sealed_room_fallback'
              AND source_fact_kind = 'room_progression'
              AND source_fact_id = $2
              AND (related_realm_id IS NULL OR related_realm_id = $3)
            ",
            &[review_case_id, &room_progression_id, &realm_id],
        )
        .await
        .map_err(db_error)?
        .is_some();
    if matches_scope {
        Ok(())
    } else {
        Err(RoomProgressionError::BadRequest(
            "review_case_id must reference the room's sealed fallback review case".to_owned(),
        ))
    }
}

async fn ensure_review_case_is_active_for_live_seal_tx<C: GenericClient + Sync>(
    client: &C,
    review_case_id: &Uuid,
) -> Result<(), RoomProgressionError> {
    let is_active = client
        .query_opt(
            "
            SELECT 1
            FROM dao.review_cases
            WHERE review_case_id = $1
              AND review_status IN ('open', 'triaged', 'under_review', 'awaiting_evidence')
            ",
            &[review_case_id],
        )
        .await
        .map_err(db_error)?
        .is_some();
    if is_active {
        Ok(())
    } else {
        Err(RoomProgressionError::BadRequest(
            "live-room seal transitions require an active room-scoped review case".to_owned(),
        ))
    }
}

async fn current_restore_target_stage_tx<C: GenericClient + Sync>(
    client: &C,
    track: &Row,
    review_case_id: &Uuid,
) -> Result<String, RoomProgressionError> {
    let room_progression_id = track.get::<_, Uuid>("room_progression_id");
    let row = client
        .query_opt(
            "
            SELECT from_stage
            FROM dao.room_progression_facts
            WHERE room_progression_id = $1
              AND transition_kind = 'seal'
              AND to_stage = 'sealed'
              AND from_stage <> 'sealed'
              AND review_case_id = $2
            ORDER BY recorded_at DESC, room_progression_fact_id DESC
            LIMIT 1
            ",
            &[&room_progression_id, review_case_id],
        )
        .await
        .map_err(db_error)?
        .ok_or_else(|| {
            RoomProgressionError::BadRequest(
                "restore review_case_id must reference the room's active live-room seal".to_owned(),
            )
        })?;
    Ok(row.get("from_stage"))
}

async fn ensure_review_supports_restore_tx<C: GenericClient + Sync>(
    client: &C,
    review_case_id: &Uuid,
) -> Result<(), RoomProgressionError> {
    let row = client
        .query_opt(
            "
            SELECT decision_kind
            FROM dao.operator_decision_facts
            WHERE review_case_id = $1
            ORDER BY recorded_at DESC, operator_decision_fact_id DESC
            LIMIT 1
            ",
            &[review_case_id],
        )
        .await
        .map_err(db_error)?
        .ok_or_else(|| {
            RoomProgressionError::BadRequest(
                "restore requires a writer-owned operator decision fact".to_owned(),
            )
        })?;
    let decision_kind: String = row.get("decision_kind");
    if matches!(decision_kind.as_str(), "restore" | "no_action") {
        Ok(())
    } else {
        Err(RoomProgressionError::BadRequest(
            "latest operator decision fact does not allow room restore".to_owned(),
        ))
    }
}

async fn ensure_review_supports_restriction_tx<C: GenericClient + Sync>(
    client: &C,
    review_case_id: &Uuid,
) -> Result<(), RoomProgressionError> {
    let row = client
        .query_opt(
            "
            SELECT decision_kind
            FROM dao.operator_decision_facts
            WHERE review_case_id = $1
            ORDER BY recorded_at DESC, operator_decision_fact_id DESC
            LIMIT 1
            ",
            &[review_case_id],
        )
        .await
        .map_err(db_error)?
        .ok_or_else(|| {
            RoomProgressionError::BadRequest(
                "restricted seal requires a writer-owned operator decision fact".to_owned(),
            )
        })?;
    let decision_kind: String = row.get("decision_kind");
    if decision_kind == "restrict" {
        Ok(())
    } else {
        Err(RoomProgressionError::BadRequest(
            "latest operator decision fact does not allow restricted seal".to_owned(),
        ))
    }
}

async fn ensure_account_exists_tx<C: GenericClient + Sync>(
    client: &C,
    account_id: &Uuid,
) -> Result<(), RoomProgressionError> {
    let exists = client
        .query_opt(
            "
            SELECT 1
            FROM core.accounts
            WHERE account_id = $1
              AND account_state = 'active'
            ",
            &[account_id],
        )
        .await
        .map_err(db_error)?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(RoomProgressionError::BadRequest(
            "account id must reference an active account".to_owned(),
        ))
    }
}

async fn ensure_operator_transition_role_tx<C: GenericClient + Sync>(
    client: &C,
    operator_id: &Uuid,
) -> Result<(), RoomProgressionError> {
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
            &[operator_id, &OPERATOR_TRANSITION_ROLES],
        )
        .await
        .map_err(db_error)?;
    if row.get("has_role") {
        Ok(())
    } else {
        Err(RoomProgressionError::Unauthorized(
            "operator role is not allowed for room progression transitions".to_owned(),
        ))
    }
}

fn create_room_progression_payload_hash(
    input: &CreateRoomProgressionInput,
    realm_id: &str,
    participants: &(Uuid, Uuid),
    related_promise_intent_id: &Option<Uuid>,
    related_settlement_case_id: &Option<Uuid>,
) -> String {
    hash_json_value(&json!({
        "schema_version": 1,
        "realm_id": realm_id,
        "participant_account_ids": [
            participants.0.to_string(),
            participants.1.to_string()
        ],
        "related_promise_intent_id": optional_uuid_hash_value(related_promise_intent_id),
        "related_settlement_case_id": optional_uuid_hash_value(related_settlement_case_id),
        "user_facing_reason_code": &input.user_facing_reason_code,
        "source_fact_kind": &input.source_fact_kind,
        "source_fact_id": &input.source_fact_id,
        "source_snapshot_json": &input.source_snapshot_json,
    }))
}

fn room_progression_fact_payload_hash_from_input(
    existing: &Row,
    input: &AppendRoomProgressionFactInput,
) -> Result<String, RoomProgressionError> {
    let triggered_by_account_id =
        parse_optional_uuid(&input.triggered_by_account_id, "triggered by account id")?;
    let review_case_id = parse_optional_uuid(&input.review_case_id, "review case id")?;
    Ok(create_room_progression_fact_payload_hash(
        &input.transition_kind,
        &existing.get::<_, String>("from_stage"),
        &input.to_stage,
        &existing.get::<_, String>("status_code"),
        &input.user_facing_reason_code,
        &input.triggered_by_kind,
        &triggered_by_account_id,
        &input.source_fact_kind,
        &input.source_fact_id,
        &input.source_snapshot_json,
        &review_case_id,
    ))
}

fn create_room_progression_fact_payload_hash(
    transition_kind: &str,
    from_stage: &str,
    to_stage: &str,
    status_code: &str,
    user_facing_reason_code: &str,
    triggered_by_kind: &str,
    triggered_by_account_id: &Option<Uuid>,
    source_fact_kind: &str,
    source_fact_id: &str,
    source_snapshot_json: &Value,
    review_case_id: &Option<Uuid>,
) -> String {
    hash_json_value(&json!({
        "schema_version": 1,
        "transition_kind": transition_kind,
        "from_stage": from_stage,
        "to_stage": to_stage,
        "status_code": status_code,
        "user_facing_reason_code": user_facing_reason_code,
        "triggered_by_kind": triggered_by_kind,
        "triggered_by_account_id": optional_uuid_hash_value(triggered_by_account_id),
        "source_fact_kind": source_fact_kind,
        "source_fact_id": source_fact_id,
        "source_snapshot_json": source_snapshot_json,
        "review_case_id": optional_uuid_hash_value(review_case_id),
    }))
}

fn ensure_track_matches_payload_hash(
    row: &Row,
    request_payload_hash: &str,
) -> Result<(), RoomProgressionError> {
    let existing_hash: String = row.get("request_payload_hash");
    if existing_hash == request_payload_hash {
        Ok(())
    } else {
        Err(RoomProgressionError::BadRequest(
            "request_idempotency_key was already used with a different room progression payload"
                .to_owned(),
        ))
    }
}

fn ensure_fact_matches_payload_hash(
    row: &Row,
    fact_payload_hash: &str,
) -> Result<(), RoomProgressionError> {
    let existing_hash: String = row.get("fact_payload_hash");
    if existing_hash == fact_payload_hash {
        Ok(())
    } else {
        Err(RoomProgressionError::BadRequest(
            "fact_idempotency_key was already used with a different room progression fact payload"
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

fn parse_participant_pair(values: &[String]) -> Result<(Uuid, Uuid), RoomProgressionError> {
    if values.len() != 2 {
        return Err(RoomProgressionError::BadRequest(
            "participant_account_ids must contain exactly two account ids".to_owned(),
        ));
    }
    let mut parsed = vec![
        parse_uuid(&values[0], "participant account id")?,
        parse_uuid(&values[1], "participant account id")?,
    ];
    parsed.sort();
    parsed.dedup();
    if parsed.len() != 2 {
        return Err(RoomProgressionError::BadRequest(
            "room participants must be distinct".to_owned(),
        ));
    }
    Ok((parsed[0], parsed[1]))
}

fn parse_uuid(value: &str, label: &str) -> Result<Uuid, RoomProgressionError> {
    Uuid::parse_str(value.trim())
        .map_err(|_| RoomProgressionError::BadRequest(format!("{label} must be a valid UUID")))
}

fn parse_optional_uuid(
    value: &Option<String>,
    label: &str,
) -> Result<Option<Uuid>, RoomProgressionError> {
    value
        .as_ref()
        .map(|value| parse_uuid(value, label))
        .transpose()
}

fn optional_uuid_hash_value(value: &Option<Uuid>) -> Option<String> {
    value.map(|value| value.to_string())
}

fn optional_uuid_to_string(value: Option<Uuid>) -> Option<String> {
    value.map(|value| value.to_string())
}

fn normalize_required(value: &str, label: &str) -> Result<String, RoomProgressionError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        Err(RoomProgressionError::BadRequest(format!(
            "{label} must not be empty"
        )))
    } else {
        Ok(normalized.to_owned())
    }
}

fn normalize_optional(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn require_non_empty(label: &str, value: &str) -> Result<(), RoomProgressionError> {
    if value.trim().is_empty() {
        Err(RoomProgressionError::BadRequest(format!(
            "{label} must not be empty"
        )))
    } else {
        Ok(())
    }
}

fn validate_allowed(
    label: &str,
    value: &str,
    allowed: &[&str],
) -> Result<(), RoomProgressionError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(RoomProgressionError::BadRequest(format!(
            "{label} is not supported"
        )))
    }
}

fn room_progression_track_from_row(row: &Row) -> RoomProgressionTrackSnapshot {
    RoomProgressionTrackSnapshot {
        room_progression_id: row.get::<_, Uuid>("room_progression_id").to_string(),
        realm_id: row.get("realm_id"),
        participant_a_account_id: row.get::<_, Uuid>("participant_a_account_id").to_string(),
        participant_b_account_id: row.get::<_, Uuid>("participant_b_account_id").to_string(),
        related_promise_intent_id: optional_uuid_to_string(row.get("related_promise_intent_id")),
        related_settlement_case_id: optional_uuid_to_string(row.get("related_settlement_case_id")),
        current_stage: row.get("current_stage"),
        current_status_code: row.get("current_status_code"),
        current_user_facing_reason_code: row.get("current_user_facing_reason_code"),
        current_review_case_id: optional_uuid_to_string(row.get("current_review_case_id")),
        source_fact_kind: row.get("source_fact_kind"),
        source_fact_id: row.get("source_fact_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn room_progression_fact_from_row(row: &Row) -> RoomProgressionFactSnapshot {
    RoomProgressionFactSnapshot {
        room_progression_fact_id: row.get::<_, Uuid>("room_progression_fact_id").to_string(),
        room_progression_id: row.get::<_, Uuid>("room_progression_id").to_string(),
        from_stage: row.get("from_stage"),
        to_stage: row.get("to_stage"),
        transition_kind: row.get("transition_kind"),
        status_code: row.get("status_code"),
        user_facing_reason_code: row.get("user_facing_reason_code"),
        triggered_by_kind: row.get("triggered_by_kind"),
        triggered_by_account_id: optional_uuid_to_string(row.get("triggered_by_account_id")),
        source_fact_kind: row.get("source_fact_kind"),
        source_fact_id: row.get("source_fact_id"),
        review_case_id: optional_uuid_to_string(row.get("review_case_id")),
        recorded_at: row.get("recorded_at"),
    }
}

fn room_progression_view_from_row(row: &Row) -> RoomProgressionViewSnapshot {
    RoomProgressionViewSnapshot {
        room_progression_id: row.get::<_, Uuid>("room_progression_id").to_string(),
        realm_id: row.get("realm_id"),
        participant_a_account_id: row.get::<_, Uuid>("participant_a_account_id").to_string(),
        participant_b_account_id: row.get::<_, Uuid>("participant_b_account_id").to_string(),
        visible_stage: row.get("visible_stage"),
        status_code: row.get("status_code"),
        user_facing_reason_code: row.get("user_facing_reason_code"),
        review_case_id: optional_uuid_to_string(row.get("review_case_id")),
        review_pending: row.get("review_pending"),
        review_status: row.get("review_status"),
        appeal_available: row.get("appeal_available"),
        evidence_requested: row.get("evidence_requested"),
        source_watermark_at: row.get("source_watermark_at"),
        source_fact_count: row.get("source_fact_count"),
        projection_lag_ms: row.get("projection_lag_ms"),
        rebuild_generation: row.get("rebuild_generation"),
        last_projected_at: row.get("last_projected_at"),
    }
}

fn db_error(error: tokio_postgres::Error) -> RoomProgressionError {
    let code = error.code().map(|code| code.code().to_owned());
    let constraint = error
        .as_db_error()
        .and_then(|db_error| db_error.constraint().map(str::to_owned));
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
    RoomProgressionError::Database {
        message: error.to_string(),
        code,
        constraint,
        retryable,
    }
}
