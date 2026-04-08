use chrono::{DateTime, TimeDelta, Utc};
use uuid::Uuid;

use crate::ProcessingFailure;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryClass {
    Transient,
    Permanent,
    Deferred,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    // Includes the first delivery/handler attempt. With max_attempts = 3,
    // the runtime allows attempts #1, #2, and #3, then quarantines.
    pub max_attempts: u32,
    pub base_delay: TimeDelta,
    pub max_delay: TimeDelta,
    pub max_jitter: TimeDelta,
}

impl RetryPolicy {
    pub fn next_retry_at(
        &self,
        now: DateTime<Utc>,
        correlation_id: Uuid,
        next_attempt_number: u32,
    ) -> DateTime<Utc> {
        let exponent = next_attempt_number.saturating_sub(1).min(30);
        let factor = 2_i64.saturating_pow(exponent);
        let base_seconds = self.base_delay.num_seconds().saturating_mul(factor);
        let capped_seconds = base_seconds.min(self.max_delay.num_seconds());
        let jitter_window = self.max_jitter.num_seconds().max(0);
        let jitter_seconds = if jitter_window == 0 {
            0
        } else {
            (correlation_id.as_u128() % (jitter_window as u128 + 1)) as i64
        };

        let bounded_seconds = capped_seconds
            .saturating_add(jitter_seconds)
            .min(self.max_delay.num_seconds());

        now + TimeDelta::seconds(bounded_seconds)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub published_outbox_for: TimeDelta,
    pub quarantined_outbox_for: TimeDelta,
    pub completed_command_for: TimeDelta,
    pub quarantined_command_for: TimeDelta,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaCompatibilityPolicy {
    pub max_supported_schema_version: i32,
    pub compatibility_window: TimeDelta,
}

impl SchemaCompatibilityPolicy {
    pub fn classify(
        &self,
        schema_version: i32,
        first_seen_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingFailure> {
        if schema_version <= self.max_supported_schema_version {
            return Ok(());
        }

        if now < first_seen_at + self.compatibility_window {
            Err(ProcessingFailure::deferred(
                "unknown_schema_version",
                format!(
                    "schema_version {} is newer than supported version {}",
                    schema_version, self.max_supported_schema_version
                ),
            ))
        } else {
            Err(ProcessingFailure::compatibility_window_expired(
                "unknown_schema_version",
                format!(
                    "schema_version {} remained unsupported beyond the compatibility window",
                    schema_version
                ),
            ))
        }
    }
}
