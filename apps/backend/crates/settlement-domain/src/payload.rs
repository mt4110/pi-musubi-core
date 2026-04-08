use crate::{Money, ProviderCallbackId, ProviderRef, ProviderSubmissionId, ProviderTxHash};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderPayload {
    pub schema: ProviderPayloadSchema,
    pub fields: Vec<ProviderPayloadField>,
}

impl ProviderPayload {
    pub fn new(schema: ProviderPayloadSchema, fields: Vec<ProviderPayloadField>) -> Self {
        Self { schema, fields }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderPayloadSchema {
    pub name: String,
    pub version: u16,
}

impl ProviderPayloadSchema {
    pub fn new(name: impl Into<String>, version: u16) -> Self {
        Self {
            name: name.into(),
            version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderPayloadField {
    pub name: String,
    pub value: ProviderPayloadValue,
}

impl ProviderPayloadField {
    pub fn new(name: impl Into<String>, value: ProviderPayloadValue) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderPayloadValue {
    Text(String),
    Integer(i128),
    Money(Money),
    ProviderRef(ProviderRef),
    ProviderSubmissionId(ProviderSubmissionId),
    ProviderCallbackId(ProviderCallbackId),
    ProviderTxHash(ProviderTxHash),
    Boolean(bool),
}
