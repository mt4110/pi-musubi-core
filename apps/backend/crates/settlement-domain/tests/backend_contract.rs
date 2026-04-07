use async_trait::async_trait;
use musubi_settlement_domain::{
    BackendCapabilities, BackendDescriptor, BackendError, BackendKey, BackendPin, BackendVersion,
    ExecutionMode, InternalIdempotencyKey, NormalizeCallbackCmd, ProviderFamily,
    ProviderIdempotencyKey, ReceiptVerification, ReconcileResult, ReconcileSubmissionCmd,
    SettlementBackend, SettlementCapability, SubmitActionCmd, VerifyReceiptCmd,
};

#[test]
fn descriptor_supports_explicit_capabilities() {
    let descriptor = test_descriptor();

    assert!(descriptor.supports(SettlementCapability::ReceiptVerify));
    assert!(!descriptor.supports(SettlementCapability::RefundValue));
}

#[test]
fn backend_pin_mismatch_fails_closed() {
    let backend = TestBackend::new();
    let requested = BackendPin::new(BackendKey::new("other"), BackendVersion::new("2026-04"));

    let result = backend.ensure_backend_pin(&requested);

    assert_eq!(
        result,
        Err(BackendError::BackendPinMismatch {
            requested,
            available: test_descriptor().pin(),
        })
    );
}

fn test_descriptor() -> BackendDescriptor {
    BackendDescriptor {
        backend_key: BackendKey::new("pi"),
        backend_version: BackendVersion::new("2026-04"),
        provider_family: ProviderFamily::PiNetwork,
        execution_mode: ExecutionMode::Hybrid,
        capabilities: BackendCapabilities::new(vec![
            SettlementCapability::ReceiptVerify,
            SettlementCapability::HoldValue,
        ]),
    }
}

struct TestBackend {
    descriptor: BackendDescriptor,
}

impl TestBackend {
    fn new() -> Self {
        Self {
            descriptor: test_descriptor(),
        }
    }
}

#[async_trait]
impl SettlementBackend for TestBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn provider_idempotency_key(
        &self,
        internal_idempotency_key: &InternalIdempotencyKey,
    ) -> Result<ProviderIdempotencyKey, BackendError> {
        Ok(ProviderIdempotencyKey::new(
            internal_idempotency_key.as_str(),
        ))
    }

    async fn verify_receipt_impl(
        &self,
        _cmd: VerifyReceiptCmd,
    ) -> Result<ReceiptVerification, BackendError> {
        unreachable!("test backend does not execute async paths")
    }

    async fn submit_action_impl(
        &self,
        _cmd: SubmitActionCmd,
    ) -> Result<musubi_settlement_domain::SubmissionResult, BackendError> {
        unreachable!("test backend does not execute async paths")
    }

    async fn reconcile_submission_impl(
        &self,
        _cmd: ReconcileSubmissionCmd,
    ) -> Result<ReconcileResult, BackendError> {
        unreachable!("test backend does not execute async paths")
    }

    async fn normalize_callback_impl(
        &self,
        _cmd: NormalizeCallbackCmd,
    ) -> Result<Vec<musubi_settlement_domain::NormalizedObservation>, BackendError> {
        unreachable!("test backend does not execute async paths")
    }
}
