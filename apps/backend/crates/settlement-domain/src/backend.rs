use async_trait::async_trait;

use crate::{
    BackendDescriptor, BackendError, NormalizeCallbackCmd, ProviderIdempotencyKey,
    ReceiptVerification, ReconcileResult, ReconcileSubmissionCmd, SubmitActionCmd,
    VerifyReceiptCmd,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BackendKey(String);

impl BackendKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BackendVersion(String);

impl BackendVersion {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BackendPin {
    pub backend_key: BackendKey,
    pub backend_version: BackendVersion,
}

impl BackendPin {
    pub fn new(backend_key: BackendKey, backend_version: BackendVersion) -> Self {
        Self {
            backend_key,
            backend_version,
        }
    }

    pub fn matches_descriptor(&self, descriptor: &BackendDescriptor) -> bool {
        self.backend_key.as_str() == descriptor.backend_key.as_str()
            && self.backend_version.as_str() == descriptor.backend_version.as_str()
    }
}

#[async_trait]
pub trait SettlementBackend: Send + Sync {
    fn descriptor(&self) -> &BackendDescriptor;

    fn supports(&self, capability: crate::SettlementCapability) -> bool {
        self.descriptor().supports(capability)
    }

    fn ensure_backend_pin(&self, backend: &BackendPin) -> Result<(), BackendError> {
        let available = self.descriptor().pin();

        if backend == &available {
            Ok(())
        } else {
            Err(BackendError::BackendPinMismatch {
                requested: backend.clone(),
                available,
            })
        }
    }

    fn provider_idempotency_key(
        &self,
        internal_idempotency_key: &crate::InternalIdempotencyKey,
    ) -> Result<ProviderIdempotencyKey, BackendError>;

    async fn verify_receipt(
        &self,
        cmd: VerifyReceiptCmd,
    ) -> Result<ReceiptVerification, BackendError> {
        self.ensure_backend_pin(&cmd.backend)?;
        self.verify_receipt_impl(cmd).await
    }

    async fn submit_action(
        &self,
        cmd: SubmitActionCmd,
    ) -> Result<crate::SubmissionResult, BackendError> {
        self.ensure_backend_pin(&cmd.backend)?;
        self.submit_action_impl(cmd).await
    }

    async fn reconcile_submission(
        &self,
        cmd: ReconcileSubmissionCmd,
    ) -> Result<ReconcileResult, BackendError> {
        self.ensure_backend_pin(&cmd.backend)?;
        self.reconcile_submission_impl(cmd).await
    }

    async fn normalize_callback(
        &self,
        cmd: NormalizeCallbackCmd,
    ) -> Result<Vec<crate::NormalizedObservation>, BackendError> {
        self.ensure_backend_pin(&cmd.backend)?;
        self.normalize_callback_impl(cmd).await
    }

    async fn verify_receipt_impl(
        &self,
        cmd: VerifyReceiptCmd,
    ) -> Result<ReceiptVerification, BackendError>;

    async fn submit_action_impl(
        &self,
        cmd: SubmitActionCmd,
    ) -> Result<crate::SubmissionResult, BackendError>;

    async fn reconcile_submission_impl(
        &self,
        cmd: ReconcileSubmissionCmd,
    ) -> Result<ReconcileResult, BackendError>;

    async fn normalize_callback_impl(
        &self,
        cmd: NormalizeCallbackCmd,
    ) -> Result<Vec<crate::NormalizedObservation>, BackendError>;
}
