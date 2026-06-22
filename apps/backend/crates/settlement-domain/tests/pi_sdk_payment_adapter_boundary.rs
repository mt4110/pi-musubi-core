use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

use async_trait::async_trait;
use musubi_settlement_domain::{
    BackendCapabilities, BackendDescriptor, BackendError, BackendKey, BackendVersion, CurrencyCode,
    ExecutionMode, InternalIdempotencyKey, Money, NormalizeCallbackCmd, NormalizedObservation,
    ProviderFamily, ProviderIdempotencyKey, ProviderPayload, ProviderPayloadField,
    ProviderPayloadSchema, ProviderPayloadValue, ReceiptVerification, ReconcileResult,
    ReconcileSubmissionCmd, SettlementBackend, SettlementCapability, SettlementCaseId,
    SettlementIntentId, SettlementSubmissionId, SubmissionResult, SubmitActionCmd,
    VerifyReceiptCmd,
};

const BACKEND_KEY: &str = "pi-sdk-payment-receipt-evidence";
const BACKEND_VERSION: &str = "foundation-565-local-boundary";
const RECEIPT_SCHEMA: &str = "pi-sdk-payment-receipt-evidence";

#[test]
fn pi_sdk_payment_candidate_is_receipt_evidence_only() {
    let descriptor = pi_sdk_payment_descriptor();

    assert_eq!(descriptor.provider_family, ProviderFamily::PiNetwork);
    assert_eq!(descriptor.execution_mode, ExecutionMode::Asynchronous);
    assert!(descriptor.supports(SettlementCapability::ReceiptVerify));
    assert!(descriptor.supports(SettlementCapability::NormalizeCallback));
    assert!(descriptor.supports(SettlementCapability::ReconcileStatus));
}

#[test]
fn value_movement_capabilities_stay_outside_pi_sdk_payment_boundary() {
    let descriptor = pi_sdk_payment_descriptor();

    for capability in [
        SettlementCapability::HoldValue,
        SettlementCapability::ReleaseValue,
        SettlementCapability::RefundValue,
        SettlementCapability::CompensateValue,
        SettlementCapability::AllocateTreasury,
        SettlementCapability::AttestExecution,
    ] {
        assert!(
            !descriptor.supports(capability),
            "Pi SDK payment boundary must not expose {capability:?}"
        );
    }
}

#[test]
fn unsupported_value_movement_fails_before_provider_path_is_polled() {
    let backend = BoundaryBackend::new();

    let result = poll_ready(backend.submit_action(SubmitActionCmd {
        backend: backend.descriptor().pin(),
        case_id: SettlementCaseId::new("case_pi_sdk_payment_boundary"),
        intent_id: SettlementIntentId::new("intent_pi_sdk_payment_boundary"),
        submission_id: SettlementSubmissionId::new("submission_pi_sdk_payment_boundary"),
        internal_idempotency_key: InternalIdempotencyKey::new("internal-pi-sdk-payment-boundary"),
        capability: SettlementCapability::ReleaseValue,
        amount: Some(pi_amount(3141)),
        provider_payload: receipt_payload(),
    }));

    assert_eq!(
        result,
        Err(BackendError::CapabilityUnsupported {
            backend_key: BackendKey::new(BACKEND_KEY),
            capability: SettlementCapability::ReleaseValue,
        })
    );
    assert_eq!(backend.submit_action_impl_calls(), 0);
}

#[test]
fn provider_idempotency_mapping_is_stable_and_provider_scoped() {
    let backend = BoundaryBackend::new();
    let internal = InternalIdempotencyKey::new("settlement-case-123:receipt-456");

    let left = backend
        .provider_idempotency_key(&internal)
        .expect("mapping should be deterministic");
    let right = backend
        .provider_idempotency_key(&internal)
        .expect("mapping should be deterministic");

    assert_eq!(left, right);
    assert_eq!(
        left,
        ProviderIdempotencyKey::new("pi-sdk-payment:settlement-case-123:receipt-456")
    );
}

#[test]
fn receipt_payload_keeps_money_exact_and_separate_from_business_truth() {
    let payload = receipt_payload();

    assert_eq!(
        payload,
        ProviderPayload::new(
            ProviderPayloadSchema::new(RECEIPT_SCHEMA, 1),
            vec![ProviderPayloadField::new(
                "observed_amount",
                ProviderPayloadValue::Money(pi_amount(3141))
            )]
        )
    );
    assert_eq!(pi_amount(3141).currency().as_str(), "PI");
    assert_eq!(pi_amount(3141).minor_units(), 3141);
    assert_eq!(pi_amount(3141).scale(), 3);
}

fn pi_sdk_payment_descriptor() -> BackendDescriptor {
    BackendDescriptor {
        backend_key: BackendKey::new(BACKEND_KEY),
        backend_version: BackendVersion::new(BACKEND_VERSION),
        provider_family: ProviderFamily::PiNetwork,
        execution_mode: ExecutionMode::Asynchronous,
        capabilities: BackendCapabilities::new(vec![
            SettlementCapability::ReceiptVerify,
            SettlementCapability::NormalizeCallback,
            SettlementCapability::ReconcileStatus,
        ]),
    }
}

fn receipt_payload() -> ProviderPayload {
    ProviderPayload::new(
        ProviderPayloadSchema::new(RECEIPT_SCHEMA, 1),
        vec![ProviderPayloadField::new(
            "observed_amount",
            ProviderPayloadValue::Money(pi_amount(3141)),
        )],
    )
}

fn pi_amount(minor_units: i128) -> Money {
    Money::new(
        CurrencyCode::new("PI").expect("PI currency code must be valid"),
        minor_units,
        3,
    )
}

fn poll_ready<F>(future: F) -> F::Output
where
    F: Future,
{
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);

    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("boundary check unexpectedly reached an async provider path"),
    }
}

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

struct BoundaryBackend {
    descriptor: BackendDescriptor,
    submit_action_impl_calls: AtomicUsize,
}

impl BoundaryBackend {
    fn new() -> Self {
        Self {
            descriptor: pi_sdk_payment_descriptor(),
            submit_action_impl_calls: AtomicUsize::new(0),
        }
    }

    fn submit_action_impl_calls(&self) -> usize {
        self.submit_action_impl_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SettlementBackend for BoundaryBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn provider_idempotency_key(
        &self,
        internal_idempotency_key: &InternalIdempotencyKey,
    ) -> Result<ProviderIdempotencyKey, BackendError> {
        Ok(ProviderIdempotencyKey::new(format!(
            "pi-sdk-payment:{}",
            internal_idempotency_key.as_str()
        )))
    }

    async fn verify_receipt_impl(
        &self,
        _cmd: VerifyReceiptCmd,
    ) -> Result<ReceiptVerification, BackendError> {
        unreachable!("boundary test does not perform provider receipt verification")
    }

    async fn submit_action_impl(
        &self,
        _cmd: SubmitActionCmd,
    ) -> Result<SubmissionResult, BackendError> {
        self.submit_action_impl_calls.fetch_add(1, Ordering::SeqCst);
        Err(BackendError::InvalidProviderPayload)
    }

    async fn reconcile_submission_impl(
        &self,
        _cmd: ReconcileSubmissionCmd,
    ) -> Result<ReconcileResult, BackendError> {
        unreachable!("boundary test does not perform provider reconciliation")
    }

    async fn normalize_callback_impl(
        &self,
        _cmd: NormalizeCallbackCmd,
    ) -> Result<Vec<NormalizedObservation>, BackendError> {
        unreachable!("boundary test does not perform callback normalization")
    }
}
