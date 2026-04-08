use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrchestrationError {
    ReplicaReadForbidden,
    PayloadHashEncodingFailed(String),
    EmptyExternalIdempotencyKey,
    Database(String),
    EventAlreadyExists {
        event_id: Uuid,
    },
    IdempotencyKeyAlreadyExists {
        idempotency_key: Uuid,
    },
    StaleOutboxClaim {
        event_id: Uuid,
    },
    ConflictingCommandEnvelope {
        consumer_name: String,
        command_id: Uuid,
    },
    StaleCommandClaim {
        consumer_name: String,
        command_id: Uuid,
    },
    OutboxMessageNotFound {
        event_id: Uuid,
    },
    CommandInboxNotFound {
        consumer_name: String,
        command_id: Uuid,
    },
}
