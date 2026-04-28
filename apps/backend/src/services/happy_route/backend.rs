use std::{
    cmp::Ordering,
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use musubi_settlement_domain::{
    BackendCapabilities, BackendDescriptor, BackendError, BackendKey, BackendVersion,
    InternalIdempotencyKey, Money, NormalizeCallbackCmd, NormalizedObservation,
    NormalizedObservationKind, ObservationConfidence, ProviderCallbackId, ProviderFamily,
    ProviderIdempotencyKey, ProviderPayload, ProviderPayloadValue, ProviderRef,
    ProviderSubmissionId, ProviderTxHash, ReceiptVerification, ReconcileResult,
    ReconcileSubmissionCmd, SettlementBackend, SettlementCapability, SubmissionResult,
    SubmitActionCmd, VerifyReceiptCmd, VerifyReceiptExpectation,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::SharedState;

use super::{
    constants::{PROVIDER_KEY, PROVIDER_VERSION},
    state::{ProviderAttemptRecord, RawProviderCallbackRecord},
    types::HappyRouteError,
};

const PROVIDER_MODE_SANDBOX: &str = "sandbox";
const DEFAULT_PROVIDER_BASE_URL: &str = "https://sandbox.minepi.com/v2";
const DEFAULT_PROVIDER_TIMEOUT_MS: u64 = 3000;

pub(super) fn pi_backend_descriptor() -> BackendDescriptor {
    BackendDescriptor {
        backend_key: BackendKey::new(PROVIDER_KEY),
        backend_version: BackendVersion::new(PROVIDER_VERSION),
        provider_family: ProviderFamily::PiNetwork,
        execution_mode: musubi_settlement_domain::ExecutionMode::Hybrid,
        capabilities: BackendCapabilities::new(vec![
            SettlementCapability::HoldValue,
            SettlementCapability::NormalizeCallback,
            SettlementCapability::ReceiptVerify,
            SettlementCapability::ReconcileStatus,
        ]),
    }
}

#[derive(Clone)]
pub(super) struct PiSettlementBackend {
    state: SharedState,
    descriptor: BackendDescriptor,
    config: PiProviderConfig,
    client: SandboxPiProviderClient,
}

impl PiSettlementBackend {
    pub(super) fn new(state: SharedState) -> Self {
        let config = PiProviderConfig::from_env();
        Self {
            state,
            descriptor: pi_backend_descriptor(),
            client: SandboxPiProviderClient::new(config.clone()),
            config,
        }
    }

    async fn raw_callback(
        &self,
        raw_callback_ref: &ProviderCallbackId,
    ) -> Result<RawProviderCallbackRecord, BackendError> {
        self.state
            .happy_route
            .load_raw_callback(raw_callback_ref.as_str())
            .await
            .map_err(store_error_to_backend_error)
    }
}

#[async_trait]
impl SettlementBackend for PiSettlementBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn provider_idempotency_key(
        &self,
        internal_idempotency_key: &InternalIdempotencyKey,
    ) -> Result<ProviderIdempotencyKey, BackendError> {
        Ok(ProviderIdempotencyKey::new(format!(
            "pi:{}",
            internal_idempotency_key.as_str()
        )))
    }

    async fn verify_receipt_impl(
        &self,
        cmd: VerifyReceiptCmd,
    ) -> Result<ReceiptVerification, BackendError> {
        if !self.config.sandbox_enabled() {
            return Err(BackendError::InvalidConfiguration(format!(
                "PROVIDER_MODE '{}' is unsupported; only sandbox is implemented",
                self.config.mode
            )));
        }

        let raw_callback = self.raw_callback(&cmd.raw_callback_ref).await?;
        let provider_ref = raw_callback
            .provider_ref
            .clone()
            .ok_or(BackendError::InvalidProviderPayload)?;
        let observed_amount = raw_callback
            .amount
            .clone()
            .ok_or(BackendError::InvalidProviderPayload)?;
        let callback_status = raw_callback
            .callback_status
            .as_deref()
            .ok_or(BackendError::InvalidProviderPayload)?;

        match classify_callback_status(callback_status) {
            CallbackStatusClass::Completed => {}
            CallbackStatusClass::Rejected => {
                return Ok(ReceiptVerification::Rejected {
                    reason: musubi_settlement_domain::VerificationRejectReason::ProviderRejected,
                    observations: vec![failed_observation(&raw_callback)],
                });
            }
            CallbackStatusClass::Ambiguous => {
                return Ok(ReceiptVerification::NeedsManualReview {
                    reason: musubi_settlement_domain::ReviewReason::UnknownProviderBehavior,
                    observations: vec![ambiguous_observation(&raw_callback)],
                });
            }
        }

        if let Some(expected) = cmd.expected {
            match expected {
                VerifyReceiptExpectation::Amount(expected_amount) => {
                    if observed_amount
                        .checked_cmp(&expected_amount)
                        .map_err(|_| BackendError::InvalidProviderPayload)?
                        != Ordering::Equal
                    {
                        return Ok(ReceiptVerification::Rejected {
                            reason:
                                musubi_settlement_domain::VerificationRejectReason::AmountMismatch,
                            observations: vec![contradictory_observation(&raw_callback)],
                        });
                    }
                }
                VerifyReceiptExpectation::Currency(expected_currency) => {
                    if observed_amount.currency() != &expected_currency {
                        return Ok(ReceiptVerification::Rejected {
                            reason:
                                musubi_settlement_domain::VerificationRejectReason::CurrencyMismatch,
                            observations: vec![contradictory_observation(&raw_callback)],
                        });
                    }
                }
            }
        }

        Ok(ReceiptVerification::Verified {
            provider_ref: Some(ProviderRef::new(provider_ref)),
            observed_amount: Some(observed_amount),
            observed_at: Some(SystemTime::now()),
            observations: vec![
                NormalizedObservationBuilder::new(&raw_callback)
                    .kind(NormalizedObservationKind::ReceiptVerified)
                    .build(),
            ],
        })
    }

    async fn submit_action_impl(
        &self,
        cmd: SubmitActionCmd,
    ) -> Result<SubmissionResult, BackendError> {
        if !self.config.sandbox_enabled() {
            return Err(BackendError::InvalidConfiguration(format!(
                "PROVIDER_MODE '{}' is unsupported; only sandbox is implemented",
                self.config.mode
            )));
        }

        let provider_idempotency_key =
            self.provider_idempotency_key(&cmd.internal_idempotency_key)?;
        let request_hash = provider_request_hash(&cmd);

        if let Some(attempt) = self
            .state
            .happy_route
            .find_provider_attempt_by_request_key(provider_idempotency_key.as_str())
            .await
            .map_err(store_error_to_backend_error)?
        {
            if attempt.request_hash != request_hash {
                return Err(BackendError::IdempotencyMappingFailed);
            }

            return Ok(SubmissionResult::Accepted {
                provider_ref: attempt.provider_reference.clone().map(ProviderRef::new),
                provider_submission_id: attempt
                    .provider_submission_id
                    .clone()
                    .map(ProviderSubmissionId::new),
                provider_idempotency_key: ProviderIdempotencyKey::new(
                    attempt.provider_request_key.clone(),
                ),
                tx_hash: None,
                observations: vec![provider_observation(
                    NormalizedObservationKind::SubmissionAccepted,
                    attempt.provider_reference.clone().map(ProviderRef::new),
                )],
            });
        }

        let response = self
            .client
            .open_hold(&cmd, provider_idempotency_key.clone(), request_hash.clone())
            .await?;

        let now = chrono::Utc::now();
        let attempt = ProviderAttemptRecord {
            provider_attempt_id: Uuid::new_v4().to_string(),
            settlement_intent_id: cmd.intent_id.as_str().to_owned(),
            settlement_submission_id: cmd.submission_id.as_str().to_owned(),
            provider_name: PROVIDER_KEY.to_owned(),
            attempt_no: 1,
            provider_request_key: provider_idempotency_key.as_str().to_owned(),
            provider_reference: Some(response.provider_ref.as_str().to_owned()),
            provider_submission_id: Some(response.provider_submission_id.as_str().to_owned()),
            request_hash,
            attempt_status: "accepted".to_owned(),
            first_sent_at: now,
            last_observed_at: now,
        };

        if let Some(existing_attempt) = self
            .state
            .happy_route
            .find_provider_attempt_by_request_key(&attempt.provider_request_key)
            .await
            .map_err(store_error_to_backend_error)?
        {
            if existing_attempt.request_hash != attempt.request_hash {
                return Err(BackendError::IdempotencyMappingFailed);
            }
        } else {
            if self
                .state
                .happy_route
                .find_provider_attempt_by_ref_or_submission(
                    attempt.provider_reference.as_deref(),
                    &attempt.settlement_submission_id,
                )
                .await
                .map_err(store_error_to_backend_error)?
                .is_some()
            {
                return Err(BackendError::IdempotencyMappingFailed);
            }
            self.state
                .happy_route
                .insert_provider_attempt(&attempt)
                .await
                .map_err(store_error_to_backend_error)?;
        }

        Ok(SubmissionResult::Accepted {
            provider_ref: Some(response.provider_ref.clone()),
            provider_submission_id: Some(response.provider_submission_id),
            provider_idempotency_key,
            tx_hash: None,
            observations: vec![provider_observation(
                NormalizedObservationKind::SubmissionAccepted,
                Some(response.provider_ref),
            )],
        })
    }

    async fn reconcile_submission_impl(
        &self,
        cmd: ReconcileSubmissionCmd,
    ) -> Result<ReconcileResult, BackendError> {
        if !self.config.sandbox_enabled() {
            return Err(BackendError::InvalidConfiguration(format!(
                "PROVIDER_MODE '{}' is unsupported; only sandbox is implemented",
                self.config.mode
            )));
        }

        let attempt = self
            .state
            .happy_route
            .find_provider_attempt_by_ref_or_submission(
                cmd.provider_ref.as_ref().map(|value| value.as_str()),
                cmd.submission_id.as_str(),
            )
            .await
            .map_err(store_error_to_backend_error)?;

        let Some(attempt) = attempt else {
            return Ok(ReconcileResult::NotFound {
                observations: vec![provider_observation(
                    NormalizedObservationKind::NotFound,
                    cmd.provider_ref,
                )],
            });
        };
        let provider_ref = attempt
            .provider_reference
            .as_ref()
            .map(|value| ProviderRef::new(value.clone()))
            .or(cmd.provider_ref)
            .ok_or(BackendError::InvalidProviderResponse)?;

        let poll_response = self.client.poll_status(provider_ref.clone()).await?;
        self.state
            .happy_route
            .update_provider_attempt_status(
                &attempt.provider_attempt_id,
                poll_response.status_label,
            )
            .await
            .map_err(store_error_to_backend_error)?;

        match poll_response.status {
            ProviderPollStatus::Pending => Ok(ReconcileResult::Pending {
                observations: vec![provider_observation(
                    NormalizedObservationKind::Pending,
                    Some(provider_ref),
                )],
            }),
            ProviderPollStatus::Finalized => Ok(ReconcileResult::Finalized {
                observations: vec![provider_observation(
                    NormalizedObservationKind::Finalized,
                    Some(provider_ref),
                )],
            }),
            ProviderPollStatus::Failed => Ok(ReconcileResult::Contradictory {
                observations: vec![provider_observation(
                    NormalizedObservationKind::Contradictory,
                    Some(provider_ref),
                )],
                reason: musubi_settlement_domain::ContradictionReason::ProviderDisagreedWithReceipt,
            }),
            ProviderPollStatus::Unknown => Ok(ReconcileResult::NeedsManualReview {
                reason: musubi_settlement_domain::ReviewReason::UnknownProviderBehavior,
                observations: vec![provider_observation(
                    NormalizedObservationKind::Unknown,
                    Some(provider_ref),
                )],
            }),
        }
    }

    async fn normalize_callback_impl(
        &self,
        cmd: NormalizeCallbackCmd,
    ) -> Result<Vec<NormalizedObservation>, BackendError> {
        if !self.config.sandbox_enabled() {
            return Err(BackendError::InvalidConfiguration(format!(
                "PROVIDER_MODE '{}' is unsupported; only sandbox is implemented",
                self.config.mode
            )));
        }

        let raw_callback = self.raw_callback(&cmd.raw_callback_ref).await?;

        Ok(vec![
            NormalizedObservationBuilder::new(&raw_callback)
                .kind(NormalizedObservationKind::CallbackNormalized)
                .build(),
        ])
    }
}

#[derive(Clone)]
struct PiProviderConfig {
    mode: String,
    base_url: String,
    api_key_present: bool,
    webhook_secret_present: bool,
    timeout: Duration,
}

impl PiProviderConfig {
    fn from_env() -> Self {
        Self {
            mode: std::env::var("PROVIDER_MODE")
                .ok()
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| PROVIDER_MODE_SANDBOX.to_owned()),
            base_url: std::env::var("PROVIDER_BASE_URL")
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| DEFAULT_PROVIDER_BASE_URL.to_owned()),
            api_key_present: env_secret_present("PROVIDER_API_KEY"),
            webhook_secret_present: env_secret_present("PROVIDER_WEBHOOK_SECRET"),
            timeout: std::env::var("PROVIDER_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .map(Duration::from_millis)
                .unwrap_or_else(|| Duration::from_millis(DEFAULT_PROVIDER_TIMEOUT_MS)),
        }
    }

    fn sandbox_enabled(&self) -> bool {
        self.mode == PROVIDER_MODE_SANDBOX
    }
}

#[derive(Clone)]
struct SandboxPiProviderClient {
    config: PiProviderConfig,
}

impl SandboxPiProviderClient {
    fn new(config: PiProviderConfig) -> Self {
        Self { config }
    }

    async fn open_hold(
        &self,
        cmd: &SubmitActionCmd,
        provider_idempotency_key: ProviderIdempotencyKey,
        request_hash: String,
    ) -> Result<SandboxOpenHoldResponse, BackendError> {
        let bounded_latency = self.config.timeout.min(Duration::from_millis(5));
        tokio::time::sleep(bounded_latency).await;

        let provider_submission_id =
            ProviderSubmissionId::new(format!("pi-payment-{}", cmd.submission_id.as_str()));
        let provider_ref = ProviderRef::new(format!("pi-hold-{}", cmd.case_id.as_str()));

        let request = SandboxOpenHoldRequest {
            idempotency_key: provider_idempotency_key.as_str().to_owned(),
            request_hash,
            amount: cmd.amount.clone(),
            payload: cmd.provider_payload.clone(),
            base_url: self.config.base_url.clone(),
            api_key_present: self.config.api_key_present,
            webhook_secret_present: self.config.webhook_secret_present,
        };
        if !request.is_well_formed() {
            return Err(BackendError::InvalidProviderPayload);
        }

        Ok(SandboxOpenHoldResponse {
            provider_ref,
            provider_submission_id,
        })
    }

    async fn poll_status(
        &self,
        provider_ref: ProviderRef,
    ) -> Result<SandboxPollResponse, BackendError> {
        let bounded_latency = self.config.timeout.min(Duration::from_millis(5));
        tokio::time::sleep(bounded_latency).await;

        let status = if provider_ref.as_str().contains("failed") {
            ProviderPollStatus::Failed
        } else if provider_ref.as_str().contains("finalized") {
            ProviderPollStatus::Finalized
        } else if provider_ref.as_str().contains("unknown") {
            ProviderPollStatus::Unknown
        } else {
            ProviderPollStatus::Pending
        };

        Ok(SandboxPollResponse {
            status,
            status_label: status.as_str(),
        })
    }
}

struct SandboxOpenHoldRequest {
    idempotency_key: String,
    request_hash: String,
    amount: Option<Money>,
    payload: ProviderPayload,
    base_url: String,
    api_key_present: bool,
    webhook_secret_present: bool,
}

impl SandboxOpenHoldRequest {
    fn is_well_formed(&self) -> bool {
        !self.idempotency_key.is_empty()
            && self.request_hash.len() == 64
            && self
                .amount
                .as_ref()
                .map(|amount| amount.minor_units() > 0)
                .unwrap_or(true)
            && !self.payload.schema.name.is_empty()
            && !self.base_url.is_empty()
            && (!self.api_key_present || self.base_url.starts_with("http"))
            && (!self.webhook_secret_present || self.base_url.starts_with("http"))
    }
}

struct SandboxOpenHoldResponse {
    provider_ref: ProviderRef,
    provider_submission_id: ProviderSubmissionId,
}

struct SandboxPollResponse {
    status: ProviderPollStatus,
    status_label: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderPollStatus {
    Pending,
    Finalized,
    Failed,
    Unknown,
}

impl ProviderPollStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Finalized => "finalized",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallbackStatusClass {
    Completed,
    Rejected,
    Ambiguous,
}

fn classify_callback_status(status: &str) -> CallbackStatusClass {
    match status.trim().to_ascii_lowercase().as_str() {
        "completed" | "succeeded" | "success" => CallbackStatusClass::Completed,
        "failed" | "cancelled" | "canceled" | "rejected" | "error" => CallbackStatusClass::Rejected,
        _ => CallbackStatusClass::Ambiguous,
    }
}

fn store_error_to_backend_error(error: HappyRouteError) -> BackendError {
    match error {
        HappyRouteError::NotFound(_) => BackendError::InvalidProviderPayload,
        HappyRouteError::BadRequest(_) => BackendError::InvalidProviderPayload,
        HappyRouteError::Unauthorized(_) => BackendError::InvalidProviderPayload,
        HappyRouteError::Conflict(_) => BackendError::TemporarilyUnavailable,
        HappyRouteError::ProviderCallbackMappingDeferred(_) => BackendError::TemporarilyUnavailable,
        HappyRouteError::Database {
            code,
            constraint,
            retryable,
            ..
        } => {
            if retryable {
                BackendError::TemporarilyUnavailable
            } else if code.as_deref() == Some("23505")
                && constraint.as_deref() == Some("provider_attempts_provider_reference_key")
            {
                BackendError::IdempotencyMappingFailed
            } else {
                BackendError::InvalidProviderResponse
            }
        }
        HappyRouteError::Provider { .. } => BackendError::InvalidProviderResponse,
        HappyRouteError::Internal(_) => BackendError::InvalidProviderResponse,
    }
}

pub(super) fn callback_dedupe_key(raw_body_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(PROVIDER_KEY.len().to_string().as_bytes());
    hasher.update(b":");
    hasher.update(PROVIDER_KEY.as_bytes());
    hasher.update(b";");
    hasher.update(raw_body_bytes.len().to_string().as_bytes());
    hasher.update(b":");
    hasher.update(raw_body_bytes);
    hasher.update(b";");
    let digest = hasher.finalize();
    encode_hex(&digest)
}

fn provider_request_hash(cmd: &SubmitActionCmd) -> String {
    let amount_parts = cmd
        .amount
        .as_ref()
        .map(|amount| {
            vec![
                amount.minor_units().to_string(),
                amount.currency().as_str().to_owned(),
                amount.scale().to_string(),
            ]
        })
        .unwrap_or_else(|| vec!["none".to_owned()]);

    let mut payload_fields = cmd.provider_payload.fields.clone();
    payload_fields.sort_by(|left, right| left.name.cmp(&right.name));

    let mut parts = vec![
        cmd.case_id.as_str().to_owned(),
        cmd.intent_id.as_str().to_owned(),
        cmd.submission_id.as_str().to_owned(),
        cmd.internal_idempotency_key.as_str().to_owned(),
        match cmd.capability {
            SettlementCapability::ReceiptVerify => "receipt_verify".to_owned(),
            SettlementCapability::HoldValue => "hold_value".to_owned(),
            SettlementCapability::ReleaseValue => "release_value".to_owned(),
            SettlementCapability::RefundValue => "refund_value".to_owned(),
            SettlementCapability::CompensateValue => "compensate_value".to_owned(),
            SettlementCapability::AllocateTreasury => "allocate_treasury".to_owned(),
            SettlementCapability::AttestExecution => "attest_execution".to_owned(),
            SettlementCapability::ReconcileStatus => "reconcile_status".to_owned(),
            SettlementCapability::NormalizeCallback => "normalize_callback".to_owned(),
            _ => format!("unhandled_capability:{:?}", cmd.capability),
        },
        cmd.provider_payload.schema.name.clone(),
        cmd.provider_payload.schema.version.to_string(),
    ];
    parts.extend(amount_parts);
    for field in payload_fields {
        parts.push(field.name);
        parts.push(payload_value_for_hash(&field.value));
    }

    digest_owned_parts(&parts)
}

fn payload_value_for_hash(value: &ProviderPayloadValue) -> String {
    match value {
        ProviderPayloadValue::Text(value) => format!("text:{value}"),
        ProviderPayloadValue::Integer(value) => format!("integer:{value}"),
        ProviderPayloadValue::Money(value) => format!(
            "money:{}:{}:{}",
            value.minor_units(),
            value.currency().as_str(),
            value.scale()
        ),
        ProviderPayloadValue::ProviderRef(value) => format!("provider_ref:{}", value.as_str()),
        ProviderPayloadValue::ProviderSubmissionId(value) => {
            format!("provider_submission_id:{}", value.as_str())
        }
        ProviderPayloadValue::ProviderCallbackId(value) => {
            format!("provider_callback_id:{}", value.as_str())
        }
        ProviderPayloadValue::ProviderTxHash(value) => {
            format!("provider_tx_hash:{}", value.as_str())
        }
        ProviderPayloadValue::Boolean(value) => format!("boolean:{value}"),
        _ => format!("unhandled:{value:?}"),
    }
}

fn failed_observation(raw_callback: &RawProviderCallbackRecord) -> NormalizedObservation {
    NormalizedObservationBuilder::new(raw_callback)
        .kind(NormalizedObservationKind::Failed)
        .build()
}

fn ambiguous_observation(raw_callback: &RawProviderCallbackRecord) -> NormalizedObservation {
    NormalizedObservationBuilder::new(raw_callback)
        .kind(NormalizedObservationKind::Unknown)
        .build()
}

fn contradictory_observation(raw_callback: &RawProviderCallbackRecord) -> NormalizedObservation {
    NormalizedObservationBuilder::new(raw_callback)
        .kind(NormalizedObservationKind::Contradictory)
        .build()
}

fn provider_observation(
    kind: NormalizedObservationKind,
    provider_ref: Option<ProviderRef>,
) -> NormalizedObservation {
    NormalizedObservation {
        observation_id: musubi_settlement_domain::ObservationId::new(Uuid::new_v4().to_string()),
        kind,
        confidence: ObservationConfidence::ProviderConfirmed,
        observed_at: Some(SystemTime::now()),
        provider_ref,
        provider_tx_hash: None,
    }
}

struct NormalizedObservationBuilder<'a> {
    raw_callback: &'a RawProviderCallbackRecord,
    kind: NormalizedObservationKind,
}

impl<'a> NormalizedObservationBuilder<'a> {
    fn new(raw_callback: &'a RawProviderCallbackRecord) -> Self {
        Self {
            raw_callback,
            kind: NormalizedObservationKind::CallbackNormalized,
        }
    }

    fn kind(mut self, kind: NormalizedObservationKind) -> Self {
        self.kind = kind;
        self
    }

    fn build(self) -> NormalizedObservation {
        NormalizedObservation {
            observation_id: musubi_settlement_domain::ObservationId::new(
                Uuid::new_v4().to_string(),
            ),
            kind: self.kind,
            confidence: ObservationConfidence::ProviderConfirmed,
            observed_at: Some(SystemTime::now()),
            provider_ref: self.raw_callback.provider_ref.clone().map(ProviderRef::new),
            provider_tx_hash: self.raw_callback.txid.clone().map(ProviderTxHash::new),
        }
    }
}

fn env_secret_present(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| !value.trim().is_empty() && !value.contains("your_"))
        .unwrap_or(false)
}

fn digest_parts(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.len().to_string().as_bytes());
        hasher.update(b":");
        hasher.update(part.as_bytes());
        hasher.update(b";");
    }
    let digest = hasher.finalize();
    encode_hex(&digest)
}

fn digest_owned_parts(parts: &[String]) -> String {
    let borrowed = parts.iter().map(String::as_str).collect::<Vec<_>>();
    digest_parts(&borrowed)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use musubi_settlement_domain::{
        BackendKey, BackendPin, BackendVersion, CurrencyCode, InternalIdempotencyKey, Money,
        ProviderPayloadField, ProviderPayloadSchema, SettlementCaseId, SettlementIntentId,
        SettlementSubmissionId,
    };

    use super::*;

    #[test]
    fn provider_request_hash_changes_when_payload_changes() {
        let first = provider_request_hash(&submit_cmd("realm-a"));
        let second = provider_request_hash(&submit_cmd("realm-b"));

        assert_ne!(first, second);
    }

    #[test]
    fn callback_dedupe_key_is_stable_for_exact_replay() {
        let raw_body = r#"{"payment_id":"payment-1","status":"completed"}"#;

        assert_eq!(
            callback_dedupe_key(raw_body.as_bytes()),
            callback_dedupe_key(raw_body.as_bytes())
        );
    }

    #[test]
    fn callback_status_classification_is_fail_closed_for_unknown_values() {
        assert_eq!(
            classify_callback_status("completed"),
            CallbackStatusClass::Completed
        );
        assert_eq!(
            classify_callback_status("failed"),
            CallbackStatusClass::Rejected
        );
        assert_eq!(
            classify_callback_status("provider-new-state"),
            CallbackStatusClass::Ambiguous
        );
    }

    #[test]
    fn poll_status_labels_are_stable_for_reconcile_mapping() {
        assert_eq!(ProviderPollStatus::Pending.as_str(), "pending");
        assert_eq!(ProviderPollStatus::Finalized.as_str(), "finalized");
        assert_eq!(ProviderPollStatus::Failed.as_str(), "failed");
        assert_eq!(ProviderPollStatus::Unknown.as_str(), "unknown");
    }

    #[test]
    fn database_store_failures_map_to_retryable_backend_errors() {
        let error = store_error_to_backend_error(HappyRouteError::Database {
            message: "database operation failed: writer disconnected".to_owned(),
            code: None,
            constraint: None,
            retryable: true,
        });

        assert_eq!(error, BackendError::TemporarilyUnavailable);
    }

    #[test]
    fn permanent_database_store_failures_fail_closed() {
        let error = store_error_to_backend_error(HappyRouteError::Database {
            message: "database operation failed: check constraint".to_owned(),
            code: Some("23514".to_owned()),
            constraint: Some("outbox_events_last_error_class_check".to_owned()),
            retryable: false,
        });

        assert_eq!(error, BackendError::InvalidProviderResponse);
    }

    #[test]
    fn provider_reference_uniqueness_maps_to_idempotency_failure() {
        let error = store_error_to_backend_error(HappyRouteError::Database {
            message: "database operation failed: duplicate key value".to_owned(),
            code: Some("23505".to_owned()),
            constraint: Some("provider_attempts_provider_reference_key".to_owned()),
            retryable: false,
        });

        assert_eq!(error, BackendError::IdempotencyMappingFailed);
    }

    #[test]
    fn non_database_internal_failures_still_fail_closed() {
        let error = store_error_to_backend_error(HappyRouteError::Internal(
            "stored provider attempt was missing request key".to_owned(),
        ));

        assert_eq!(error, BackendError::InvalidProviderResponse);
    }

    #[tokio::test]
    async fn unsupported_provider_mode_fails_closed() {
        let test_state = crate::new_test_state()
            .await
            .expect("test database state must initialize");
        let config = PiProviderConfig {
            mode: "production".to_owned(),
            base_url: DEFAULT_PROVIDER_BASE_URL.to_owned(),
            api_key_present: true,
            webhook_secret_present: true,
            timeout: Duration::from_millis(DEFAULT_PROVIDER_TIMEOUT_MS),
        };
        let backend = PiSettlementBackend {
            state: test_state.state.clone(),
            descriptor: pi_backend_descriptor(),
            client: SandboxPiProviderClient::new(config.clone()),
            config,
        };

        let error = backend
            .submit_action_impl(submit_cmd("realm-a"))
            .await
            .expect_err("unsupported provider mode must fail closed");

        assert!(matches!(
            error,
            BackendError::InvalidConfiguration(message)
                if message.contains("only sandbox is implemented")
        ));
    }

    fn submit_cmd(realm_id: &str) -> SubmitActionCmd {
        SubmitActionCmd {
            backend: BackendPin::new(
                BackendKey::new(PROVIDER_KEY),
                BackendVersion::new(PROVIDER_VERSION),
            ),
            case_id: SettlementCaseId::new("case-1"),
            intent_id: SettlementIntentId::new("intent-1"),
            submission_id: SettlementSubmissionId::new("submission-1"),
            internal_idempotency_key: InternalIdempotencyKey::new("key-1"),
            capability: SettlementCapability::HoldValue,
            amount: Some(Money::new(CurrencyCode::new("PI").unwrap(), 10000, 3)),
            provider_payload: ProviderPayload::new(
                ProviderPayloadSchema::new("pi-hold-intent", 1),
                vec![ProviderPayloadField::new(
                    "realm_id",
                    ProviderPayloadValue::Text(realm_id.to_owned()),
                )],
            ),
        }
    }
}
