use std::{cmp::Ordering, fmt::Write as _, sync::Arc};

use chrono::{DateTime, Utc};
use musubi_db_runtime::{DbConfig, connect_writer};
use musubi_settlement_domain::{
    BackendKey, BackendPin, BackendVersion, CurrencyCode, Money, NormalizedObservation,
    NormalizedObservationKind, ObservationConfidence, ReceiptVerification, SubmissionResult,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_postgres::{Client, Row, error::SqlState};
use uuid::Uuid;

use super::{
    backend::{callback_dedupe_key, pi_backend_descriptor},
    common::canonical_pi_money,
    constants::{
        COMMAND_INBOX_RETENTION_MINUTES, EVENT_INGEST_PROVIDER_CALLBACK, EVENT_OPEN_HOLD_INTENT,
        EVENT_REFRESH_PROMISE_VIEW, EVENT_REFRESH_SETTLEMENT_VIEW,
        LEDGER_ACCOUNT_PROVIDER_CLEARING_INBOUND, LEDGER_ACCOUNT_USER_SECURED_FUNDS_LIABILITY,
        LEDGER_DIRECTION_CREDIT, LEDGER_DIRECTION_DEBIT, OUTBOX_MANUAL_REVIEW, OUTBOX_PENDING,
        OUTBOX_PROCESSING, OUTBOX_PUBLISHED, OUTBOX_QUARANTINED, OUTBOX_RETRY_BACKOFF_MILLIS,
        PI_CURRENCY_CODE, PROJECTION_BUILDER, PROMISE_INTENT_PROPOSED, PROVIDER_CALLBACK_CONSUMER,
        PROVIDER_KEY, RECEIPT_STATUS_MANUAL_REVIEW, RECEIPT_STATUS_REJECTED,
        RECEIPT_STATUS_VERIFIED, SETTLEMENT_CASE_FUNDED, SETTLEMENT_CASE_PENDING_FUNDING,
        SETTLEMENT_ORCHESTRATOR,
    },
    state::{
        OutboxCommand, OutboxMessageRecord, ProviderAttemptRecord, RawProviderCallbackRecord,
        SettlementCaseRecord,
    },
    types::{
        AuthenticatedAccount, AuthenticationInput, CallbackContext, ExpandedSettlementViewSnapshot,
        HappyRouteError, OpenHoldIntentPersistResult, OpenHoldIntentPrepareOutcome,
        ParsedPaymentCallback, PaymentCallbackInput, PaymentCallbackOutcome, ProjectionProvenance,
        ProjectionRebuildItem, ProjectionRebuildOutcome, PromiseIntentInput, PromiseIntentOutcome,
        PromiseProjectionSnapshot, RawPaymentCallbackFields, SettlementViewSnapshot,
        SubmissionPreparation, TrustSnapshot, processed_outbox_message,
    },
};

#[derive(Clone)]
pub struct HappyRouteStore {
    client: Arc<Mutex<Client>>,
}

impl HappyRouteStore {
    pub(crate) async fn connect(config: &DbConfig) -> musubi_db_runtime::Result<Self> {
        let client = connect_writer(config, "musubi-backend happy-route").await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }

    pub(crate) async fn reset_for_test(&self) -> Result<(), HappyRouteError> {
        let client = self.client.lock().await;
        client
            .batch_execute(
                "
                TRUNCATE
                    projection.projection_meta,
                    projection.realm_trust_snapshots,
                    projection.trust_snapshots,
                    projection.settlement_views,
                    projection.promise_views,
                    ledger.account_postings,
                    ledger.journal_entries,
                    core.payment_receipts,
                    dao.settlement_observations,
                    dao.provider_attempts,
                    dao.settlement_submissions,
                    dao.settlement_intents,
                    outbox.command_inbox,
                    outbox.events,
                    core.raw_provider_callbacks,
                    core.raw_provider_callback_dedupe,
                    dao.promise_intent_idempotency_keys,
                    dao.settlement_cases,
                    dao.promise_intents,
                    core.auth_sessions,
                    core.pi_account_links,
                    core.person_profiles,
                    core.accounts
                RESTART IDENTITY CASCADE
                ",
            )
            .await
            .map_err(db_error)?;
        Ok(())
    }

    pub(super) async fn authenticate_pi_account(
        &self,
        input: AuthenticationInput,
    ) -> Result<AuthenticatedAccount, HappyRouteError> {
        let access_token_digest = digest_access_token(&input.access_token);
        let token = format!("pi-session-{}", Uuid::new_v4());
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;

        let existing = tx
            .query_opt(
                "
                SELECT account_id, access_token_digest
                FROM core.pi_account_links
                WHERE pi_uid = $1
                FOR UPDATE
                ",
                &[&input.pi_uid],
            )
            .await
            .map_err(db_error)?;

        let account_id = if let Some(row) = existing {
            let account_id: Uuid = row.get("account_id");
            let stored_digest: String = row.get("access_token_digest");
            if stored_digest != access_token_digest {
                return Err(HappyRouteError::Unauthorized(
                    "pi identity proof did not match the existing account".to_owned(),
                ));
            }
            tx.execute(
                "
                UPDATE core.pi_account_links
                SET username = $2,
                    wallet_address = $3,
                    updated_at = CURRENT_TIMESTAMP
                WHERE pi_uid = $1
                ",
                &[&input.pi_uid, &input.username, &input.wallet_address],
            )
            .await
            .map_err(db_error)?;
            account_id
        } else {
            let new_account_id = Uuid::new_v4();
            tx.execute(
                "
                INSERT INTO core.accounts (account_id, account_class, account_state)
                VALUES ($1, 'Ordinary Account', 'active')
                ",
                &[&new_account_id],
            )
            .await
            .map_err(db_error)?;
            let row = tx
                .query_one(
                    "
                    INSERT INTO core.pi_account_links (
                        account_id,
                        pi_uid,
                        username,
                        wallet_address,
                        access_token_digest
                    )
                    VALUES ($1, $2, $3, $4, $5)
                    ON CONFLICT (pi_uid) DO UPDATE
                    SET username = CASE
                            WHEN core.pi_account_links.access_token_digest = EXCLUDED.access_token_digest
                                THEN EXCLUDED.username
                            ELSE core.pi_account_links.username
                        END,
                        wallet_address = CASE
                            WHEN core.pi_account_links.access_token_digest = EXCLUDED.access_token_digest
                                THEN EXCLUDED.wallet_address
                            ELSE core.pi_account_links.wallet_address
                        END,
                        updated_at = CASE
                            WHEN core.pi_account_links.access_token_digest = EXCLUDED.access_token_digest
                                THEN CURRENT_TIMESTAMP
                            ELSE core.pi_account_links.updated_at
                        END
                    RETURNING account_id, access_token_digest
                    ",
                    &[
                        &new_account_id,
                        &input.pi_uid,
                        &input.username,
                        &input.wallet_address,
                        &access_token_digest,
                    ],
                )
                .await
                .map_err(db_error)?;
            let account_id: Uuid = row.get("account_id");
            let stored_digest: String = row.get("access_token_digest");
            if account_id != new_account_id {
                tx.execute(
                    "DELETE FROM core.accounts WHERE account_id = $1",
                    &[&new_account_id],
                )
                .await
                .map_err(db_error)?;
                if stored_digest != access_token_digest {
                    return Err(HappyRouteError::Unauthorized(
                        "pi identity proof did not match the existing account".to_owned(),
                    ));
                }
            }
            account_id
        };

        tx.execute(
            "DELETE FROM core.auth_sessions WHERE account_id = $1",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
        tx.execute(
            "
            INSERT INTO core.auth_sessions (session_token, account_id)
            VALUES ($1, $2)
            ",
            &[&token, &account_id],
        )
        .await
        .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;

        Ok(AuthenticatedAccount {
            token,
            account_id: account_id.to_string(),
            pi_uid: input.pi_uid,
            username: input.username,
        })
    }

    pub(super) async fn authorize_account(
        &self,
        token: &str,
    ) -> Result<AuthenticatedAccount, HappyRouteError> {
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "
                SELECT
                    auth.session_token,
                    auth.account_id,
                    link.pi_uid,
                    link.username
                FROM core.auth_sessions auth
                JOIN core.pi_account_links link ON link.account_id = auth.account_id
                JOIN core.accounts account ON account.account_id = auth.account_id
                WHERE auth.session_token = $1
                  AND account.account_state = 'active'
                ",
                &[&token],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(|| {
                HappyRouteError::Unauthorized("valid bearer token is required".to_owned())
            })?;

        let account_id: Uuid = row.get("account_id");
        Ok(AuthenticatedAccount {
            token: row.get("session_token"),
            account_id: account_id.to_string(),
            pi_uid: row.get("pi_uid"),
            username: row.get("username"),
        })
    }

    pub(super) async fn create_promise_intent(
        &self,
        initiator_account_id: &str,
        input: PromiseIntentInput,
    ) -> Result<PromiseIntentOutcome, HappyRouteError> {
        let initiator_account_id = parse_uuid(initiator_account_id, "initiator account id")?;
        let counterparty_account_id =
            parse_uuid(&input.counterparty_account_id, "counterparty account id")?;
        if initiator_account_id == counterparty_account_id {
            return Err(HappyRouteError::BadRequest(
                "initiator_account_id and counterparty_account_id must differ".to_owned(),
            ));
        }
        let deposit_amount =
            canonical_pi_money(input.deposit_amount_minor_units, &input.currency_code)?;
        let deposit_minor_units = to_i64(deposit_amount.minor_units(), "deposit amount")?;
        let request_hash = promise_request_hash(
            &input.realm_id,
            &counterparty_account_id.to_string(),
            &deposit_amount,
        );
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;

        ensure_account_exists(
            &tx,
            &initiator_account_id,
            "initiator account was not found",
        )
        .await?;
        ensure_account_exists(
            &tx,
            &counterparty_account_id,
            "counterparty account was not found",
        )
        .await?;

        if let Some(row) = tx
            .query_opt(
                "
                SELECT
                    idem.promise_intent_id,
                    idem.request_payload_hash,
                    settlement.settlement_case_id,
                    settlement.case_status
                FROM dao.promise_intent_idempotency_keys idem
                JOIN dao.settlement_cases settlement
                    ON settlement.promise_intent_id = idem.promise_intent_id
                WHERE idem.initiator_account_id = $1
                  AND idem.internal_idempotency_key = $2
                FOR UPDATE
                ",
                &[&initiator_account_id, &input.internal_idempotency_key],
            )
            .await
            .map_err(db_error)?
        {
            let existing_hash: String = row.get("request_payload_hash");
            if existing_hash != request_hash {
                return Err(HappyRouteError::BadRequest(
                    "internal_idempotency_key was already used with a different Promise payload"
                        .to_owned(),
                ));
            }
            let promise_intent_id: Uuid = row.get("promise_intent_id");
            let settlement_case_id: Uuid = row.get("settlement_case_id");
            let case_status: String = row.get("case_status");
            tx.commit().await.map_err(db_error)?;
            return Ok(PromiseIntentOutcome {
                promise_intent_id: promise_intent_id.to_string(),
                settlement_case_id: settlement_case_id.to_string(),
                case_status,
                outbox_event_ids: Vec::new(),
                replayed_intent: true,
            });
        }

        let promise_intent_id = Uuid::new_v4();
        let settlement_case_id = Uuid::new_v4();
        let backend_pin = pi_backend_descriptor().pin();

        tx.execute(
            "
            INSERT INTO dao.promise_intents (
                promise_intent_id,
                realm_id,
                initiator_account_id,
                counterparty_account_id,
                intent_status,
                deposit_amount_minor_units,
                deposit_currency_code,
                deposit_scale
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ",
            &[
                &promise_intent_id,
                &input.realm_id,
                &initiator_account_id,
                &counterparty_account_id,
                &PROMISE_INTENT_PROPOSED,
                &deposit_minor_units,
                &deposit_amount.currency().as_str(),
                &(deposit_amount.scale() as i32),
            ],
        )
        .await
        .map_err(db_error)?;
        tx.execute(
            "
            INSERT INTO dao.settlement_cases (
                settlement_case_id,
                promise_intent_id,
                realm_id,
                case_status,
                backend_key,
                backend_version
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ",
            &[
                &settlement_case_id,
                &promise_intent_id,
                &input.realm_id,
                &SETTLEMENT_CASE_PENDING_FUNDING,
                &backend_pin.backend_key.as_str(),
                &backend_pin.backend_version.as_str(),
            ],
        )
        .await
        .map_err(db_error)?;
        if tx
            .execute(
                "
                INSERT INTO dao.promise_intent_idempotency_keys (
                    initiator_account_id,
                    internal_idempotency_key,
                    promise_intent_id,
                    request_payload_hash
                )
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (initiator_account_id, internal_idempotency_key) DO NOTHING
                ",
                &[
                    &initiator_account_id,
                    &input.internal_idempotency_key,
                    &promise_intent_id,
                    &request_hash,
                ],
            )
            .await
            .map_err(db_error)?
            == 0
        {
            tx.execute(
                "DELETE FROM dao.settlement_cases WHERE settlement_case_id = $1",
                &[&settlement_case_id],
            )
            .await
            .map_err(db_error)?;
            tx.execute(
                "DELETE FROM dao.promise_intents WHERE promise_intent_id = $1",
                &[&promise_intent_id],
            )
            .await
            .map_err(db_error)?;

            let row = tx
                .query_one(
                    "
                    SELECT
                        idem.promise_intent_id,
                        idem.request_payload_hash,
                        settlement.settlement_case_id,
                        settlement.case_status
                    FROM dao.promise_intent_idempotency_keys idem
                    JOIN dao.settlement_cases settlement
                        ON settlement.promise_intent_id = idem.promise_intent_id
                    WHERE idem.initiator_account_id = $1
                      AND idem.internal_idempotency_key = $2
                    FOR UPDATE
                    ",
                    &[&initiator_account_id, &input.internal_idempotency_key],
                )
                .await
                .map_err(db_error)?;
            let existing_hash: String = row.get("request_payload_hash");
            if existing_hash != request_hash {
                return Err(HappyRouteError::BadRequest(
                    "internal_idempotency_key was already used with a different Promise payload"
                        .to_owned(),
                ));
            }
            let promise_intent_id: Uuid = row.get("promise_intent_id");
            let settlement_case_id: Uuid = row.get("settlement_case_id");
            let case_status: String = row.get("case_status");
            tx.commit().await.map_err(db_error)?;
            return Ok(PromiseIntentOutcome {
                promise_intent_id: promise_intent_id.to_string(),
                settlement_case_id: settlement_case_id.to_string(),
                case_status,
                outbox_event_ids: Vec::new(),
                replayed_intent: true,
            });
        }
        let hold_event_id = insert_outbox_message_tx(
            &tx,
            "settlement_case",
            settlement_case_id,
            EVENT_OPEN_HOLD_INTENT,
            &OutboxCommand::OpenHoldIntent {
                settlement_case_id: settlement_case_id.to_string(),
            },
        )
        .await?;
        let promise_event_id = insert_outbox_message_tx(
            &tx,
            "promise_intent",
            promise_intent_id,
            EVENT_REFRESH_PROMISE_VIEW,
            &OutboxCommand::RefreshPromiseView {
                promise_intent_id: promise_intent_id.to_string(),
            },
        )
        .await?;
        tx.commit().await.map_err(db_error)?;

        Ok(PromiseIntentOutcome {
            promise_intent_id: promise_intent_id.to_string(),
            settlement_case_id: settlement_case_id.to_string(),
            case_status: SETTLEMENT_CASE_PENDING_FUNDING.to_owned(),
            outbox_event_ids: vec![hold_event_id.to_string(), promise_event_id.to_string()],
            replayed_intent: false,
        })
    }

    pub(super) async fn accept_payment_callback(
        &self,
        input: &PaymentCallbackInput,
        raw_fields: Option<&RawPaymentCallbackFields>,
        raw_callback_id: Uuid,
        received_at: DateTime<Utc>,
    ) -> Result<(bool, Uuid), HappyRouteError> {
        let dedupe_key = callback_dedupe_key(&input.raw_body_bytes);
        let raw_body = std::str::from_utf8(&input.raw_body_bytes)
            .map(str::to_owned)
            .unwrap_or_else(|_| "[non-utf8 body; see raw_body_bytes]".to_owned());
        let headers = redacted_headers_json(&input.redacted_headers);
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;

        let inserted = tx
            .execute(
                "
                INSERT INTO core.raw_provider_callback_dedupe (
                    provider_name,
                    dedupe_key,
                    first_raw_callback_id
                )
                VALUES ($1, $2, $3)
                ON CONFLICT DO NOTHING
                ",
                &[&PROVIDER_KEY, &dedupe_key, &raw_callback_id],
            )
            .await
            .map_err(db_error)?;
        let replay_of_raw_callback_id = if inserted == 0 {
            let row = tx
                .query_one(
                    "
                    SELECT first_raw_callback_id
                    FROM core.raw_provider_callback_dedupe
                    WHERE provider_name = $1
                      AND dedupe_key = $2
                    ",
                    &[&PROVIDER_KEY, &dedupe_key],
                )
                .await
                .map_err(db_error)?;
            Some(row.get::<_, Uuid>("first_raw_callback_id"))
        } else {
            None
        };

        let amount_minor_units = raw_fields
            .and_then(|fields| fields.amount_minor_units)
            .and_then(|value| i64::try_from(value).ok());
        tx.execute(
            "
            INSERT INTO core.raw_provider_callbacks (
                raw_callback_id,
                provider_name,
                dedupe_key,
                replay_of_raw_callback_id,
                raw_body_bytes,
                raw_body,
                redacted_headers,
                signature_valid,
                provider_submission_id,
                provider_ref,
                payer_pi_uid,
                amount_minor_units,
                currency_code,
                amount_scale,
                txid,
                callback_status,
                received_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, $8, NULL, $9, $10, $11, NULL, $12, $13, $14)
            ",
            &[
                &raw_callback_id,
                &PROVIDER_KEY,
                &dedupe_key,
                &replay_of_raw_callback_id,
                &input.raw_body_bytes,
                &raw_body,
                &headers,
                &raw_fields.and_then(|fields| fields.provider_submission_id.clone()),
                &raw_fields.and_then(|fields| fields.payer_pi_uid.clone()),
                &amount_minor_units,
                &raw_fields.and_then(|fields| fields.currency_code.clone()),
                &raw_fields.and_then(|fields| fields.txid.clone()),
                &raw_fields.and_then(|fields| fields.callback_status.clone()),
                &received_at,
            ],
        )
        .await
        .map_err(db_error)?;

        let outbox_event_id = insert_outbox_message_tx(
            &tx,
            "provider_callback",
            raw_callback_id,
            EVENT_INGEST_PROVIDER_CALLBACK,
            &OutboxCommand::IngestProviderCallback {
                raw_callback_id: raw_callback_id.to_string(),
            },
        )
        .await?;
        tx.commit().await.map_err(db_error)?;
        Ok((replay_of_raw_callback_id.is_some(), outbox_event_id))
    }

    pub(super) async fn claim_pending_outbox_message(
        &self,
    ) -> Result<Option<OutboxMessageRecord>, HappyRouteError> {
        let mut client = self.client.lock().await;
        loop {
            let tx = client.transaction().await.map_err(db_error)?;
            let row = tx
                .query_opt(
                    "
                    WITH claimed AS (
                        SELECT event_id
                        FROM outbox.events
                        WHERE (
                                delivery_status = $1
                                OR (
                                    delivery_status = $2
                                    AND COALESCE(
                                        claimed_until,
                                        last_attempt_at + interval '5 minutes'
                                    ) < CURRENT_TIMESTAMP
                                )
                            )
                          AND available_at <= CURRENT_TIMESTAMP
                        ORDER BY causal_order
                        LIMIT 1
                        FOR UPDATE SKIP LOCKED
                    )
                    UPDATE outbox.events events
                    SET delivery_status = $2,
                        claimed_by = $3,
                        claimed_until = CURRENT_TIMESTAMP + interval '5 minutes',
                        attempt_count = events.attempt_count + 1,
                        last_attempt_at = CURRENT_TIMESTAMP
                    FROM claimed
                    WHERE events.event_id = claimed.event_id
                    RETURNING
                        events.event_id,
                        events.idempotency_key,
                        events.aggregate_type,
                        events.aggregate_id,
                        events.event_type,
                        events.schema_version,
                        events.payload_json,
                        events.delivery_status,
                        events.attempt_count,
                        events.last_error_class,
                        events.last_error_detail,
                        events.available_at,
                        events.published_at,
                        events.created_at
                    ",
                    &[&OUTBOX_PENDING, &OUTBOX_PROCESSING, &"happy-route-drain"],
                )
                .await
                .map_err(db_error)?;
            let Some(row) = row else {
                tx.commit().await.map_err(db_error)?;
                return Ok(None);
            };

            match outbox_message_from_row(&row) {
                Ok(message) => {
                    tx.commit().await.map_err(db_error)?;
                    return Ok(Some(message));
                }
                Err(error) => {
                    let event_id: Uuid = row.get("event_id");
                    tx.execute(
                        "
                        UPDATE outbox.events
                        SET delivery_status = $2,
                            claimed_by = NULL,
                            claimed_until = NULL,
                            last_error_class = $3,
                            last_error_detail = $4,
                            retain_until = CURRENT_TIMESTAMP + ($5::bigint * interval '1 minute')
                        WHERE event_id = $1
                        ",
                        &[
                            &event_id,
                            &OUTBOX_QUARANTINED,
                            &"permanent",
                            &error.message(),
                            &COMMAND_INBOX_RETENTION_MINUTES,
                        ],
                    )
                    .await
                    .map_err(db_error)?;
                    tx.commit().await.map_err(db_error)?;
                }
            }
        }
    }

    pub(super) async fn finalize_provider_callback_replay(
        &self,
        message: &OutboxMessageRecord,
        outcome: &PaymentCallbackOutcome,
    ) -> Result<(), HappyRouteError> {
        let event_id = parse_uuid(&message.event_id, "outbox event id")?;
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        if begin_command_tx(&tx, PROVIDER_CALLBACK_CONSUMER, message).await?
            == CommandBegin::Started
        {
            complete_command_tx(
                &tx,
                PROVIDER_CALLBACK_CONSUMER,
                &event_id,
                Some(json!({
                    "payment_receipt_id": outcome.payment_receipt_id,
                    "duplicate_receipt": outcome.duplicate_receipt
                })),
            )
            .await?;
        }
        mark_outbox_published_tx(&tx, &event_id).await?;
        tx.commit().await.map_err(db_error)?;
        Ok(())
    }

    pub(super) async fn record_outbox_failure(
        &self,
        message: &OutboxMessageRecord,
        error: &HappyRouteError,
    ) -> Result<(), HappyRouteError> {
        let event_id = parse_uuid(&message.event_id, "outbox event id")?;
        let consumer_name = command_consumer_name(message);
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        match error.provider_error_class() {
            Some(super::types::ProviderErrorClass::Retryable) => {
                if provider_callback_mapping_retry_window_exhausted(message, error) {
                    mark_outbox_terminal_tx(
                        &tx,
                        &event_id,
                        OUTBOX_MANUAL_REVIEW,
                        "deferred",
                        error.message(),
                    )
                    .await?;
                    mark_command_terminal_tx(
                        &tx,
                        consumer_name,
                        message,
                        "deferred",
                        error.message(),
                    )
                    .await?;
                } else {
                    mark_command_retry_pending_tx(
                        &tx,
                        consumer_name,
                        message,
                        "transient",
                        error.message(),
                    )
                    .await?;
                    tx.execute(
                        "
                        UPDATE outbox.events
                        SET delivery_status = $2,
                            claimed_by = NULL,
                            claimed_until = NULL,
                            last_error_class = $3,
                            last_error_detail = $4,
                            available_at = CURRENT_TIMESTAMP + ($5::bigint * interval '1 millisecond')
                        WHERE event_id = $1
                        ",
                        &[
                            &event_id,
                            &OUTBOX_PENDING,
                            &"transient",
                            &error.message(),
                            &OUTBOX_RETRY_BACKOFF_MILLIS,
                        ],
                    )
                    .await
                    .map_err(db_error)?;
                }
            }
            Some(super::types::ProviderErrorClass::ManualReview) => {
                mark_outbox_terminal_tx(
                    &tx,
                    &event_id,
                    OUTBOX_MANUAL_REVIEW,
                    "deferred",
                    error.message(),
                )
                .await?;
                mark_command_terminal_tx(&tx, consumer_name, message, "deferred", error.message())
                    .await?;
            }
            Some(super::types::ProviderErrorClass::Terminal) | None => {
                mark_outbox_terminal_tx(
                    &tx,
                    &event_id,
                    OUTBOX_QUARANTINED,
                    "permanent",
                    error.message(),
                )
                .await?;
                mark_command_terminal_tx(&tx, consumer_name, message, "permanent", error.message())
                    .await?;
            }
        }
        tx.commit().await.map_err(db_error)?;
        Ok(())
    }

    pub(super) async fn prune_processed_command_inbox(&self) -> Result<(), HappyRouteError> {
        let client = self.client.lock().await;
        client
            .execute(
                "
                DELETE FROM outbox.command_inbox
                WHERE status IN ('completed', 'quarantined')
                  AND retain_until IS NOT NULL
                  AND retain_until < CURRENT_TIMESTAMP
                ",
                &[],
            )
            .await
            .map_err(db_error)?;
        Ok(())
    }

    pub(super) async fn prune_terminal_outbox_events(&self) -> Result<(), HappyRouteError> {
        let client = self.client.lock().await;
        client
            .execute(
                "
                DELETE FROM outbox.events
                WHERE delivery_status IN ($1, $2, $3)
                  AND retain_until IS NOT NULL
                  AND retain_until < CURRENT_TIMESTAMP
                ",
                &[
                    &OUTBOX_PUBLISHED,
                    &OUTBOX_QUARANTINED,
                    &OUTBOX_MANUAL_REVIEW,
                ],
            )
            .await
            .map_err(db_error)?;
        Ok(())
    }

    pub(super) async fn prepare_open_hold_intent(
        &self,
        message: &OutboxMessageRecord,
        settlement_case_id: &str,
    ) -> Result<OpenHoldIntentPrepareOutcome, HappyRouteError> {
        let event_id = parse_uuid(&message.event_id, "outbox event id")?;
        let settlement_case_id = parse_uuid(settlement_case_id, "settlement case id")?;
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        let inbox = begin_command_tx(&tx, SETTLEMENT_ORCHESTRATOR, message).await?;
        if inbox == CommandBegin::Completed {
            mark_outbox_published_tx(&tx, &event_id).await?;
            tx.commit().await.map_err(db_error)?;
            return Ok(OpenHoldIntentPrepareOutcome::ReplayNoop(
                processed_outbox_message(message, SETTLEMENT_ORCHESTRATOR, None, true),
            ));
        }

        let settlement_case = load_settlement_case_tx(&tx, &settlement_case_id).await?;
        let promise_intent_id =
            parse_uuid(&settlement_case.promise_intent_id, "promise intent id")?;
        let promise_intent = load_promise_intent_tx(&tx, &promise_intent_id).await?;
        let internal_idempotency_key = format!("hold-intent:{}", message.idempotency_key);

        if let Some(row) = tx
            .query_opt(
                "
                SELECT
                    intent.settlement_intent_id,
                    submission.settlement_submission_id
                FROM dao.settlement_intents intent
                JOIN dao.settlement_submissions submission
                    ON submission.settlement_intent_id = intent.settlement_intent_id
                WHERE intent.settlement_case_id = $1
                  AND intent.internal_idempotency_key = $2
                ",
                &[&settlement_case_id, &internal_idempotency_key],
            )
            .await
            .map_err(db_error)?
        {
            let settlement_intent_id: Uuid = row.get("settlement_intent_id");
            let settlement_submission_id: Uuid = row.get("settlement_submission_id");
            tx.commit().await.map_err(db_error)?;
            return Ok(OpenHoldIntentPrepareOutcome::Ready(SubmissionPreparation {
                settlement_case,
                promise_intent,
                settlement_intent_id: settlement_intent_id.to_string(),
                settlement_submission_id: settlement_submission_id.to_string(),
                internal_idempotency_key,
            }));
        }

        let settlement_intent_id = Uuid::new_v4();
        let settlement_submission_id = Uuid::new_v4();
        tx.execute(
            "
            INSERT INTO dao.settlement_intents (
                settlement_intent_id,
                settlement_case_id,
                capability,
                internal_idempotency_key
            )
            VALUES ($1, $2, 'HoldValue', $3)
            ",
            &[
                &settlement_intent_id,
                &settlement_case_id,
                &internal_idempotency_key,
            ],
        )
        .await
        .map_err(db_error)?;
        tx.execute(
            "
            INSERT INTO dao.settlement_submissions (
                settlement_submission_id,
                settlement_case_id,
                settlement_intent_id,
                submission_status
            )
            VALUES ($1, $2, $3, 'pending')
            ",
            &[
                &settlement_submission_id,
                &settlement_case_id,
                &settlement_intent_id,
            ],
        )
        .await
        .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;

        Ok(OpenHoldIntentPrepareOutcome::Ready(SubmissionPreparation {
            settlement_case,
            promise_intent,
            settlement_intent_id: settlement_intent_id.to_string(),
            settlement_submission_id: settlement_submission_id.to_string(),
            internal_idempotency_key,
        }))
    }

    pub(super) async fn persist_open_hold_intent_result(
        &self,
        message: &OutboxMessageRecord,
        prepare: &SubmissionPreparation,
        submission_result: SubmissionResult,
    ) -> Result<OpenHoldIntentPersistResult, HappyRouteError> {
        let event_id = parse_uuid(&message.event_id, "outbox event id")?;
        let settlement_case_id = parse_uuid(
            &prepare.settlement_case.settlement_case_id,
            "settlement case id",
        )?;
        let settlement_submission_id = parse_uuid(
            &prepare.settlement_submission_id,
            "settlement submission id",
        )?;
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        let mut provider_submission_id = None::<String>;

        match submission_result {
            SubmissionResult::Accepted {
                provider_ref,
                provider_submission_id: accepted_submission_id,
                provider_idempotency_key,
                observations,
                ..
            } => {
                let accepted_provider_submission_id = accepted_submission_id
                    .as_ref()
                    .map(|value| value.as_str().to_owned());
                provider_submission_id = accepted_provider_submission_id.clone();
                tx.execute(
                    "
                    UPDATE dao.settlement_submissions
                    SET provider_submission_id = $2,
                        provider_ref = $3,
                        provider_idempotency_key = $4,
                        submission_status = 'accepted',
                        updated_at = CURRENT_TIMESTAMP
                    WHERE settlement_submission_id = $1
                    ",
                    &[
                        &settlement_submission_id,
                        &accepted_provider_submission_id,
                        &provider_ref.as_ref().map(|value| value.as_str().to_owned()),
                        &provider_idempotency_key.as_str(),
                    ],
                )
                .await
                .map_err(db_error)?;
                append_normalized_observations_tx(
                    &tx,
                    &settlement_case_id,
                    Some(&settlement_submission_id),
                    &observations,
                )
                .await?;
            }
            SubmissionResult::Deferred {
                provider_idempotency_key,
                observations,
                ..
            } => {
                tx.execute(
                    "
                    UPDATE dao.settlement_submissions
                    SET provider_idempotency_key = $2,
                        submission_status = 'deferred',
                        updated_at = CURRENT_TIMESTAMP
                    WHERE settlement_submission_id = $1
                    ",
                    &[
                        &settlement_submission_id,
                        &provider_idempotency_key.as_str(),
                    ],
                )
                .await
                .map_err(db_error)?;
                append_normalized_observations_tx(
                    &tx,
                    &settlement_case_id,
                    Some(&settlement_submission_id),
                    &observations,
                )
                .await?;
            }
            SubmissionResult::RejectedPermanent { observations, .. } => {
                update_submission_status_tx(&tx, &settlement_submission_id, "rejected").await?;
                append_normalized_observations_tx(
                    &tx,
                    &settlement_case_id,
                    Some(&settlement_submission_id),
                    &observations,
                )
                .await?;
            }
            SubmissionResult::NeedsManualReview { observations, .. } => {
                update_submission_status_tx(&tx, &settlement_submission_id, "manual_review")
                    .await?;
                append_normalized_observations_tx(
                    &tx,
                    &settlement_case_id,
                    Some(&settlement_submission_id),
                    &observations,
                )
                .await?;
            }
            _ => {
                return Err(HappyRouteError::Internal(
                    "submission result returned an unsupported non-exhaustive variant".to_owned(),
                ));
            }
        }

        insert_outbox_message_tx(
            &tx,
            "settlement_case",
            settlement_case_id,
            EVENT_REFRESH_SETTLEMENT_VIEW,
            &OutboxCommand::RefreshSettlementView {
                settlement_case_id: settlement_case_id.to_string(),
            },
        )
        .await?;
        complete_command_tx(&tx, SETTLEMENT_ORCHESTRATOR, &event_id, None).await?;
        mark_outbox_published_tx(&tx, &event_id).await?;
        tx.commit().await.map_err(db_error)?;

        Ok(OpenHoldIntentPersistResult {
            provider_submission_id,
        })
    }

    pub(super) async fn load_raw_callback(
        &self,
        raw_callback_id: &str,
    ) -> Result<RawProviderCallbackRecord, HappyRouteError> {
        let raw_callback_id = parse_uuid(raw_callback_id, "raw callback id")?;
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "
                SELECT *
                FROM core.raw_provider_callbacks
                WHERE raw_callback_id = $1
                ",
                &[&raw_callback_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(|| HappyRouteError::Provider {
                class: super::types::ProviderErrorClass::ManualReview,
                message: "provider callback raw evidence is missing".to_owned(),
            })?;
        raw_callback_from_row(&row)
    }

    pub(super) async fn attach_callback_amount(
        &self,
        raw_callback_id: &str,
        observed_amount: &Money,
    ) -> Result<(), HappyRouteError> {
        let raw_callback_id = parse_uuid(raw_callback_id, "raw callback id")?;
        let amount_minor_units = i64::try_from(observed_amount.minor_units()).ok();
        let client = self.client.lock().await;
        client
            .execute(
                "
                UPDATE core.raw_provider_callbacks
                SET amount_minor_units = $2,
                    currency_code = $3,
                    amount_scale = $4
                WHERE raw_callback_id = $1
                ",
                &[
                    &raw_callback_id,
                    &amount_minor_units,
                    &observed_amount.currency().as_str(),
                    &(observed_amount.scale() as i32),
                ],
            )
            .await
            .map_err(db_error)?;
        Ok(())
    }

    pub(super) async fn load_callback_context(
        &self,
        parsed: &ParsedPaymentCallback,
        observed_amount: &Money,
        raw_callback_id: &str,
        duplicate_callback: bool,
    ) -> Result<CallbackContext, HappyRouteError> {
        let raw_callback_uuid = parse_uuid(raw_callback_id, "raw callback id")?;
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        let row = tx
            .query_opt(
                "
                SELECT
                    submission.settlement_submission_id,
                    submission.provider_ref,
                    settlement.settlement_case_id,
                    settlement.promise_intent_id,
                    settlement.realm_id AS settlement_realm_id,
                    settlement.case_status,
                    settlement.backend_key,
                    settlement.backend_version,
                    settlement.created_at AS settlement_created_at,
                    settlement.updated_at AS settlement_updated_at,
                    promise.realm_id AS promise_realm_id,
                    promise.initiator_account_id,
                    promise.counterparty_account_id,
                    promise.intent_status,
                    promise.deposit_amount_minor_units,
                    promise.deposit_currency_code,
                    promise.deposit_scale,
                    promise.created_at AS promise_created_at,
                    promise.updated_at AS promise_updated_at,
                    link.pi_uid AS initiator_pi_uid
                FROM dao.settlement_submissions submission
                JOIN dao.settlement_cases settlement
                    ON settlement.settlement_case_id = submission.settlement_case_id
                JOIN dao.promise_intents promise
                    ON promise.promise_intent_id = settlement.promise_intent_id
                JOIN core.pi_account_links link
                    ON link.account_id = promise.initiator_account_id
                WHERE submission.provider_submission_id = $1
                FOR UPDATE
                ",
                &[&parsed.provider_submission_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(|| {
                HappyRouteError::ProviderCallbackMappingDeferred(format!(
                    "provider submission mapping is not ready for provider_submission_id {}",
                    parsed.provider_submission_id
                ))
            })?;
        let promise_intent = promise_intent_from_joined_row(&row)?;
        if promise_intent
            .deposit_amount
            .checked_cmp(observed_amount)
            .map_err(|_| {
                HappyRouteError::BadRequest(
                    "callback amount is incompatible with promise amount".to_owned(),
                )
            })?
            != Ordering::Equal
        {
            return Err(HappyRouteError::BadRequest(
                "callback amount does not match the bounded Promise deposit".to_owned(),
            ));
        }
        let initiator_pi_uid: String = row.get("initiator_pi_uid");
        if initiator_pi_uid != parsed.payer_pi_uid {
            return Err(HappyRouteError::BadRequest(
                "payer_pi_uid does not match the Promise initiator".to_owned(),
            ));
        }
        let provider_ref: Option<String> = row.get("provider_ref");
        tx.execute(
            "
            UPDATE core.raw_provider_callbacks
            SET provider_ref = $2
            WHERE raw_callback_id = $1
            ",
            &[&raw_callback_uuid, &provider_ref],
        )
        .await
        .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;

        let settlement_submission_id: Uuid = row.get("settlement_submission_id");
        Ok(CallbackContext {
            raw_callback_id: raw_callback_id.to_owned(),
            duplicate_callback,
            provider_submission_id: parsed.provider_submission_id.clone(),
            settlement_case: settlement_case_from_joined_row(&row)?,
            settlement_submission_id: settlement_submission_id.to_string(),
            promise_intent,
        })
    }

    pub(super) async fn payment_callback_replay_outcome(
        &self,
        context: &CallbackContext,
    ) -> Result<Option<PaymentCallbackOutcome>, HappyRouteError> {
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "
                SELECT
                    receipt.payment_receipt_id,
                    receipt.settlement_case_id,
                    receipt.promise_intent_id,
                    receipt.receipt_status,
                    settlement.case_status
                FROM core.payment_receipts receipt
                JOIN dao.settlement_cases settlement
                    ON settlement.settlement_case_id = receipt.settlement_case_id
                WHERE receipt.provider_key = $1
                  AND receipt.external_payment_id = $2
                ",
                &[&PROVIDER_KEY, &context.provider_submission_id],
            )
            .await
            .map_err(db_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let payment_receipt_id: Uuid = row.get("payment_receipt_id");
        let settlement_case_id: Uuid = row.get("settlement_case_id");
        let promise_intent_id: Uuid = row.get("promise_intent_id");
        Ok(Some(PaymentCallbackOutcome {
            payment_receipt_id: payment_receipt_id.to_string(),
            raw_callback_id: context.raw_callback_id.clone(),
            settlement_case_id: settlement_case_id.to_string(),
            promise_intent_id: promise_intent_id.to_string(),
            case_status: row.get("case_status"),
            receipt_status: row.get("receipt_status"),
            ledger_journal_id: None,
            outbox_event_ids: Vec::new(),
            duplicate_receipt: true,
        }))
    }

    pub(super) async fn persist_payment_callback_result(
        &self,
        message: &OutboxMessageRecord,
        context: &CallbackContext,
        observed_amount: Money,
        verification: ReceiptVerification,
        normalized_observations: Vec<NormalizedObservation>,
    ) -> Result<PaymentCallbackOutcome, HappyRouteError> {
        let event_id = parse_uuid(&message.event_id, "outbox event id")?;
        let settlement_case_id = parse_uuid(
            &context.settlement_case.settlement_case_id,
            "settlement case id",
        )?;
        let settlement_submission_id = parse_uuid(
            &context.settlement_submission_id,
            "settlement submission id",
        )?;
        let promise_intent_id = parse_uuid(
            &context.promise_intent.promise_intent_id,
            "promise intent id",
        )?;
        let raw_callback_id = parse_uuid(&context.raw_callback_id, "raw callback id")?;
        let amount_minor_units = to_i64(observed_amount.minor_units(), "receipt amount")?;
        let receipt_status = receipt_status(&verification).to_owned();
        let verification_observations = verification_observations(&verification)?;
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;

        if begin_command_tx(&tx, super::constants::PROVIDER_CALLBACK_CONSUMER, message).await?
            == CommandBegin::Completed
        {
            mark_outbox_published_tx(&tx, &event_id).await?;
            tx.commit().await.map_err(db_error)?;
            drop(client);
            return self
                .payment_callback_replay_outcome(context)
                .await?
                .ok_or_else(|| {
                    HappyRouteError::Internal(
                        "completed provider callback command was missing receipt outcome"
                            .to_owned(),
                    )
                });
        }
        append_normalized_observations_tx(
            &tx,
            &settlement_case_id,
            Some(&settlement_submission_id),
            &normalized_observations,
        )
        .await?;
        append_normalized_observations_tx(
            &tx,
            &settlement_case_id,
            Some(&settlement_submission_id),
            verification_observations,
        )
        .await?;

        let existing = tx
            .query_opt(
                "
                SELECT payment_receipt_id, receipt_status, settlement_case_id, promise_intent_id
                FROM core.payment_receipts
                WHERE provider_key = $1
                  AND external_payment_id = $2
                FOR UPDATE
                ",
                &[&PROVIDER_KEY, &context.provider_submission_id],
            )
            .await
            .map_err(db_error)?;

        let mut duplicate_receipt = false;
        let mut payment_receipt_id = existing
            .as_ref()
            .map(|row| row.get::<_, Uuid>("payment_receipt_id"))
            .unwrap_or_else(Uuid::new_v4);
        let mut should_apply_verified_effects = receipt_status == RECEIPT_STATUS_VERIFIED;

        if let Some(row) = existing {
            let existing_status: String = row.get("receipt_status");
            if should_upgrade_existing_receipt(&existing_status, &receipt_status) {
                tx.execute(
                    "
                    UPDATE core.payment_receipts
                    SET amount_minor_units = $2,
                        currency_code = $3,
                        amount_scale = $4,
                        receipt_status = $5,
                        raw_callback_id = $6,
                        updated_at = CURRENT_TIMESTAMP
                    WHERE payment_receipt_id = $1
                    ",
                    &[
                        &payment_receipt_id,
                        &amount_minor_units,
                        &observed_amount.currency().as_str(),
                        &(observed_amount.scale() as i32),
                        &receipt_status,
                        &raw_callback_id,
                    ],
                )
                .await
                .map_err(db_error)?;
            } else {
                duplicate_receipt = true;
                should_apply_verified_effects = false;
            }
        } else {
            let inserted = tx
                .execute(
                    "
                INSERT INTO core.payment_receipts (
                    payment_receipt_id,
                    provider_key,
                    external_payment_id,
                    settlement_case_id,
                    promise_intent_id,
                    amount_minor_units,
                    currency_code,
                    amount_scale,
                    receipt_status,
                    raw_callback_id
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (provider_key, external_payment_id) DO NOTHING
                ",
                    &[
                        &payment_receipt_id,
                        &PROVIDER_KEY,
                        &context.provider_submission_id,
                        &settlement_case_id,
                        &promise_intent_id,
                        &amount_minor_units,
                        &observed_amount.currency().as_str(),
                        &(observed_amount.scale() as i32),
                        &receipt_status,
                        &raw_callback_id,
                    ],
                )
                .await
                .map_err(db_error)?;
            if inserted == 0 {
                let row = tx
                    .query_one(
                        "
                        SELECT payment_receipt_id, receipt_status
                        FROM core.payment_receipts
                        WHERE provider_key = $1
                          AND external_payment_id = $2
                        FOR UPDATE
                        ",
                        &[&PROVIDER_KEY, &context.provider_submission_id],
                    )
                    .await
                    .map_err(db_error)?;
                payment_receipt_id = row.get("payment_receipt_id");
                let existing_status: String = row.get("receipt_status");
                if should_upgrade_existing_receipt(&existing_status, &receipt_status) {
                    tx.execute(
                        "
                        UPDATE core.payment_receipts
                        SET amount_minor_units = $2,
                            currency_code = $3,
                            amount_scale = $4,
                            receipt_status = $5,
                            raw_callback_id = $6,
                            updated_at = CURRENT_TIMESTAMP
                        WHERE payment_receipt_id = $1
                        ",
                        &[
                            &payment_receipt_id,
                            &amount_minor_units,
                            &observed_amount.currency().as_str(),
                            &(observed_amount.scale() as i32),
                            &receipt_status,
                            &raw_callback_id,
                        ],
                    )
                    .await
                    .map_err(db_error)?;
                } else {
                    duplicate_receipt = true;
                    should_apply_verified_effects = false;
                }
            }
        }

        let mut ledger_journal_id = None;
        let mut outbox_event_ids = Vec::new();
        if should_apply_verified_effects {
            let effects = apply_verified_receipt_side_effects_tx(&tx, &settlement_case_id).await?;
            ledger_journal_id = effects.0;
            outbox_event_ids = effects.1;
            if outbox_event_ids.is_empty() && !duplicate_receipt {
                let settlement_refresh_event_id = insert_outbox_message_tx(
                    &tx,
                    "settlement_case",
                    settlement_case_id,
                    EVENT_REFRESH_SETTLEMENT_VIEW,
                    &OutboxCommand::RefreshSettlementView {
                        settlement_case_id: settlement_case_id.to_string(),
                    },
                )
                .await?;
                outbox_event_ids.push(settlement_refresh_event_id);
            }
        } else if !duplicate_receipt {
            let settlement_refresh_event_id = insert_outbox_message_tx(
                &tx,
                "settlement_case",
                settlement_case_id,
                EVENT_REFRESH_SETTLEMENT_VIEW,
                &OutboxCommand::RefreshSettlementView {
                    settlement_case_id: settlement_case_id.to_string(),
                },
            )
            .await?;
            outbox_event_ids.push(settlement_refresh_event_id);
        }
        complete_command_tx(
            &tx,
            super::constants::PROVIDER_CALLBACK_CONSUMER,
            &event_id,
            Some(json!({
                "payment_receipt_id": payment_receipt_id,
                "duplicate_receipt": duplicate_receipt
            })),
        )
        .await?;
        mark_outbox_published_tx(&tx, &event_id).await?;

        let case_status = tx
            .query_one(
                "SELECT case_status FROM dao.settlement_cases WHERE settlement_case_id = $1",
                &[&settlement_case_id],
            )
            .await
            .map_err(db_error)?
            .get("case_status");
        tx.commit().await.map_err(db_error)?;

        Ok(PaymentCallbackOutcome {
            payment_receipt_id: payment_receipt_id.to_string(),
            raw_callback_id: context.raw_callback_id.clone(),
            settlement_case_id: context.settlement_case.settlement_case_id.clone(),
            promise_intent_id: context.promise_intent.promise_intent_id.clone(),
            case_status,
            receipt_status,
            ledger_journal_id: ledger_journal_id.map(|id| id.to_string()),
            outbox_event_ids: outbox_event_ids
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
            duplicate_receipt,
        })
    }

    pub(super) async fn process_refresh_promise_view(
        &self,
        message: &OutboxMessageRecord,
        promise_intent_id: &str,
    ) -> Result<super::types::ProcessedOutboxMessage, HappyRouteError> {
        let event_id = parse_uuid(&message.event_id, "outbox event id")?;
        let promise_intent_id = parse_uuid(promise_intent_id, "promise intent id")?;
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        if begin_command_tx(&tx, PROJECTION_BUILDER, message).await? == CommandBegin::Completed {
            mark_outbox_published_tx(&tx, &event_id).await?;
            tx.commit().await.map_err(db_error)?;
            return Ok(processed_outbox_message(
                message,
                PROJECTION_BUILDER,
                None,
                true,
            ));
        }

        refresh_promise_projection_tx(&tx, &promise_intent_id, None).await?;
        refresh_trust_for_promise_tx(&tx, &promise_intent_id, None).await?;
        complete_command_tx(&tx, PROJECTION_BUILDER, &event_id, None).await?;
        mark_outbox_published_tx(&tx, &event_id).await?;
        tx.commit().await.map_err(db_error)?;
        Ok(processed_outbox_message(
            message,
            PROJECTION_BUILDER,
            None,
            false,
        ))
    }

    pub(super) async fn process_refresh_settlement_view(
        &self,
        message: &OutboxMessageRecord,
        settlement_case_id: &str,
    ) -> Result<super::types::ProcessedOutboxMessage, HappyRouteError> {
        let event_id = parse_uuid(&message.event_id, "outbox event id")?;
        let settlement_case_id = parse_uuid(settlement_case_id, "settlement case id")?;
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        if begin_command_tx(&tx, PROJECTION_BUILDER, message).await? == CommandBegin::Completed {
            mark_outbox_published_tx(&tx, &event_id).await?;
            tx.commit().await.map_err(db_error)?;
            return Ok(processed_outbox_message(
                message,
                PROJECTION_BUILDER,
                None,
                true,
            ));
        }

        let promise_intent_id =
            refresh_settlement_projection_tx(&tx, &settlement_case_id, None).await?;
        refresh_trust_for_promise_tx(&tx, &promise_intent_id, None).await?;
        complete_command_tx(&tx, PROJECTION_BUILDER, &event_id, None).await?;
        mark_outbox_published_tx(&tx, &event_id).await?;
        tx.commit().await.map_err(db_error)?;
        Ok(processed_outbox_message(
            message,
            PROJECTION_BUILDER,
            None,
            false,
        ))
    }

    pub(super) async fn get_settlement_view(
        &self,
        settlement_case_id: &str,
        viewer_account_id: &str,
    ) -> Result<SettlementViewSnapshot, HappyRouteError> {
        let settlement_case_id = match parse_uuid(settlement_case_id, "settlement case id") {
            Ok(settlement_case_id) => settlement_case_id,
            Err(HappyRouteError::BadRequest(_)) => {
                return Err(settlement_projection_not_found());
            }
            Err(error) => return Err(error),
        };
        let viewer_account_id = parse_uuid(viewer_account_id, "viewer account id")?;
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "
                SELECT
                    view.settlement_case_id,
                    view.promise_intent_id,
                    view.realm_id,
                    view.current_settlement_status,
                    view.total_funded_minor_units,
                    view.currency_code,
                    view.latest_journal_entry_id,
                    promise.initiator_account_id,
                    promise.counterparty_account_id
                FROM projection.settlement_views view
                JOIN dao.promise_intents promise
                    ON promise.promise_intent_id = view.promise_intent_id
                WHERE view.settlement_case_id = $1
                ",
                &[&settlement_case_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(settlement_projection_not_found)?;
        let initiator: Uuid = row.get("initiator_account_id");
        let counterparty: Uuid = row.get("counterparty_account_id");
        if viewer_account_id != initiator && viewer_account_id != counterparty {
            return Err(settlement_projection_not_found());
        }
        let settlement_case_id: Uuid = row.get("settlement_case_id");
        let promise_intent_id: Uuid = row.get("promise_intent_id");
        let latest_journal_entry_id: Option<Uuid> = row.get("latest_journal_entry_id");
        let total_funded: i64 = row.get("total_funded_minor_units");
        Ok(SettlementViewSnapshot {
            settlement_case_id: settlement_case_id.to_string(),
            promise_intent_id: promise_intent_id.to_string(),
            realm_id: row.get("realm_id"),
            current_settlement_status: row.get("current_settlement_status"),
            total_funded_minor_units: i128::from(total_funded),
            currency_code: row.get("currency_code"),
            latest_journal_entry_id: latest_journal_entry_id.map(|id| id.to_string()),
        })
    }

    pub(super) async fn get_promise_projection(
        &self,
        promise_intent_id: &str,
        viewer_account_id: &str,
    ) -> Result<PromiseProjectionSnapshot, HappyRouteError> {
        let promise_intent_id = match parse_uuid(promise_intent_id, "promise intent id") {
            Ok(promise_intent_id) => promise_intent_id,
            Err(HappyRouteError::BadRequest(_)) => {
                return Err(promise_projection_not_found());
            }
            Err(error) => return Err(error),
        };
        let viewer_account_id = parse_uuid(viewer_account_id, "viewer account id")?;
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "
                SELECT
                    view.promise_intent_id,
                    view.realm_id,
                    promise.initiator_account_id AS initiator_account_id,
                    promise.counterparty_account_id AS counterparty_account_id,
                    view.current_intent_status,
                    view.deposit_amount_minor_units,
                    view.currency_code,
                    view.deposit_scale,
                    view.latest_settlement_case_id,
                    view.latest_settlement_status,
                    view.source_watermark_at,
                    view.source_fact_count,
                    view.freshness_checked_at,
                    view.projection_lag_ms,
                    view.last_projected_at,
                    view.rebuild_generation
                FROM projection.promise_views view
                JOIN dao.promise_intents promise
                    ON promise.promise_intent_id = view.promise_intent_id
                WHERE view.promise_intent_id = $1
                ",
                &[&promise_intent_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(promise_projection_not_found)?;
        let initiator: Uuid = row.get("initiator_account_id");
        let counterparty: Uuid = row.get("counterparty_account_id");
        if viewer_account_id != initiator && viewer_account_id != counterparty {
            return Err(promise_projection_not_found());
        }

        promise_projection_from_row(&row)
    }

    pub(super) async fn get_expanded_settlement_view(
        &self,
        settlement_case_id: &str,
        viewer_account_id: &str,
    ) -> Result<ExpandedSettlementViewSnapshot, HappyRouteError> {
        let settlement_case_id = match parse_uuid(settlement_case_id, "settlement case id") {
            Ok(settlement_case_id) => settlement_case_id,
            Err(HappyRouteError::BadRequest(_)) => {
                return Err(settlement_projection_not_found());
            }
            Err(error) => return Err(error),
        };
        let viewer_account_id = parse_uuid(viewer_account_id, "viewer account id")?;
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "
                SELECT
                    view.*,
                    promise.initiator_account_id,
                    promise.counterparty_account_id
                FROM projection.settlement_views view
                JOIN dao.promise_intents promise
                    ON promise.promise_intent_id = view.promise_intent_id
                WHERE view.settlement_case_id = $1
                ",
                &[&settlement_case_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(settlement_projection_not_found)?;
        let initiator: Uuid = row.get("initiator_account_id");
        let counterparty: Uuid = row.get("counterparty_account_id");
        if viewer_account_id != initiator && viewer_account_id != counterparty {
            return Err(settlement_projection_not_found());
        }

        expanded_settlement_projection_from_row(&row)
    }

    pub(super) async fn get_trust_snapshot(
        &self,
        account_id: &str,
        viewer_account_id: &str,
    ) -> Result<TrustSnapshot, HappyRouteError> {
        let account_id = match parse_uuid(account_id, "account id") {
            Ok(account_id) => account_id,
            Err(HappyRouteError::BadRequest(_)) => {
                return Err(trust_projection_not_found());
            }
            Err(error) => return Err(error),
        };
        let viewer_account_id = parse_uuid(viewer_account_id, "viewer account id")?;
        if account_id != viewer_account_id {
            return Err(trust_projection_not_found());
        }
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "
                SELECT *
                FROM projection.trust_snapshots
                WHERE account_id = $1
                ",
                &[&account_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(trust_projection_not_found)?;

        trust_snapshot_from_row(&row, None)
    }

    pub(super) async fn get_realm_trust_snapshot(
        &self,
        realm_id: &str,
        account_id: &str,
        viewer_account_id: &str,
    ) -> Result<TrustSnapshot, HappyRouteError> {
        let realm_id = realm_id.trim();
        if realm_id.is_empty() {
            return Err(HappyRouteError::BadRequest(
                "realm_id is required".to_owned(),
            ));
        }
        let account_id = match parse_uuid(account_id, "account id") {
            Ok(account_id) => account_id,
            Err(HappyRouteError::BadRequest(_)) => {
                return Err(trust_projection_not_found());
            }
            Err(error) => return Err(error),
        };
        let viewer_account_id = parse_uuid(viewer_account_id, "viewer account id")?;
        if account_id != viewer_account_id {
            return Err(trust_projection_not_found());
        }
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "
                SELECT *
                FROM projection.realm_trust_snapshots
                WHERE account_id = $1
                  AND realm_id = $2
                ",
                &[&account_id, &realm_id],
            )
            .await
            .map_err(db_error)?
            .ok_or_else(trust_projection_not_found)?;

        trust_snapshot_from_row(&row, Some(realm_id.to_owned()))
    }

    pub(super) async fn rebuild_projection_read_models(
        &self,
    ) -> Result<ProjectionRebuildOutcome, HappyRouteError> {
        let rebuild_generation = Uuid::new_v4();
        let mut client = self.client.lock().await;
        let tx = client.transaction().await.map_err(db_error)?;
        tx.batch_execute(
            "
            TRUNCATE TABLE
                projection.projection_meta,
                projection.realm_trust_snapshots,
                projection.trust_snapshots,
                projection.settlement_views,
                projection.promise_views;
            ",
        )
        .await
        .map_err(db_error)?;

        let promise_rows = tx
            .query(
                "
                SELECT promise_intent_id
                FROM dao.promise_intents
                ORDER BY created_at, promise_intent_id
                ",
                &[],
            )
            .await
            .map_err(db_error)?;
        for row in promise_rows {
            let promise_intent_id: Uuid = row.get("promise_intent_id");
            refresh_promise_projection_tx(&tx, &promise_intent_id, Some(rebuild_generation))
                .await?;
        }

        let settlement_rows = tx
            .query(
                "
                SELECT settlement_case_id
                FROM dao.settlement_cases
                ORDER BY created_at, settlement_case_id
                ",
                &[],
            )
            .await
            .map_err(db_error)?;
        for row in settlement_rows {
            let settlement_case_id: Uuid = row.get("settlement_case_id");
            refresh_settlement_projection_tx(&tx, &settlement_case_id, Some(rebuild_generation))
                .await?;
        }

        let trust_account_rows = tx
            .query(
                "
                SELECT DISTINCT account_id
                FROM (
                    SELECT initiator_account_id AS account_id
                    FROM dao.promise_intents
                    UNION ALL
                    SELECT counterparty_account_id AS account_id
                    FROM dao.promise_intents
                ) participants
                ORDER BY account_id
                ",
                &[],
            )
            .await
            .map_err(db_error)?;
        for row in trust_account_rows {
            let account_id: Uuid = row.get("account_id");
            refresh_global_trust_snapshot_tx(&tx, &account_id, Some(rebuild_generation)).await?;
        }

        let trust_realm_rows = tx
            .query(
                "
                SELECT DISTINCT account_id, realm_id
                FROM (
                    SELECT initiator_account_id AS account_id, realm_id
                    FROM dao.promise_intents
                    UNION ALL
                    SELECT counterparty_account_id AS account_id, realm_id
                    FROM dao.promise_intents
                ) participants
                ORDER BY account_id, realm_id
                ",
                &[],
            )
            .await
            .map_err(db_error)?;
        for row in trust_realm_rows {
            let account_id: Uuid = row.get("account_id");
            let realm_id: String = row.get("realm_id");
            refresh_realm_trust_snapshot_tx(&tx, &account_id, &realm_id, Some(rebuild_generation))
                .await?;
        }

        let rebuilt_at = tx
            .query_one("SELECT CURRENT_TIMESTAMP AS rebuilt_at", &[])
            .await
            .map_err(db_error)?
            .get("rebuilt_at");
        let rebuilt = upsert_projection_meta_tx(&tx, rebuild_generation).await?;
        tx.commit().await.map_err(db_error)?;

        Ok(ProjectionRebuildOutcome {
            rebuild_generation: rebuild_generation.to_string(),
            rebuilt_at,
            rebuilt,
        })
    }

    pub(super) async fn find_provider_attempt_by_request_key(
        &self,
        provider_request_key: &str,
    ) -> Result<Option<ProviderAttemptRecord>, HappyRouteError> {
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "SELECT * FROM dao.provider_attempts WHERE provider_request_key = $1",
                &[&provider_request_key],
            )
            .await
            .map_err(db_error)?;
        row.map(|row| provider_attempt_from_row(&row)).transpose()
    }

    pub(super) async fn find_provider_attempt_by_ref_or_submission(
        &self,
        provider_ref: Option<&str>,
        settlement_submission_id: &str,
    ) -> Result<Option<ProviderAttemptRecord>, HappyRouteError> {
        let settlement_submission_id =
            parse_uuid(settlement_submission_id, "settlement submission id")?;
        let client = self.client.lock().await;
        let row = if let Some(provider_ref) = provider_ref {
            client
                .query_opt(
                    "
                    SELECT *
                    FROM dao.provider_attempts
                    WHERE provider_reference = $1
                    ",
                    &[&provider_ref],
                )
                .await
                .map_err(db_error)?
        } else {
            client
                .query_opt(
                    "
                    SELECT *
                    FROM dao.provider_attempts
                    WHERE settlement_submission_id = $1
                    ORDER BY attempt_no DESC
                    LIMIT 1
                    ",
                    &[&settlement_submission_id],
                )
                .await
                .map_err(db_error)?
        };
        row.map(|row| provider_attempt_from_row(&row)).transpose()
    }

    pub(super) async fn insert_provider_attempt(
        &self,
        attempt: &ProviderAttemptRecord,
    ) -> Result<(), HappyRouteError> {
        let provider_attempt_id = parse_uuid(&attempt.provider_attempt_id, "provider attempt id")?;
        let settlement_intent_id =
            parse_uuid(&attempt.settlement_intent_id, "settlement intent id")?;
        let settlement_submission_id = parse_uuid(
            &attempt.settlement_submission_id,
            "settlement submission id",
        )?;
        let client = self.client.lock().await;
        client
            .execute(
                "
                INSERT INTO dao.provider_attempts (
                    provider_attempt_id,
                    settlement_intent_id,
                    settlement_submission_id,
                    provider_name,
                    attempt_no,
                    provider_request_key,
                    provider_reference,
                    provider_submission_id,
                    request_hash,
                    attempt_status,
                    first_sent_at,
                    last_observed_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                ON CONFLICT (provider_request_key) DO NOTHING
                ",
                &[
                    &provider_attempt_id,
                    &settlement_intent_id,
                    &settlement_submission_id,
                    &attempt.provider_name,
                    &attempt.attempt_no,
                    &attempt.provider_request_key,
                    &attempt.provider_reference,
                    &attempt.provider_submission_id,
                    &attempt.request_hash,
                    &attempt.attempt_status,
                    &attempt.first_sent_at,
                    &attempt.last_observed_at,
                ],
            )
            .await
            .map_err(db_error)?;
        Ok(())
    }

    pub(super) async fn update_provider_attempt_status(
        &self,
        provider_attempt_id: &str,
        status: &str,
    ) -> Result<(), HappyRouteError> {
        let provider_attempt_id = parse_uuid(provider_attempt_id, "provider attempt id")?;
        let client = self.client.lock().await;
        client
            .execute(
                "
                UPDATE dao.provider_attempts
                SET attempt_status = $2,
                    last_observed_at = CURRENT_TIMESTAMP
                WHERE provider_attempt_id = $1
                ",
                &[&provider_attempt_id, &status],
            )
            .await
            .map_err(db_error)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandBegin {
    Started,
    Completed,
}

async fn ensure_account_exists(
    tx: &tokio_postgres::Transaction<'_>,
    account_id: &Uuid,
    not_found_message: &str,
) -> Result<(), HappyRouteError> {
    let exists = tx
        .query_opt(
            "
            SELECT account_id
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
        Err(HappyRouteError::NotFound(not_found_message.to_owned()))
    }
}

async fn insert_outbox_message_tx(
    tx: &tokio_postgres::Transaction<'_>,
    aggregate_type: &str,
    aggregate_id: Uuid,
    event_type: &str,
    command: &OutboxCommand,
) -> Result<Uuid, HappyRouteError> {
    let event_id = Uuid::new_v4();
    let idempotency_key = Uuid::new_v4();
    let payload = command_payload(command);
    let payload_hash = sha256_hex(payload.to_string().as_bytes());
    let stream_key = format!("{aggregate_type}:{aggregate_id}");
    tx.execute(
        "
        INSERT INTO outbox.events (
            event_id,
            idempotency_key,
            aggregate_type,
            aggregate_id,
            event_type,
            schema_version,
            payload_json,
            payload_hash,
            stream_key,
            delivery_status
        )
        VALUES ($1, $2, $3, $4, $5, 1, $6, $7, $8, $9)
        ",
        &[
            &event_id,
            &idempotency_key,
            &aggregate_type,
            &aggregate_id,
            &event_type,
            &payload,
            &payload_hash,
            &stream_key,
            &OUTBOX_PENDING,
        ],
    )
    .await
    .map_err(db_error)?;
    Ok(event_id)
}

async fn begin_command_tx(
    tx: &tokio_postgres::Transaction<'_>,
    consumer_name: &str,
    message: &OutboxMessageRecord,
) -> Result<CommandBegin, HappyRouteError> {
    let event_id = parse_uuid(&message.event_id, "outbox event id")?;
    let payload = command_payload(&message.command);
    let payload_checksum = sha256_hex(payload.to_string().as_bytes());
    tx.execute(
        "
        INSERT INTO outbox.command_inbox (
            inbox_entry_id,
            consumer_name,
            source_event_id,
            command_id,
            payload_checksum,
            status,
            command_type,
            schema_version,
            received_at,
            available_at
        )
        VALUES ($1, $2, $3, $3, $4, 'processing', $5, $6, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT (consumer_name, command_id) DO NOTHING
        ",
        &[
            &Uuid::new_v4(),
            &consumer_name,
            &event_id,
            &payload_checksum,
            &message.event_type,
            &message.schema_version,
        ],
    )
    .await
    .map_err(db_error)?;
    let row = tx
        .query_one(
            "
            SELECT
                status,
                payload_checksum,
                command_type,
                schema_version
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id = $2
            FOR UPDATE
            ",
            &[&consumer_name, &event_id],
        )
        .await
        .map_err(db_error)?;
    let status: String = row.get("status");
    let stored_payload_checksum: Option<String> = row.get("payload_checksum");
    let stored_command_type: String = row.get("command_type");
    let stored_schema_version: i32 = row.get("schema_version");
    if stored_payload_checksum.as_deref() != Some(payload_checksum.as_str())
        || stored_command_type != message.event_type
        || stored_schema_version != message.schema_version
    {
        return Err(HappyRouteError::Internal(
            "stored command inbox entry did not match the outbox payload".to_owned(),
        ));
    }
    if status == "completed" {
        Ok(CommandBegin::Completed)
    } else {
        tx.execute(
            "
            UPDATE outbox.command_inbox
            SET status = 'processing',
                attempt_count = attempt_count + 1,
                claimed_by = $3,
                claimed_until = CURRENT_TIMESTAMP + interval '5 minutes'
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&consumer_name, &event_id, &consumer_name],
        )
        .await
        .map_err(db_error)?;
        Ok(CommandBegin::Started)
    }
}

async fn complete_command_tx(
    tx: &tokio_postgres::Transaction<'_>,
    consumer_name: &str,
    command_id: &Uuid,
    result_json: Option<Value>,
) -> Result<(), HappyRouteError> {
    let result_type = result_json.as_ref().map(|_| "processed");
    tx.execute(
        "
        UPDATE outbox.command_inbox
        SET status = 'completed',
            claimed_by = NULL,
            claimed_until = NULL,
            processed_at = CURRENT_TIMESTAMP,
            completed_at = CURRENT_TIMESTAMP,
            result_type = $3,
            result_json = $4,
            retain_until = CURRENT_TIMESTAMP + ($5::bigint * interval '1 minute')
        WHERE consumer_name = $1
          AND command_id = $2
        ",
        &[
            &consumer_name,
            command_id,
            &result_type,
            &result_json,
            &COMMAND_INBOX_RETENTION_MINUTES,
        ],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn mark_outbox_published_tx(
    tx: &tokio_postgres::Transaction<'_>,
    event_id: &Uuid,
) -> Result<(), HappyRouteError> {
    tx.execute(
        "
        UPDATE outbox.events
        SET delivery_status = $2,
            claimed_by = NULL,
            claimed_until = NULL,
            published_at = CURRENT_TIMESTAMP,
            retain_until = CURRENT_TIMESTAMP + ($3::bigint * interval '1 minute')
        WHERE event_id = $1
        ",
        &[
            event_id,
            &OUTBOX_PUBLISHED,
            &COMMAND_INBOX_RETENTION_MINUTES,
        ],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn mark_outbox_terminal_tx(
    tx: &tokio_postgres::Transaction<'_>,
    event_id: &Uuid,
    status: &str,
    error_class: &str,
    error_message: &str,
) -> Result<(), HappyRouteError> {
    tx.execute(
        "
        UPDATE outbox.events
        SET delivery_status = $2,
            claimed_by = NULL,
            claimed_until = NULL,
            last_error_class = $3,
            last_error_detail = $4,
            retain_until = CURRENT_TIMESTAMP + ($5::bigint * interval '1 minute')
        WHERE event_id = $1
        ",
        &[
            event_id,
            &status,
            &error_class,
            &error_message,
            &COMMAND_INBOX_RETENTION_MINUTES,
        ],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn mark_command_terminal_tx(
    tx: &tokio_postgres::Transaction<'_>,
    consumer_name: &str,
    message: &OutboxMessageRecord,
    error_class: &str,
    error_message: &str,
) -> Result<(), HappyRouteError> {
    let command_id = parse_uuid(&message.event_id, "outbox event id")?;
    let payload = command_payload(&message.command);
    let payload_checksum = sha256_hex(payload.to_string().as_bytes());
    tx.execute(
        "
        INSERT INTO outbox.command_inbox (
            inbox_entry_id,
            consumer_name,
            source_event_id,
            command_id,
            payload_checksum,
            status,
            command_type,
            schema_version,
            received_at,
            available_at,
            completed_at,
            last_error_class,
            last_error_detail,
            retain_until
        )
        VALUES (
            $1,
            $2,
            $3,
            $3,
            $4,
            'quarantined',
            $5,
            $6,
            CURRENT_TIMESTAMP,
            CURRENT_TIMESTAMP,
            CURRENT_TIMESTAMP,
            $7,
            $8,
            CURRENT_TIMESTAMP + ($9::bigint * interval '1 minute')
        )
        ON CONFLICT (consumer_name, command_id) DO UPDATE
        SET status = 'quarantined',
            claimed_by = NULL,
            claimed_until = NULL,
            completed_at = CURRENT_TIMESTAMP,
            last_error_class = EXCLUDED.last_error_class,
            last_error_detail = EXCLUDED.last_error_detail,
            retain_until = EXCLUDED.retain_until
        ",
        &[
            &Uuid::new_v4(),
            &consumer_name,
            &command_id,
            &payload_checksum,
            &message.event_type,
            &message.schema_version,
            &error_class,
            &error_message,
            &COMMAND_INBOX_RETENTION_MINUTES,
        ],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn mark_command_retry_pending_tx(
    tx: &tokio_postgres::Transaction<'_>,
    consumer_name: &str,
    message: &OutboxMessageRecord,
    error_class: &str,
    error_message: &str,
) -> Result<(), HappyRouteError> {
    let command_id = parse_uuid(&message.event_id, "outbox event id")?;
    let payload = command_payload(&message.command);
    let payload_checksum = sha256_hex(payload.to_string().as_bytes());
    tx.execute(
        "
        INSERT INTO outbox.command_inbox (
            inbox_entry_id,
            consumer_name,
            source_event_id,
            command_id,
            payload_checksum,
            status,
            command_type,
            schema_version,
            received_at,
            available_at,
            last_error_class,
            last_error_detail
        )
        VALUES (
            $1,
            $2,
            $3,
            $3,
            $4,
            'pending',
            $5,
            $6,
            CURRENT_TIMESTAMP,
            CURRENT_TIMESTAMP + ($7::bigint * interval '1 millisecond'),
            $8,
            $9
        )
        ON CONFLICT (consumer_name, command_id) DO UPDATE
        SET status = 'pending',
            claimed_by = NULL,
            claimed_until = NULL,
            last_error_class = EXCLUDED.last_error_class,
            last_error_detail = EXCLUDED.last_error_detail,
            available_at = EXCLUDED.available_at
        ",
        &[
            &Uuid::new_v4(),
            &consumer_name,
            &command_id,
            &payload_checksum,
            &message.event_type,
            &message.schema_version,
            &OUTBOX_RETRY_BACKOFF_MILLIS,
            &error_class,
            &error_message,
        ],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

fn command_consumer_name(message: &OutboxMessageRecord) -> &'static str {
    match &message.command {
        OutboxCommand::OpenHoldIntent { .. } => SETTLEMENT_ORCHESTRATOR,
        OutboxCommand::IngestProviderCallback { .. } => PROVIDER_CALLBACK_CONSUMER,
        OutboxCommand::RefreshPromiseView { .. } | OutboxCommand::RefreshSettlementView { .. } => {
            PROJECTION_BUILDER
        }
    }
}

fn settlement_projection_not_found() -> HappyRouteError {
    HappyRouteError::NotFound(
        "settlement projection has not been built for that settlement_case_id".to_owned(),
    )
}

fn promise_projection_not_found() -> HappyRouteError {
    HappyRouteError::NotFound(
        "promise projection has not been built for that promise_intent_id".to_owned(),
    )
}

fn trust_projection_not_found() -> HappyRouteError {
    HappyRouteError::NotFound("trust projection is not visible for that account".to_owned())
}

async fn refresh_promise_projection_tx(
    tx: &tokio_postgres::Transaction<'_>,
    promise_intent_id: &Uuid,
    rebuild_generation: Option<Uuid>,
) -> Result<(), HappyRouteError> {
    tx.execute(
        "
        WITH source AS (
            SELECT
                promise.promise_intent_id,
                promise.realm_id,
                promise.initiator_account_id,
                promise.counterparty_account_id,
                promise.intent_status,
                promise.deposit_amount_minor_units,
                promise.deposit_currency_code,
                promise.deposit_scale,
                settlement.settlement_case_id,
                settlement.case_status AS settlement_status,
                GREATEST(
                    promise.updated_at,
                    COALESCE(settlement.updated_at, promise.updated_at)
                ) AS source_watermark_at,
                (
                    1
                    + CASE WHEN settlement.settlement_case_id IS NULL THEN 0 ELSE 1 END
                )::bigint AS source_fact_count
            FROM dao.promise_intents promise
            LEFT JOIN dao.settlement_cases settlement
                ON settlement.promise_intent_id = promise.promise_intent_id
            WHERE promise.promise_intent_id = $1
        )
        INSERT INTO projection.promise_views (
            promise_intent_id,
            realm_id,
            initiator_account_id,
            counterparty_account_id,
            current_intent_status,
            deposit_amount_minor_units,
            currency_code,
            deposit_scale,
            latest_settlement_case_id,
            latest_settlement_status,
            source_watermark_at,
            source_fact_count,
            freshness_checked_at,
            projection_lag_ms,
            last_projected_at,
            rebuild_generation
        )
        SELECT
            source.promise_intent_id,
            source.realm_id,
            source.initiator_account_id,
            source.counterparty_account_id,
            source.intent_status,
            source.deposit_amount_minor_units,
            source.deposit_currency_code,
            source.deposit_scale,
            source.settlement_case_id,
            source.settlement_status,
            source.source_watermark_at,
            source.source_fact_count,
            CURRENT_TIMESTAMP,
            GREATEST(
                0::bigint,
                (EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - source.source_watermark_at)) * 1000)::bigint
            ),
            CURRENT_TIMESTAMP,
            $2
        FROM source
        ON CONFLICT (promise_intent_id) DO UPDATE
        SET realm_id = EXCLUDED.realm_id,
            initiator_account_id = EXCLUDED.initiator_account_id,
            counterparty_account_id = EXCLUDED.counterparty_account_id,
            current_intent_status = EXCLUDED.current_intent_status,
            deposit_amount_minor_units = EXCLUDED.deposit_amount_minor_units,
            currency_code = EXCLUDED.currency_code,
            deposit_scale = EXCLUDED.deposit_scale,
            latest_settlement_case_id = EXCLUDED.latest_settlement_case_id,
            latest_settlement_status = EXCLUDED.latest_settlement_status,
            source_watermark_at = EXCLUDED.source_watermark_at,
            source_fact_count = EXCLUDED.source_fact_count,
            freshness_checked_at = EXCLUDED.freshness_checked_at,
            projection_lag_ms = EXCLUDED.projection_lag_ms,
            last_projected_at = EXCLUDED.last_projected_at,
            rebuild_generation = EXCLUDED.rebuild_generation
        ",
        &[promise_intent_id, &rebuild_generation],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn refresh_settlement_projection_tx(
    tx: &tokio_postgres::Transaction<'_>,
    settlement_case_id: &Uuid,
    rebuild_generation: Option<Uuid>,
) -> Result<Uuid, HappyRouteError> {
    let row = tx
        .query_one(
            "
            WITH source AS (
                SELECT
                    settlement.settlement_case_id,
                    settlement.realm_id,
                    settlement.promise_intent_id,
                    settlement.case_status,
                    latest_journal.latest_journal_entry_id,
                    COALESCE(funded.total_funded_minor_units, 0) AS total_funded_minor_units,
                    COALESCE(funded.currency_code, promise.deposit_currency_code, $4) AS currency_code,
                    GREATEST(
                        settlement.updated_at,
                        promise.updated_at,
                        COALESCE(journal_facts.latest_created_at, settlement.updated_at),
                        COALESCE(receipt_facts.latest_updated_at, settlement.updated_at),
                        COALESCE(observation_facts.latest_observed_at, settlement.updated_at)
                    ) AS source_watermark_at,
                    (
                        2
                        + COALESCE(journal_facts.journal_count, 0)
                        + COALESCE(journal_facts.posting_count, 0)
                        + COALESCE(receipt_facts.receipt_count, 0)
                        + COALESCE(observation_facts.observation_count, 0)
                    )::bigint AS source_fact_count
                FROM dao.settlement_cases settlement
                JOIN dao.promise_intents promise
                    ON promise.promise_intent_id = settlement.promise_intent_id
                LEFT JOIN LATERAL (
                    SELECT journal_entry_id AS latest_journal_entry_id
                    FROM ledger.journal_entries journal
                    WHERE journal.settlement_case_id = settlement.settlement_case_id
                    ORDER BY journal.created_at DESC, journal.journal_entry_id DESC
                    LIMIT 1
                ) latest_journal ON TRUE
                LEFT JOIN LATERAL (
                    SELECT
                        count(*)::bigint AS journal_count,
                        max(created_at) AS latest_created_at,
                        (
                            SELECT count(*)::bigint
                            FROM ledger.account_postings posting
                            JOIN ledger.journal_entries posting_journal
                                ON posting_journal.journal_entry_id = posting.journal_entry_id
                            WHERE posting_journal.settlement_case_id = settlement.settlement_case_id
                        ) AS posting_count
                    FROM ledger.journal_entries journal
                    WHERE journal.settlement_case_id = settlement.settlement_case_id
                ) journal_facts ON TRUE
                LEFT JOIN LATERAL (
                    SELECT
                        SUM(posting.amount_minor_units) AS total_funded_minor_units,
                        max(posting.currency_code) AS currency_code
                    FROM ledger.journal_entries journal
                    JOIN ledger.account_postings posting
                        ON posting.journal_entry_id = journal.journal_entry_id
                    WHERE journal.settlement_case_id = settlement.settlement_case_id
                      AND posting.ledger_account_code = $2
                      AND posting.direction = $3
                ) funded ON TRUE
                LEFT JOIN LATERAL (
                    SELECT
                        count(*)::bigint AS receipt_count,
                        max(updated_at) AS latest_updated_at
                    FROM core.payment_receipts receipt
                    WHERE receipt.settlement_case_id = settlement.settlement_case_id
                ) receipt_facts ON TRUE
                LEFT JOIN LATERAL (
                    SELECT
                        count(*)::bigint AS observation_count,
                        max(observed_at) AS latest_observed_at
                    FROM dao.settlement_observations observation
                    WHERE observation.settlement_case_id = settlement.settlement_case_id
                ) observation_facts ON TRUE
                WHERE settlement.settlement_case_id = $1
            ),
            upserted AS (
                INSERT INTO projection.settlement_views (
                    settlement_case_id,
                    realm_id,
                    promise_intent_id,
                    latest_journal_entry_id,
                    current_settlement_status,
                    total_funded_minor_units,
                    currency_code,
                    source_watermark_at,
                    source_fact_count,
                    freshness_checked_at,
                    projection_lag_ms,
                    proof_status,
                    proof_signal_count,
                    last_projected_at,
                    rebuild_generation
                )
                SELECT
                    source.settlement_case_id,
                    source.realm_id,
                    source.promise_intent_id,
                    source.latest_journal_entry_id,
                    source.case_status,
                    source.total_funded_minor_units,
                    source.currency_code,
                    source.source_watermark_at,
                    source.source_fact_count,
                    CURRENT_TIMESTAMP,
                    GREATEST(
                        0::bigint,
                        (EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - source.source_watermark_at)) * 1000)::bigint
                    ),
                    'unavailable',
                    0,
                    CURRENT_TIMESTAMP,
                    $5
                FROM source
                ON CONFLICT (settlement_case_id) DO UPDATE
                SET realm_id = EXCLUDED.realm_id,
                    promise_intent_id = EXCLUDED.promise_intent_id,
                    latest_journal_entry_id = EXCLUDED.latest_journal_entry_id,
                    current_settlement_status = EXCLUDED.current_settlement_status,
                    total_funded_minor_units = EXCLUDED.total_funded_minor_units,
                    currency_code = EXCLUDED.currency_code,
                    source_watermark_at = EXCLUDED.source_watermark_at,
                    source_fact_count = EXCLUDED.source_fact_count,
                    freshness_checked_at = EXCLUDED.freshness_checked_at,
                    projection_lag_ms = EXCLUDED.projection_lag_ms,
                    proof_status = EXCLUDED.proof_status,
                    proof_signal_count = EXCLUDED.proof_signal_count,
                    last_projected_at = EXCLUDED.last_projected_at,
                    rebuild_generation = EXCLUDED.rebuild_generation
                RETURNING promise_intent_id
            )
            SELECT promise_intent_id
            FROM upserted
            ",
            &[
                settlement_case_id,
                &LEDGER_ACCOUNT_USER_SECURED_FUNDS_LIABILITY,
                &LEDGER_DIRECTION_CREDIT,
                &PI_CURRENCY_CODE,
                &rebuild_generation,
            ],
        )
        .await
        .map_err(db_error)
        .map_err(|error| match error {
            HappyRouteError::Database { .. } => error,
            other => other,
        })?;
    Ok(row.get("promise_intent_id"))
}

async fn refresh_trust_for_promise_tx(
    tx: &tokio_postgres::Transaction<'_>,
    promise_intent_id: &Uuid,
    rebuild_generation: Option<Uuid>,
) -> Result<(), HappyRouteError> {
    let rows = tx
        .query(
            "
            SELECT DISTINCT account_id, realm_id
            FROM (
                SELECT initiator_account_id AS account_id, realm_id
                FROM dao.promise_intents
                WHERE promise_intent_id = $1
                UNION ALL
                SELECT counterparty_account_id AS account_id, realm_id
                FROM dao.promise_intents
                WHERE promise_intent_id = $1
            ) participants
            ",
            &[promise_intent_id],
        )
        .await
        .map_err(db_error)?;

    for row in rows {
        let account_id: Uuid = row.get("account_id");
        let realm_id: String = row.get("realm_id");
        refresh_global_trust_snapshot_tx(tx, &account_id, rebuild_generation).await?;
        refresh_realm_trust_snapshot_tx(tx, &account_id, &realm_id, rebuild_generation).await?;
    }

    Ok(())
}

async fn refresh_global_trust_snapshot_tx(
    tx: &tokio_postgres::Transaction<'_>,
    account_id: &Uuid,
    rebuild_generation: Option<Uuid>,
) -> Result<(), HappyRouteError> {
    tx.execute(
        "
        WITH account_promises AS (
            SELECT
                promise.promise_intent_id,
                promise.realm_id,
                promise.initiator_account_id,
                promise.counterparty_account_id,
                promise.updated_at AS promise_updated_at,
                settlement.settlement_case_id,
                settlement.case_status,
                settlement.updated_at AS settlement_updated_at
            FROM dao.promise_intents promise
            LEFT JOIN dao.settlement_cases settlement
                ON settlement.promise_intent_id = promise.promise_intent_id
            WHERE promise.initiator_account_id = $1
               OR promise.counterparty_account_id = $1
        ),
        receipt_facts AS (
            SELECT
                count(*)::bigint AS receipt_count,
                count(*) FILTER (WHERE receipt.receipt_status = 'manual_review')::bigint AS manual_review_count,
                max(receipt.updated_at) AS latest_receipt_at
            FROM account_promises promise
            JOIN core.payment_receipts receipt
                ON receipt.promise_intent_id = promise.promise_intent_id
        ),
        observation_facts AS (
            SELECT
                count(*)::bigint AS observation_count,
                max(observation.observed_at) AS latest_observation_at
            FROM account_promises promise
            JOIN dao.settlement_observations observation
                ON observation.settlement_case_id = promise.settlement_case_id
        ),
        facts AS (
            SELECT
                count(*) FILTER (
                    WHERE promise_updated_at >= CURRENT_TIMESTAMP - interval '90 days'
                )::bigint AS promise_participation_count_90d,
                count(*) FILTER (
                    WHERE case_status = 'funded'
                      AND settlement_updated_at >= CURRENT_TIMESTAMP - interval '90 days'
                )::bigint AS funded_settlement_count_90d,
                count(settlement_case_id)::bigint AS settlement_count,
                max(promise_updated_at) AS latest_promise_at,
                max(settlement_updated_at) AS latest_settlement_at
            FROM account_promises
        ),
        source AS (
            SELECT
                facts.promise_participation_count_90d,
                facts.funded_settlement_count_90d,
                COALESCE(receipt_facts.manual_review_count, 0) AS manual_review_count,
                (
                    COALESCE((SELECT count(*) FROM account_promises), 0)
                    + COALESCE(facts.settlement_count, 0)
                    + COALESCE(receipt_facts.receipt_count, 0)
                    + COALESCE(observation_facts.observation_count, 0)
                )::bigint AS source_fact_count,
                GREATEST(
                    COALESCE(facts.latest_promise_at, 'epoch'::timestamptz),
                    COALESCE(facts.latest_settlement_at, 'epoch'::timestamptz),
                    COALESCE(receipt_facts.latest_receipt_at, 'epoch'::timestamptz),
                    COALESCE(observation_facts.latest_observation_at, 'epoch'::timestamptz)
                ) AS source_watermark_at
            FROM facts
            CROSS JOIN receipt_facts
            CROSS JOIN observation_facts
        ),
        shaped AS (
            SELECT
                CASE
                    WHEN manual_review_count > 0 THEN 'review_attention_needed'
                    WHEN funded_settlement_count_90d > 0 THEN 'bounded_reliability_observed'
                    ELSE 'insufficient_authoritative_facts'
                END AS trust_posture,
                (
                    SELECT COALESCE(jsonb_agg(code ORDER BY code), '[]'::jsonb)
                    FROM (
                        VALUES
                            ('deposit_backed_promise_funded', funded_settlement_count_90d > 0),
                            ('manual_review_bucket_nonzero', manual_review_count > 0),
                            ('promise_participation_observed', promise_participation_count_90d > 0),
                            ('proof_unavailable', TRUE)
                    ) AS reasons(code, include_reason)
                    WHERE include_reason
                ) AS reason_codes,
                promise_participation_count_90d,
                funded_settlement_count_90d,
                CASE
                    WHEN manual_review_count = 0 THEN 'none'
                    WHEN manual_review_count <= 2 THEN 'some'
                    ELSE 'multiple'
                END AS manual_review_case_bucket,
                CASE
                    WHEN source_watermark_at = 'epoch'::timestamptz THEN CURRENT_TIMESTAMP
                    ELSE source_watermark_at
                END AS source_watermark_at,
                source_fact_count
            FROM source
        )
        INSERT INTO projection.trust_snapshots (
            account_id,
            trust_posture,
            reason_codes,
            promise_participation_count_90d,
            funded_settlement_count_90d,
            manual_review_case_bucket,
            proof_status,
            proof_signal_count,
            source_watermark_at,
            source_fact_count,
            freshness_checked_at,
            projection_lag_ms,
            last_projected_at,
            rebuild_generation
        )
        SELECT
            $1,
            shaped.trust_posture,
            shaped.reason_codes,
            shaped.promise_participation_count_90d,
            shaped.funded_settlement_count_90d,
            shaped.manual_review_case_bucket,
            'unavailable',
            0,
            shaped.source_watermark_at,
            shaped.source_fact_count,
            CURRENT_TIMESTAMP,
            GREATEST(
                0::bigint,
                (EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - shaped.source_watermark_at)) * 1000)::bigint
            ),
            CURRENT_TIMESTAMP,
            $2
        FROM shaped
        ON CONFLICT (account_id) DO UPDATE
        SET trust_posture = EXCLUDED.trust_posture,
            reason_codes = EXCLUDED.reason_codes,
            promise_participation_count_90d = EXCLUDED.promise_participation_count_90d,
            funded_settlement_count_90d = EXCLUDED.funded_settlement_count_90d,
            manual_review_case_bucket = EXCLUDED.manual_review_case_bucket,
            proof_status = EXCLUDED.proof_status,
            proof_signal_count = EXCLUDED.proof_signal_count,
            source_watermark_at = EXCLUDED.source_watermark_at,
            source_fact_count = EXCLUDED.source_fact_count,
            freshness_checked_at = EXCLUDED.freshness_checked_at,
            projection_lag_ms = EXCLUDED.projection_lag_ms,
            last_projected_at = EXCLUDED.last_projected_at,
            rebuild_generation = EXCLUDED.rebuild_generation
        ",
        &[account_id, &rebuild_generation],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn refresh_realm_trust_snapshot_tx(
    tx: &tokio_postgres::Transaction<'_>,
    account_id: &Uuid,
    realm_id: &str,
    rebuild_generation: Option<Uuid>,
) -> Result<(), HappyRouteError> {
    tx.execute(
        "
        WITH account_promises AS (
            SELECT
                promise.promise_intent_id,
                promise.realm_id,
                promise.initiator_account_id,
                promise.counterparty_account_id,
                promise.updated_at AS promise_updated_at,
                settlement.settlement_case_id,
                settlement.case_status,
                settlement.updated_at AS settlement_updated_at
            FROM dao.promise_intents promise
            LEFT JOIN dao.settlement_cases settlement
                ON settlement.promise_intent_id = promise.promise_intent_id
            WHERE promise.realm_id = $2
              AND (
                    promise.initiator_account_id = $1
                 OR promise.counterparty_account_id = $1
              )
        ),
        receipt_facts AS (
            SELECT
                count(*)::bigint AS receipt_count,
                count(*) FILTER (WHERE receipt.receipt_status = 'manual_review')::bigint AS manual_review_count,
                max(receipt.updated_at) AS latest_receipt_at
            FROM account_promises promise
            JOIN core.payment_receipts receipt
                ON receipt.promise_intent_id = promise.promise_intent_id
        ),
        observation_facts AS (
            SELECT
                count(*)::bigint AS observation_count,
                max(observation.observed_at) AS latest_observation_at
            FROM account_promises promise
            JOIN dao.settlement_observations observation
                ON observation.settlement_case_id = promise.settlement_case_id
        ),
        facts AS (
            SELECT
                count(*) FILTER (
                    WHERE promise_updated_at >= CURRENT_TIMESTAMP - interval '90 days'
                )::bigint AS promise_participation_count_90d,
                count(*) FILTER (
                    WHERE case_status = 'funded'
                      AND settlement_updated_at >= CURRENT_TIMESTAMP - interval '90 days'
                )::bigint AS funded_settlement_count_90d,
                count(settlement_case_id)::bigint AS settlement_count,
                max(promise_updated_at) AS latest_promise_at,
                max(settlement_updated_at) AS latest_settlement_at
            FROM account_promises
        ),
        source AS (
            SELECT
                facts.promise_participation_count_90d,
                facts.funded_settlement_count_90d,
                COALESCE(receipt_facts.manual_review_count, 0) AS manual_review_count,
                (
                    COALESCE((SELECT count(*) FROM account_promises), 0)
                    + COALESCE(facts.settlement_count, 0)
                    + COALESCE(receipt_facts.receipt_count, 0)
                    + COALESCE(observation_facts.observation_count, 0)
                )::bigint AS source_fact_count,
                GREATEST(
                    COALESCE(facts.latest_promise_at, 'epoch'::timestamptz),
                    COALESCE(facts.latest_settlement_at, 'epoch'::timestamptz),
                    COALESCE(receipt_facts.latest_receipt_at, 'epoch'::timestamptz),
                    COALESCE(observation_facts.latest_observation_at, 'epoch'::timestamptz)
                ) AS source_watermark_at
            FROM facts
            CROSS JOIN receipt_facts
            CROSS JOIN observation_facts
        ),
        shaped AS (
            SELECT
                CASE
                    WHEN manual_review_count > 0 THEN 'review_attention_needed'
                    WHEN funded_settlement_count_90d > 0 THEN 'bounded_reliability_observed'
                    ELSE 'insufficient_authoritative_facts'
                END AS trust_posture,
                (
                    SELECT COALESCE(jsonb_agg(code ORDER BY code), '[]'::jsonb)
                    FROM (
                        VALUES
                            ('deposit_backed_promise_funded', funded_settlement_count_90d > 0),
                            ('manual_review_bucket_nonzero', manual_review_count > 0),
                            ('promise_participation_observed', promise_participation_count_90d > 0),
                            ('proof_unavailable', TRUE),
                            ('realm_scoped', TRUE)
                    ) AS reasons(code, include_reason)
                    WHERE include_reason
                ) AS reason_codes,
                promise_participation_count_90d,
                funded_settlement_count_90d,
                CASE
                    WHEN manual_review_count = 0 THEN 'none'
                    WHEN manual_review_count <= 2 THEN 'some'
                    ELSE 'multiple'
                END AS manual_review_case_bucket,
                CASE
                    WHEN source_watermark_at = 'epoch'::timestamptz THEN CURRENT_TIMESTAMP
                    ELSE source_watermark_at
                END AS source_watermark_at,
                source_fact_count
            FROM source
        )
        INSERT INTO projection.realm_trust_snapshots (
            account_id,
            realm_id,
            trust_posture,
            reason_codes,
            promise_participation_count_90d,
            funded_settlement_count_90d,
            manual_review_case_bucket,
            proof_status,
            proof_signal_count,
            source_watermark_at,
            source_fact_count,
            freshness_checked_at,
            projection_lag_ms,
            last_projected_at,
            rebuild_generation
        )
        SELECT
            $1,
            $2,
            shaped.trust_posture,
            shaped.reason_codes,
            shaped.promise_participation_count_90d,
            shaped.funded_settlement_count_90d,
            shaped.manual_review_case_bucket,
            'unavailable',
            0,
            shaped.source_watermark_at,
            shaped.source_fact_count,
            CURRENT_TIMESTAMP,
            GREATEST(
                0::bigint,
                (EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - shaped.source_watermark_at)) * 1000)::bigint
            ),
            CURRENT_TIMESTAMP,
            $3
        FROM shaped
        ON CONFLICT (account_id, realm_id) DO UPDATE
        SET trust_posture = EXCLUDED.trust_posture,
            reason_codes = EXCLUDED.reason_codes,
            promise_participation_count_90d = EXCLUDED.promise_participation_count_90d,
            funded_settlement_count_90d = EXCLUDED.funded_settlement_count_90d,
            manual_review_case_bucket = EXCLUDED.manual_review_case_bucket,
            proof_status = EXCLUDED.proof_status,
            proof_signal_count = EXCLUDED.proof_signal_count,
            source_watermark_at = EXCLUDED.source_watermark_at,
            source_fact_count = EXCLUDED.source_fact_count,
            freshness_checked_at = EXCLUDED.freshness_checked_at,
            projection_lag_ms = EXCLUDED.projection_lag_ms,
            last_projected_at = EXCLUDED.last_projected_at,
            rebuild_generation = EXCLUDED.rebuild_generation
        ",
        &[account_id, &realm_id, &rebuild_generation],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn upsert_projection_meta_tx(
    tx: &tokio_postgres::Transaction<'_>,
    rebuild_generation: Uuid,
) -> Result<Vec<ProjectionRebuildItem>, HappyRouteError> {
    let rows = tx
        .query(
            "
            WITH meta AS (
                SELECT
                    'promise_views'::text AS projection_name,
                    count(*)::bigint AS projection_row_count,
                    COALESCE(sum(source_fact_count), 0)::bigint AS source_fact_count,
                    COALESCE(max(source_watermark_at), CURRENT_TIMESTAMP) AS source_watermark_at
                FROM projection.promise_views
                UNION ALL
                SELECT
                    'settlement_views',
                    count(*)::bigint,
                    COALESCE(sum(source_fact_count), 0)::bigint,
                    COALESCE(max(source_watermark_at), CURRENT_TIMESTAMP)
                FROM projection.settlement_views
                UNION ALL
                SELECT
                    'trust_snapshots',
                    count(*)::bigint,
                    COALESCE(sum(source_fact_count), 0)::bigint,
                    COALESCE(max(source_watermark_at), CURRENT_TIMESTAMP)
                FROM projection.trust_snapshots
                UNION ALL
                SELECT
                    'realm_trust_snapshots',
                    count(*)::bigint,
                    COALESCE(sum(source_fact_count), 0)::bigint,
                    COALESCE(max(source_watermark_at), CURRENT_TIMESTAMP)
                FROM projection.realm_trust_snapshots
            ),
            upserted AS (
                INSERT INTO projection.projection_meta (
                    projection_name,
                    last_rebuilt_at,
                    source_watermark_at,
                    source_fact_count,
                    projection_row_count,
                    projection_lag_ms,
                    rebuild_generation,
                    updated_at
                )
                SELECT
                    meta.projection_name,
                    CURRENT_TIMESTAMP,
                    meta.source_watermark_at,
                    meta.source_fact_count,
                    meta.projection_row_count,
                    GREATEST(
                        0::bigint,
                        (EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - meta.source_watermark_at)) * 1000)::bigint
                    ),
                    $1,
                    CURRENT_TIMESTAMP
                FROM meta
                ON CONFLICT (projection_name) DO UPDATE
                SET last_rebuilt_at = EXCLUDED.last_rebuilt_at,
                    source_watermark_at = EXCLUDED.source_watermark_at,
                    source_fact_count = EXCLUDED.source_fact_count,
                    projection_row_count = EXCLUDED.projection_row_count,
                    projection_lag_ms = EXCLUDED.projection_lag_ms,
                    rebuild_generation = EXCLUDED.rebuild_generation,
                    updated_at = EXCLUDED.updated_at
                RETURNING
                    projection_name,
                    projection_row_count,
                    source_fact_count,
                    source_watermark_at,
                    projection_lag_ms
            )
            SELECT *
            FROM upserted
            ORDER BY projection_name
            ",
            &[&rebuild_generation],
        )
        .await
        .map_err(db_error)?;

    Ok(rows
        .into_iter()
        .map(|row| ProjectionRebuildItem {
            projection_name: row.get("projection_name"),
            projection_row_count: row.get("projection_row_count"),
            source_fact_count: row.get("source_fact_count"),
            source_watermark_at: row.get("source_watermark_at"),
            projection_lag_ms: row.get("projection_lag_ms"),
        })
        .collect())
}

fn db_error(error: tokio_postgres::Error) -> HappyRouteError {
    let message = format!("database operation failed: {error}");
    if let Some(db_error) = error.as_db_error() {
        let code = db_error.code().code().to_owned();
        let retryable = matches!(
            db_error.code(),
            &SqlState::T_R_SERIALIZATION_FAILURE | &SqlState::T_R_DEADLOCK_DETECTED
        );
        return HappyRouteError::Database {
            message,
            code: Some(code),
            constraint: db_error.constraint().map(str::to_owned),
            retryable,
        };
    }

    HappyRouteError::Database {
        message,
        code: None,
        constraint: None,
        retryable: true,
    }
}

async fn update_submission_status_tx(
    tx: &tokio_postgres::Transaction<'_>,
    settlement_submission_id: &Uuid,
    status: &str,
) -> Result<(), HappyRouteError> {
    tx.execute(
        "
        UPDATE dao.settlement_submissions
        SET submission_status = $2,
            updated_at = CURRENT_TIMESTAMP
        WHERE settlement_submission_id = $1
        ",
        &[settlement_submission_id, &status],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn load_settlement_case_tx(
    tx: &tokio_postgres::Transaction<'_>,
    settlement_case_id: &Uuid,
) -> Result<SettlementCaseRecord, HappyRouteError> {
    let row = tx
        .query_opt(
            "
            SELECT *
            FROM dao.settlement_cases
            WHERE settlement_case_id = $1
            ",
            &[settlement_case_id],
        )
        .await
        .map_err(db_error)?
        .ok_or_else(|| {
            HappyRouteError::NotFound("settlement case referenced by outbox is missing".to_owned())
        })?;
    settlement_case_from_row(&row)
}

async fn load_promise_intent_tx(
    tx: &tokio_postgres::Transaction<'_>,
    promise_intent_id: &Uuid,
) -> Result<super::state::PromiseIntentRecord, HappyRouteError> {
    let row = tx
        .query_opt(
            "
            SELECT *
            FROM dao.promise_intents
            WHERE promise_intent_id = $1
            ",
            &[promise_intent_id],
        )
        .await
        .map_err(db_error)?
        .ok_or_else(|| {
            HappyRouteError::NotFound(
                "promise intent referenced by settlement case is missing".to_owned(),
            )
        })?;
    promise_intent_from_row(&row)
}

async fn append_normalized_observations_tx(
    tx: &tokio_postgres::Transaction<'_>,
    settlement_case_id: &Uuid,
    settlement_submission_id: Option<&Uuid>,
    observations: &[NormalizedObservation],
) -> Result<(), HappyRouteError> {
    for observation in observations {
        let observation_id = parse_or_new_uuid(observation.observation_id.as_str());
        let observation_kind = normalized_observation_kind(observation.kind).to_owned();
        let confidence = observation_confidence(observation.confidence).to_owned();
        let provider_ref = observation
            .provider_ref
            .as_ref()
            .map(|value| value.as_str().to_owned());
        let provider_tx_hash = observation
            .provider_tx_hash
            .as_ref()
            .map(|value| value.as_str().to_owned());
        let dedupe_key = normalized_observation_dedupe_key(
            &settlement_case_id.to_string(),
            settlement_submission_id.map(|id| id.to_string()).as_deref(),
            &observation_kind,
            &confidence,
            provider_ref.as_deref(),
            provider_tx_hash.as_deref(),
        );
        let observed_at = observation
            .observed_at
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(Utc::now);
        tx.execute(
            "
            INSERT INTO dao.settlement_observations (
                observation_id,
                settlement_case_id,
                settlement_submission_id,
                observation_kind,
                confidence,
                provider_ref,
                provider_tx_hash,
                observed_at,
                observation_dedupe_key
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (observation_dedupe_key) DO NOTHING
            ",
            &[
                &observation_id,
                settlement_case_id,
                &settlement_submission_id,
                &observation_kind,
                &confidence,
                &provider_ref,
                &provider_tx_hash,
                &observed_at,
                &dedupe_key,
            ],
        )
        .await
        .map_err(db_error)?;
    }
    Ok(())
}

async fn apply_verified_receipt_side_effects_tx(
    tx: &tokio_postgres::Transaction<'_>,
    settlement_case_id: &Uuid,
) -> Result<(Option<Uuid>, Vec<Uuid>), HappyRouteError> {
    let row = tx
        .query_one(
            "
            SELECT
                settlement.settlement_case_id,
                settlement.promise_intent_id,
                settlement.realm_id AS settlement_realm_id,
                settlement.case_status,
                settlement.backend_key,
                settlement.backend_version,
                promise.initiator_account_id,
                promise.counterparty_account_id,
                promise.intent_status,
                promise.deposit_amount_minor_units,
                promise.deposit_currency_code,
                promise.deposit_scale
            FROM dao.settlement_cases settlement
            JOIN dao.promise_intents promise
                ON promise.promise_intent_id = settlement.promise_intent_id
            WHERE settlement.settlement_case_id = $1
            FOR UPDATE
            ",
            &[settlement_case_id],
        )
        .await
        .map_err(db_error)?;
    let case_status: String = row.get("case_status");
    if case_status == SETTLEMENT_CASE_FUNDED {
        return Ok((None, Vec::new()));
    }
    tx.execute(
        "
        UPDATE dao.settlement_cases
        SET case_status = $2,
            updated_at = CURRENT_TIMESTAMP
        WHERE settlement_case_id = $1
        ",
        &[settlement_case_id, &SETTLEMENT_CASE_FUNDED],
    )
    .await
    .map_err(db_error)?;

    let promise_intent_id: Uuid = row.get("promise_intent_id");
    let realm_id: String = row.get("settlement_realm_id");
    let initiator_account_id: Uuid = row.get("initiator_account_id");
    let amount_minor_units: i64 = row.get("deposit_amount_minor_units");
    let currency_code: String = row.get("deposit_currency_code");
    let journal_entry_id = Uuid::new_v4();
    tx.execute(
        "
        INSERT INTO ledger.journal_entries (
            journal_entry_id,
            settlement_case_id,
            promise_intent_id,
            realm_id,
            entry_kind,
            effective_at
        )
        VALUES ($1, $2, $3, $4, 'receipt_recognized', CURRENT_TIMESTAMP)
        ",
        &[
            &journal_entry_id,
            settlement_case_id,
            &promise_intent_id,
            &realm_id,
        ],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "
        INSERT INTO ledger.account_postings (
            posting_id,
            journal_entry_id,
            posting_order,
            ledger_account_code,
            account_id,
            direction,
            amount_minor_units,
            currency_code
        )
        VALUES
            ($1, $2, 1, $3, NULL, $4, $5, $6),
            ($7, $2, 2, $8, $9, $10, $5, $6)
        ",
        &[
            &Uuid::new_v4(),
            &journal_entry_id,
            &LEDGER_ACCOUNT_PROVIDER_CLEARING_INBOUND,
            &LEDGER_DIRECTION_DEBIT,
            &amount_minor_units,
            &currency_code,
            &Uuid::new_v4(),
            &LEDGER_ACCOUNT_USER_SECURED_FUNDS_LIABILITY,
            &initiator_account_id,
            &LEDGER_DIRECTION_CREDIT,
        ],
    )
    .await
    .map_err(db_error)?;
    let settlement_event_id = insert_outbox_message_tx(
        tx,
        "settlement_case",
        *settlement_case_id,
        EVENT_REFRESH_SETTLEMENT_VIEW,
        &OutboxCommand::RefreshSettlementView {
            settlement_case_id: settlement_case_id.to_string(),
        },
    )
    .await?;
    let promise_event_id = insert_outbox_message_tx(
        tx,
        "promise_intent",
        promise_intent_id,
        EVENT_REFRESH_PROMISE_VIEW,
        &OutboxCommand::RefreshPromiseView {
            promise_intent_id: promise_intent_id.to_string(),
        },
    )
    .await?;
    Ok((
        Some(journal_entry_id),
        vec![settlement_event_id, promise_event_id],
    ))
}

fn outbox_message_from_row(row: &Row) -> Result<OutboxMessageRecord, HappyRouteError> {
    let event_id: Uuid = row.get("event_id");
    let idempotency_key: Uuid = row.get("idempotency_key");
    let aggregate_id: Uuid = row.get("aggregate_id");
    let payload: Value = row.get("payload_json");
    let event_type: String = row.get("event_type");
    let command = outbox_command_from_payload(&event_type, &payload)?;
    Ok(OutboxMessageRecord {
        event_id: event_id.to_string(),
        idempotency_key: idempotency_key.to_string(),
        aggregate_type: row.get("aggregate_type"),
        aggregate_id: aggregate_id.to_string(),
        event_type,
        schema_version: row.get("schema_version"),
        command,
        delivery_status: row.get("delivery_status"),
        attempt_count: row.get("attempt_count"),
        last_error_class: row.get("last_error_class"),
        last_error_message: row.get("last_error_detail"),
        available_at: row.get("available_at"),
        published_at: row.get("published_at"),
        created_at: row.get("created_at"),
    })
}

fn promise_intent_from_row(
    row: &Row,
) -> Result<super::state::PromiseIntentRecord, HappyRouteError> {
    let promise_intent_id: Uuid = row.get("promise_intent_id");
    let initiator_account_id: Uuid = row.get("initiator_account_id");
    let counterparty_account_id: Uuid = row.get("counterparty_account_id");
    let amount_minor_units: i64 = row.get("deposit_amount_minor_units");
    let currency_code: String = row.get("deposit_currency_code");
    let deposit_scale: i32 = row.get("deposit_scale");
    Ok(super::state::PromiseIntentRecord {
        promise_intent_id: promise_intent_id.to_string(),
        internal_idempotency_key: String::new(),
        realm_id: row.get("realm_id"),
        initiator_account_id: initiator_account_id.to_string(),
        counterparty_account_id: counterparty_account_id.to_string(),
        deposit_amount: Money::new(
            CurrencyCode::new(&currency_code).map_err(|_| {
                HappyRouteError::Internal("stored currency code is invalid".to_owned())
            })?,
            i128::from(amount_minor_units),
            deposit_scale as u32,
        ),
        intent_status: row.get("intent_status"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn promise_intent_from_joined_row(
    row: &Row,
) -> Result<super::state::PromiseIntentRecord, HappyRouteError> {
    let promise_intent_id: Uuid = row.get("promise_intent_id");
    let initiator_account_id: Uuid = row.get("initiator_account_id");
    let counterparty_account_id: Uuid = row.get("counterparty_account_id");
    let amount_minor_units: i64 = row.get("deposit_amount_minor_units");
    let currency_code: String = row.get("deposit_currency_code");
    let deposit_scale: i32 = row.get("deposit_scale");
    Ok(super::state::PromiseIntentRecord {
        promise_intent_id: promise_intent_id.to_string(),
        internal_idempotency_key: String::new(),
        realm_id: row.get("promise_realm_id"),
        initiator_account_id: initiator_account_id.to_string(),
        counterparty_account_id: counterparty_account_id.to_string(),
        deposit_amount: Money::new(
            CurrencyCode::new(&currency_code).map_err(|_| {
                HappyRouteError::Internal("stored currency code is invalid".to_owned())
            })?,
            i128::from(amount_minor_units),
            deposit_scale as u32,
        ),
        intent_status: row.get("intent_status"),
        created_at: row.get("promise_created_at"),
        updated_at: row.get("promise_updated_at"),
    })
}

fn settlement_case_from_row(row: &Row) -> Result<SettlementCaseRecord, HappyRouteError> {
    let settlement_case_id: Uuid = row.get("settlement_case_id");
    let promise_intent_id: Uuid = row.get("promise_intent_id");
    let backend_key: String = row.get("backend_key");
    let backend_version: String = row.get("backend_version");
    Ok(SettlementCaseRecord {
        settlement_case_id: settlement_case_id.to_string(),
        promise_intent_id: promise_intent_id.to_string(),
        realm_id: row.get("realm_id"),
        case_status: row.get("case_status"),
        backend_pin: BackendPin::new(
            BackendKey::new(backend_key),
            BackendVersion::new(backend_version),
        ),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn settlement_case_from_joined_row(row: &Row) -> Result<SettlementCaseRecord, HappyRouteError> {
    let settlement_case_id: Uuid = row.get("settlement_case_id");
    let promise_intent_id: Uuid = row.get("promise_intent_id");
    let backend_key: String = row.get("backend_key");
    let backend_version: String = row.get("backend_version");
    Ok(SettlementCaseRecord {
        settlement_case_id: settlement_case_id.to_string(),
        promise_intent_id: promise_intent_id.to_string(),
        realm_id: row.get("settlement_realm_id"),
        case_status: row.get("case_status"),
        backend_pin: BackendPin::new(
            BackendKey::new(backend_key),
            BackendVersion::new(backend_version),
        ),
        created_at: row.get("settlement_created_at"),
        updated_at: row.get("settlement_updated_at"),
    })
}

fn promise_projection_from_row(row: &Row) -> Result<PromiseProjectionSnapshot, HappyRouteError> {
    let promise_intent_id: Uuid = row.get("promise_intent_id");
    let initiator_account_id: Uuid = row.get("initiator_account_id");
    let counterparty_account_id: Uuid = row.get("counterparty_account_id");
    let latest_settlement_case_id: Option<Uuid> = row.get("latest_settlement_case_id");
    let amount_minor_units: i64 = row.get("deposit_amount_minor_units");
    Ok(PromiseProjectionSnapshot {
        promise_intent_id: promise_intent_id.to_string(),
        realm_id: row.get("realm_id"),
        initiator_account_id: initiator_account_id.to_string(),
        counterparty_account_id: counterparty_account_id.to_string(),
        current_intent_status: row.get("current_intent_status"),
        deposit_amount_minor_units: i128::from(amount_minor_units),
        currency_code: row.get("currency_code"),
        deposit_scale: row.get("deposit_scale"),
        latest_settlement_case_id: latest_settlement_case_id.map(|id| id.to_string()),
        latest_settlement_status: row.get("latest_settlement_status"),
        provenance: projection_provenance_from_row(row),
    })
}

fn expanded_settlement_projection_from_row(
    row: &Row,
) -> Result<ExpandedSettlementViewSnapshot, HappyRouteError> {
    let settlement_case_id: Uuid = row.get("settlement_case_id");
    let promise_intent_id: Uuid = row.get("promise_intent_id");
    let latest_journal_entry_id: Option<Uuid> = row.get("latest_journal_entry_id");
    let total_funded: i64 = row.get("total_funded_minor_units");
    let proof_signal_count: i32 = row.get("proof_signal_count");
    Ok(ExpandedSettlementViewSnapshot {
        settlement_case_id: settlement_case_id.to_string(),
        promise_intent_id: promise_intent_id.to_string(),
        realm_id: row.get("realm_id"),
        current_settlement_status: row.get("current_settlement_status"),
        total_funded_minor_units: i128::from(total_funded),
        currency_code: row.get("currency_code"),
        latest_journal_entry_id: latest_journal_entry_id.map(|id| id.to_string()),
        proof_status: row.get("proof_status"),
        proof_signal_count: i64::from(proof_signal_count),
        provenance: projection_provenance_from_row(row),
    })
}

fn trust_snapshot_from_row(
    row: &Row,
    realm_id: Option<String>,
) -> Result<TrustSnapshot, HappyRouteError> {
    let account_id: Uuid = row.get("account_id");
    let promise_participation_count: i64 = row.get("promise_participation_count_90d");
    let funded_settlement_count: i64 = row.get("funded_settlement_count_90d");
    let proof_signal_count: i32 = row.get("proof_signal_count");
    let reason_codes: Value = row.get("reason_codes");
    Ok(TrustSnapshot {
        account_id: account_id.to_string(),
        realm_id,
        trust_posture: row.get("trust_posture"),
        reason_codes: reason_codes_from_json(&reason_codes),
        promise_participation_count_90d: promise_participation_count,
        funded_settlement_count_90d: funded_settlement_count,
        manual_review_case_bucket: row.get("manual_review_case_bucket"),
        proof_status: row.get("proof_status"),
        proof_signal_count: i64::from(proof_signal_count),
        provenance: projection_provenance_from_row(row),
    })
}

fn projection_provenance_from_row(row: &Row) -> ProjectionProvenance {
    let rebuild_generation: Option<Uuid> = row.get("rebuild_generation");
    ProjectionProvenance {
        source_watermark_at: row.get("source_watermark_at"),
        source_fact_count: row.get("source_fact_count"),
        freshness_checked_at: row.get("freshness_checked_at"),
        projection_lag_ms: row.get("projection_lag_ms"),
        last_projected_at: row.get("last_projected_at"),
        rebuild_generation: rebuild_generation.map(|id| id.to_string()),
    }
}

fn reason_codes_from_json(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn raw_callback_from_row(row: &Row) -> Result<RawProviderCallbackRecord, HappyRouteError> {
    let raw_callback_id: Uuid = row.get("raw_callback_id");
    let replay_of_raw_callback_id: Option<Uuid> = row.get("replay_of_raw_callback_id");
    let amount_minor_units: Option<i64> = row.get("amount_minor_units");
    let amount_scale: Option<i32> = row.get("amount_scale");
    let currency_code: Option<String> = row.get("currency_code");
    let amount = match (amount_minor_units, currency_code.as_ref(), amount_scale) {
        (Some(minor_units), Some(currency), Some(scale)) => Some(Money::new(
            CurrencyCode::new(currency).map_err(|_| {
                HappyRouteError::Internal("stored callback currency code is invalid".to_owned())
            })?,
            i128::from(minor_units),
            scale as u32,
        )),
        _ => None,
    };
    let headers: Value = row.get("redacted_headers");
    Ok(RawProviderCallbackRecord {
        raw_callback_id: raw_callback_id.to_string(),
        provider_name: row.get("provider_name"),
        dedupe_key: row.get("dedupe_key"),
        replay_of_raw_callback_id: replay_of_raw_callback_id.map(|id| id.to_string()),
        raw_body_bytes: row.get("raw_body_bytes"),
        raw_body: row.get("raw_body"),
        redacted_headers: redacted_headers_from_json(&headers),
        signature_valid: row.get("signature_valid"),
        provider_submission_id: row.get("provider_submission_id"),
        provider_ref: row.get("provider_ref"),
        payer_pi_uid: row.get("payer_pi_uid"),
        amount_minor_units: amount_minor_units.map(i128::from),
        currency_code,
        amount,
        txid: row.get("txid"),
        callback_status: row.get("callback_status"),
        received_at: row.get("received_at"),
    })
}

fn provider_attempt_from_row(row: &Row) -> Result<ProviderAttemptRecord, HappyRouteError> {
    let provider_attempt_id: Uuid = row.get("provider_attempt_id");
    let settlement_intent_id: Uuid = row.get("settlement_intent_id");
    let settlement_submission_id: Uuid = row.get("settlement_submission_id");
    Ok(ProviderAttemptRecord {
        provider_attempt_id: provider_attempt_id.to_string(),
        settlement_intent_id: settlement_intent_id.to_string(),
        settlement_submission_id: settlement_submission_id.to_string(),
        provider_name: row.get("provider_name"),
        attempt_no: row.get("attempt_no"),
        provider_request_key: row.get("provider_request_key"),
        provider_reference: row.get("provider_reference"),
        provider_submission_id: row.get("provider_submission_id"),
        request_hash: row.get("request_hash"),
        attempt_status: row.get("attempt_status"),
        first_sent_at: row.get("first_sent_at"),
        last_observed_at: row.get("last_observed_at"),
    })
}

fn command_payload(command: &OutboxCommand) -> Value {
    match command {
        OutboxCommand::OpenHoldIntent { settlement_case_id } => {
            json!({ "settlement_case_id": settlement_case_id })
        }
        OutboxCommand::IngestProviderCallback { raw_callback_id } => {
            json!({ "raw_callback_id": raw_callback_id })
        }
        OutboxCommand::RefreshPromiseView { promise_intent_id } => {
            json!({ "promise_intent_id": promise_intent_id })
        }
        OutboxCommand::RefreshSettlementView { settlement_case_id } => {
            json!({ "settlement_case_id": settlement_case_id })
        }
    }
}

fn outbox_command_from_payload(
    event_type: &str,
    payload: &Value,
) -> Result<OutboxCommand, HappyRouteError> {
    let field = |name: &str| {
        payload
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                HappyRouteError::Internal(format!(
                    "outbox payload for {event_type} is missing {name}"
                ))
            })
    };
    match event_type {
        EVENT_OPEN_HOLD_INTENT => Ok(OutboxCommand::OpenHoldIntent {
            settlement_case_id: field("settlement_case_id")?,
        }),
        EVENT_INGEST_PROVIDER_CALLBACK => Ok(OutboxCommand::IngestProviderCallback {
            raw_callback_id: field("raw_callback_id")?,
        }),
        EVENT_REFRESH_PROMISE_VIEW => Ok(OutboxCommand::RefreshPromiseView {
            promise_intent_id: field("promise_intent_id")?,
        }),
        EVENT_REFRESH_SETTLEMENT_VIEW => Ok(OutboxCommand::RefreshSettlementView {
            settlement_case_id: field("settlement_case_id")?,
        }),
        _ => Err(HappyRouteError::Internal(format!(
            "unknown outbox event_type {event_type}"
        ))),
    }
}

fn receipt_status(verification: &ReceiptVerification) -> &'static str {
    match verification {
        ReceiptVerification::Verified { .. } => RECEIPT_STATUS_VERIFIED,
        ReceiptVerification::Rejected { .. } => RECEIPT_STATUS_REJECTED,
        ReceiptVerification::NeedsManualReview { .. } => RECEIPT_STATUS_MANUAL_REVIEW,
        _ => RECEIPT_STATUS_MANUAL_REVIEW,
    }
}

fn verification_observations(
    verification: &ReceiptVerification,
) -> Result<&[NormalizedObservation], HappyRouteError> {
    match verification {
        ReceiptVerification::Verified { observations, .. }
        | ReceiptVerification::Rejected { observations, .. }
        | ReceiptVerification::NeedsManualReview { observations, .. } => Ok(observations),
        _ => Err(HappyRouteError::Internal(
            "receipt verification returned an unsupported non-exhaustive variant".to_owned(),
        )),
    }
}

fn should_upgrade_existing_receipt(existing_status: &str, next_status: &str) -> bool {
    existing_status != RECEIPT_STATUS_VERIFIED && next_status == RECEIPT_STATUS_VERIFIED
}

fn normalized_observation_kind(kind: NormalizedObservationKind) -> &'static str {
    match kind {
        NormalizedObservationKind::ReceiptVerified => "receipt_verified",
        NormalizedObservationKind::SubmissionAccepted => "submission_accepted",
        NormalizedObservationKind::Pending => "pending",
        NormalizedObservationKind::Finalized => "finalized",
        NormalizedObservationKind::Failed => "failed",
        NormalizedObservationKind::Contradictory => "contradictory",
        NormalizedObservationKind::NotFound => "not_found",
        NormalizedObservationKind::CallbackNormalized => "callback_normalized",
        NormalizedObservationKind::Unknown => "unknown",
        _ => "unknown",
    }
}

fn observation_confidence(confidence: ObservationConfidence) -> &'static str {
    match confidence {
        ObservationConfidence::CryptographicProof => "cryptographic_proof",
        ObservationConfidence::ProviderConfirmed => "provider_confirmed",
        ObservationConfidence::HeuristicPending => "heuristic_pending",
        ObservationConfidence::Unknown => "unknown",
        _ => "unknown",
    }
}

fn normalized_observation_dedupe_key(
    settlement_case_id: &str,
    settlement_submission_id: Option<&str>,
    observation_kind: &str,
    confidence: &str,
    provider_ref: Option<&str>,
    provider_tx_hash: Option<&str>,
) -> String {
    [
        settlement_case_id,
        settlement_submission_id.unwrap_or(""),
        observation_kind,
        confidence,
        provider_ref.unwrap_or(""),
        provider_tx_hash.unwrap_or(""),
    ]
    .join("\u{1f}")
}

fn provider_callback_mapping_retry_window_exhausted(
    message: &OutboxMessageRecord,
    error: &HappyRouteError,
) -> bool {
    matches!(
        message.command,
        OutboxCommand::IngestProviderCallback { .. }
    ) && error.is_provider_callback_mapping_deferred()
        && message.attempt_count >= super::constants::PROVIDER_CALLBACK_MAPPING_DEFER_ATTEMPTS
}

fn promise_request_hash(realm_id: &str, counterparty_account_id: &str, amount: &Money) -> String {
    let material = [
        realm_id,
        counterparty_account_id,
        &amount.minor_units().to_string(),
        amount.currency().as_str(),
        &amount.scale().to_string(),
    ]
    .join("\u{1f}");
    sha256_hex(material.as_bytes())
}

fn digest_access_token(access_token: &str) -> String {
    sha256_hex(access_token.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn redacted_headers_json(headers: &[(String, String)]) -> Value {
    Value::Array(
        headers
            .iter()
            .map(|(name, value)| json!({ "name": name, "value": value }))
            .collect(),
    )
}

fn redacted_headers_from_json(value: &Value) -> Vec<(String, String)> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some((
                        item.get("name")?.as_str()?.to_owned(),
                        item.get("value")?.as_str()?.to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_uuid(value: &str, label: &str) -> Result<Uuid, HappyRouteError> {
    Uuid::parse_str(value).map_err(|_| {
        HappyRouteError::BadRequest(format!("{label} must be a valid UUID-backed identifier"))
    })
}

fn parse_or_new_uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).unwrap_or_else(|_| Uuid::new_v4())
}

fn to_i64(value: i128, label: &str) -> Result<i64, HappyRouteError> {
    i64::try_from(value)
        .map_err(|_| HappyRouteError::BadRequest(format!("{label} exceeds BIGINT range")))
}
