use std::{
    cmp::Ordering,
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use musubi_settlement_domain::{
    BackendCapabilities, BackendDescriptor, BackendError, BackendKey, BackendVersion,
    InternalIdempotencyKey, NormalizeCallbackCmd, NormalizedObservation, NormalizedObservationKind,
    ObservationConfidence, ProviderCallbackId, ProviderFamily, ProviderIdempotencyKey, ProviderRef,
    ProviderSubmissionId, ProviderTxHash, ReceiptVerification, SettlementBackend,
    SettlementCapability, SubmissionResult, SubmitActionCmd, VerifyReceiptCmd,
    VerifyReceiptExpectation,
};
use uuid::Uuid;

use crate::SharedState;

use super::{
    constants::{PROVIDER_KEY, PROVIDER_VERSION},
    state::RawProviderCallbackRecord,
};

pub(super) fn stub_backend_descriptor() -> BackendDescriptor {
    BackendDescriptor {
        backend_key: BackendKey::new(PROVIDER_KEY),
        backend_version: BackendVersion::new(PROVIDER_VERSION),
        provider_family: ProviderFamily::PiNetwork,
        execution_mode: musubi_settlement_domain::ExecutionMode::Hybrid,
        capabilities: BackendCapabilities::new(vec![
            SettlementCapability::HoldValue,
            SettlementCapability::NormalizeCallback,
            SettlementCapability::ReceiptVerify,
        ]),
    }
}

#[derive(Clone)]
pub(super) struct StubPiSettlementBackend {
    state: SharedState,
    descriptor: BackendDescriptor,
}

impl StubPiSettlementBackend {
    pub(super) fn new(state: SharedState) -> Self {
        Self {
            state,
            descriptor: stub_backend_descriptor(),
        }
    }

    async fn raw_callback(
        &self,
        raw_callback_ref: &ProviderCallbackId,
    ) -> Result<RawProviderCallbackRecord, BackendError> {
        let store = self.state.happy_route.read().await;
        store
            .raw_provider_callbacks_by_id
            .get(raw_callback_ref.as_str())
            .cloned()
            .ok_or(BackendError::InvalidProviderPayload)
    }
}

#[async_trait]
impl SettlementBackend for StubPiSettlementBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn provider_idempotency_key(
        &self,
        internal_idempotency_key: &InternalIdempotencyKey,
    ) -> Result<ProviderIdempotencyKey, BackendError> {
        Ok(ProviderIdempotencyKey::new(format!(
            "pi-{}",
            internal_idempotency_key.as_str()
        )))
    }

    async fn verify_receipt_impl(
        &self,
        cmd: VerifyReceiptCmd,
    ) -> Result<ReceiptVerification, BackendError> {
        let raw_callback = self.raw_callback(&cmd.raw_callback_ref).await?;

        if raw_callback.callback_status != "completed" {
            return Ok(ReceiptVerification::Rejected {
                reason: musubi_settlement_domain::VerificationRejectReason::ProviderRejected,
                observations: vec![failed_observation(&raw_callback)],
            });
        }

        if let Some(expected) = cmd.expected {
            match expected {
                VerifyReceiptExpectation::Amount(expected_amount) => {
                    if raw_callback
                        .amount
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
                    if raw_callback.amount.currency() != &expected_currency {
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
            provider_ref: Some(ProviderRef::new(raw_callback.payment_id.clone())),
            observed_amount: Some(raw_callback.amount),
            observed_at: Some(SystemTime::now()),
            observations: vec![NormalizedObservation {
                observation_id: musubi_settlement_domain::ObservationId::new(
                    Uuid::new_v4().to_string(),
                ),
                kind: NormalizedObservationKind::ReceiptVerified,
                confidence: ObservationConfidence::ProviderConfirmed,
                observed_at: Some(SystemTime::now()),
                provider_ref: Some(ProviderRef::new(raw_callback.payment_id)),
                provider_tx_hash: raw_callback.txid.map(ProviderTxHash::new),
            }],
        })
    }

    async fn submit_action_impl(
        &self,
        cmd: SubmitActionCmd,
    ) -> Result<SubmissionResult, BackendError> {
        tokio::time::sleep(Duration::from_millis(1)).await;

        let provider_submission_id =
            ProviderSubmissionId::new(format!("pi-payment-{}", cmd.submission_id.as_str()));
        let provider_ref = ProviderRef::new(format!("pi-hold-{}", cmd.case_id.as_str()));
        let provider_idempotency_key =
            self.provider_idempotency_key(&cmd.internal_idempotency_key)?;

        Ok(SubmissionResult::Accepted {
            provider_ref: Some(provider_ref.clone()),
            provider_submission_id: Some(provider_submission_id),
            provider_idempotency_key,
            tx_hash: None,
            observations: vec![NormalizedObservation {
                observation_id: musubi_settlement_domain::ObservationId::new(
                    Uuid::new_v4().to_string(),
                ),
                kind: NormalizedObservationKind::SubmissionAccepted,
                confidence: ObservationConfidence::ProviderConfirmed,
                observed_at: Some(SystemTime::now()),
                provider_ref: Some(provider_ref),
                provider_tx_hash: None,
            }],
        })
    }

    async fn reconcile_submission_impl(
        &self,
        _cmd: musubi_settlement_domain::ReconcileSubmissionCmd,
    ) -> Result<musubi_settlement_domain::ReconcileResult, BackendError> {
        Err(BackendError::CapabilityUnsupported {
            backend_key: BackendKey::new(PROVIDER_KEY),
            capability: SettlementCapability::ReconcileStatus,
        })
    }

    async fn normalize_callback_impl(
        &self,
        cmd: NormalizeCallbackCmd,
    ) -> Result<Vec<NormalizedObservation>, BackendError> {
        let raw_callback = self.raw_callback(&cmd.raw_callback_ref).await?;

        Ok(vec![NormalizedObservation {
            observation_id: musubi_settlement_domain::ObservationId::new(
                Uuid::new_v4().to_string(),
            ),
            kind: NormalizedObservationKind::CallbackNormalized,
            confidence: ObservationConfidence::ProviderConfirmed,
            observed_at: Some(SystemTime::now()),
            provider_ref: Some(ProviderRef::new(raw_callback.payment_id)),
            provider_tx_hash: raw_callback.txid.map(ProviderTxHash::new),
        }])
    }
}

fn failed_observation(raw_callback: &RawProviderCallbackRecord) -> NormalizedObservation {
    NormalizedObservation {
        observation_id: musubi_settlement_domain::ObservationId::new(Uuid::new_v4().to_string()),
        kind: NormalizedObservationKind::Failed,
        confidence: ObservationConfidence::ProviderConfirmed,
        observed_at: Some(SystemTime::now()),
        provider_ref: Some(ProviderRef::new(raw_callback.payment_id.clone())),
        provider_tx_hash: raw_callback.txid.clone().map(ProviderTxHash::new),
    }
}

fn contradictory_observation(raw_callback: &RawProviderCallbackRecord) -> NormalizedObservation {
    NormalizedObservation {
        observation_id: musubi_settlement_domain::ObservationId::new(Uuid::new_v4().to_string()),
        kind: NormalizedObservationKind::Contradictory,
        confidence: ObservationConfidence::ProviderConfirmed,
        observed_at: Some(SystemTime::now()),
        provider_ref: Some(ProviderRef::new(raw_callback.payment_id.clone())),
        provider_tx_hash: raw_callback.txid.clone().map(ProviderTxHash::new),
    }
}
