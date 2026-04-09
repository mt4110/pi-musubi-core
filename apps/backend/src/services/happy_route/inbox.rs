use chrono::{Duration, Utc};

use super::{constants::COMMAND_INBOX_RETENTION_MINUTES, state::HappyRouteState};

pub(super) fn prune_processed_command_inbox(store: &mut HappyRouteState) {
    let cutoff = Utc::now() - Duration::minutes(COMMAND_INBOX_RETENTION_MINUTES);
    store
        .command_inbox_by_key
        .retain(|_, record| match record.processed_at {
            Some(processed_at) => processed_at > cutoff,
            None => true,
        });
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::prune_processed_command_inbox;
    use crate::services::happy_route::state::{CommandInboxRecord, HappyRouteState};

    #[test]
    fn prune_processed_command_inbox_removes_only_expired_processed_rows() {
        let mut store = HappyRouteState::default();
        let now = Utc::now();
        store.command_inbox_by_key.insert(
            ("consumer".to_owned(), "processed-old".to_owned()),
            CommandInboxRecord {
                consumer_name: "consumer".to_owned(),
                source_message_id: "processed-old".to_owned(),
                received_at: now - Duration::minutes(11),
                processed_at: Some(now - Duration::minutes(11)),
            },
        );
        store.command_inbox_by_key.insert(
            ("consumer".to_owned(), "processed-fresh".to_owned()),
            CommandInboxRecord {
                consumer_name: "consumer".to_owned(),
                source_message_id: "processed-fresh".to_owned(),
                received_at: now,
                processed_at: Some(now),
            },
        );
        store.command_inbox_by_key.insert(
            ("consumer".to_owned(), "processing".to_owned()),
            CommandInboxRecord {
                consumer_name: "consumer".to_owned(),
                source_message_id: "processing".to_owned(),
                received_at: now - Duration::minutes(11),
                processed_at: None,
            },
        );

        prune_processed_command_inbox(&mut store);

        assert!(
            !store
                .command_inbox_by_key
                .contains_key(&("consumer".to_owned(), "processed-old".to_owned()))
        );
        assert!(
            store
                .command_inbox_by_key
                .contains_key(&("consumer".to_owned(), "processed-fresh".to_owned()))
        );
        assert!(
            store
                .command_inbox_by_key
                .contains_key(&("consumer".to_owned(), "processing".to_owned()))
        );
    }
}
