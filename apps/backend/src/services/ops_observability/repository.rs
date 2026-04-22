use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use chrono::Utc;
use musubi_db_runtime::{DbConfig, MigrationRunner, connect_writer};
use serde_json::json;
use tokio::sync::Mutex;
use tokio_postgres::Client;

use super::types::{
    MigrationReadinessSnapshot, ObservabilityBoundarySnapshot, OperatorReviewQueueSummary,
    OpsHealthSnapshot, OpsObservabilityError, OpsObservabilitySnapshot, OpsReadinessSnapshot,
    OpsSliMetric, OrchestrationBacklogSummary, ProjectionLagSummary, RealmReviewTriggerSummary,
    postgres_error_is_retryable,
};

const SERVICE_NAME: &str = "musubi-backend";
const PROJECTION_LAG_WARNING_MS: i64 = 60_000;
const PROJECTION_LAG_CRITICAL_MS: i64 = 1_800_000;
const REVIEW_QUEUE_OLDEST_WARNING_MS: i64 = 86_400_000;
const REVIEW_QUEUE_OLDEST_CRITICAL_MS: i64 = 259_200_000;
const REALM_REVIEW_TRIGGER_OLDEST_WARNING_MS: i64 = 86_400_000;
const REALM_REVIEW_TRIGGER_OLDEST_CRITICAL_MS: i64 = 259_200_000;
const ORCHESTRATION_BACKLOG_OLDEST_WARNING_MS: i64 = 300_000;
const ORCHESTRATION_BACKLOG_OLDEST_CRITICAL_MS: i64 = 1_800_000;

const PROJECTION_SOURCES: &[ProjectionSource] = &[
    ProjectionSource {
        projection_name: "promise_views",
        relation: "projection.promise_views",
    },
    ProjectionSource {
        projection_name: "settlement_views",
        relation: "projection.settlement_views",
    },
    ProjectionSource {
        projection_name: "trust_snapshots",
        relation: "projection.trust_snapshots",
    },
    ProjectionSource {
        projection_name: "realm_trust_snapshots",
        relation: "projection.realm_trust_snapshots",
    },
    ProjectionSource {
        projection_name: "review_status_views",
        relation: "projection.review_status_views",
    },
    ProjectionSource {
        projection_name: "room_progression_views",
        relation: "projection.room_progression_views",
    },
    ProjectionSource {
        projection_name: "realm_bootstrap_views",
        relation: "projection.realm_bootstrap_views",
    },
    ProjectionSource {
        projection_name: "realm_admission_views",
        relation: "projection.realm_admission_views",
    },
    ProjectionSource {
        projection_name: "realm_review_summaries",
        relation: "projection.realm_review_summaries",
    },
    ProjectionSource {
        projection_name: "projection_meta",
        relation: "projection.projection_meta",
    },
];

#[derive(Clone)]
pub struct OpsObservabilityStore {
    client: Arc<Mutex<Client>>,
    config: DbConfig,
}

#[derive(Clone, Copy)]
struct ProjectionSource {
    projection_name: &'static str,
    relation: &'static str,
}

struct ProjectionRelationMetadata {
    columns_by_relation: HashMap<String, HashSet<String>>,
}

impl ProjectionRelationMetadata {
    fn relation_exists(&self, relation: &str) -> bool {
        self.columns_by_relation
            .get(relation)
            .is_some_and(|columns| !columns.is_empty())
    }

    fn columns_exist(&self, relation: &str, columns: &[&str]) -> bool {
        self.columns_by_relation
            .get(relation)
            .is_some_and(|existing| columns.iter().all(|column| existing.contains(*column)))
    }
}

impl OpsObservabilityStore {
    pub(crate) async fn connect(config: &DbConfig) -> musubi_db_runtime::Result<Self> {
        let client = connect_writer(config, "musubi-backend ops-observability").await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
            config: config.clone(),
        })
    }

    pub(crate) async fn reset_for_test(&self) -> Result<(), OpsObservabilityError> {
        Ok(())
    }

    pub async fn health(&self) -> Result<OpsHealthSnapshot, OpsObservabilityError> {
        let client = self.client.lock().await;
        check_database(&client).await?;
        Ok(OpsHealthSnapshot {
            status: "ok".to_owned(),
            service: SERVICE_NAME.to_owned(),
            checked_at: Utc::now(),
            database: OpsSliMetric::ok("database_connectivity", json!("ok")),
        })
    }

    pub async fn readiness(&self) -> Result<OpsReadinessSnapshot, OpsObservabilityError> {
        let client = self.client.lock().await;
        check_database(&client).await?;
        drop(client);

        let status = MigrationRunner::new(self.config.migrations_dir.clone())
            .status(&self.config)
            .await?;
        let migrations = MigrationReadinessSnapshot {
            status: if status.is_current() {
                "ready".to_owned()
            } else {
                "degraded".to_owned()
            },
            required_latest_schema: self.config.require_latest_schema,
            bootstrap_required: status.bootstrap_required,
            migration_lock_available: status.migration_lock_available,
            applied_count: status.applied.len(),
            pending_count: status.pending.len(),
            failed_count: status.failed.len(),
            unexpected_applied_count: status.unexpected_applied.len(),
            checksum_drift_count: status.checksum_drifts.len(),
        };

        Ok(OpsReadinessSnapshot {
            status: if migrations.status == "ready" {
                "ready".to_owned()
            } else {
                "degraded".to_owned()
            },
            service: SERVICE_NAME.to_owned(),
            checked_at: Utc::now(),
            database: OpsSliMetric::ok("database_connectivity", json!("ok")),
            migrations,
        })
    }

    pub async fn snapshot(&self) -> Result<OpsObservabilitySnapshot, OpsObservabilityError> {
        let client = self.client.lock().await;
        check_database(&client).await?;
        let projection_lag = projection_lag_summaries(&client).await?;
        let operator_review_queue = operator_review_queue_summary(&client).await?;
        let realm_review_triggers = realm_review_trigger_summary(&client).await?;
        let orchestration_backlog = orchestration_backlog_summary(&client).await?;
        let status = aggregate_snapshot_status(
            &projection_lag,
            &operator_review_queue,
            &realm_review_triggers,
            &orchestration_backlog,
        );

        Ok(OpsObservabilitySnapshot {
            status,
            service: SERVICE_NAME.to_owned(),
            generated_at: Utc::now(),
            database: OpsSliMetric::ok("database_connectivity", json!("ok")),
            projection_lag,
            operator_review_queue,
            realm_review_triggers,
            orchestration_backlog,
            unsupported_metrics: unsupported_metrics(),
            boundary: ObservabilityBoundarySnapshot {
                observability_is_business_truth: false,
                projection_lag_is_writer_decision_input: false,
                participant_visible: false,
                raw_evidence_visible: false,
                operator_notes_visible: false,
                source_identifiers_visible: false,
                pii_visible: false,
            },
        })
    }
}

async fn check_database(client: &Client) -> Result<(), OpsObservabilityError> {
    client.query_one("SELECT 1", &[]).await.map_err(db_error)?;
    Ok(())
}

async fn projection_lag_summaries(
    client: &Client,
) -> Result<Vec<ProjectionLagSummary>, OpsObservabilityError> {
    let metadata = projection_relation_metadata(client).await?;
    let mut summaries = Vec::with_capacity(PROJECTION_SOURCES.len());
    for source in PROJECTION_SOURCES {
        summaries.push(projection_lag_summary(client, source, &metadata).await?);
    }
    Ok(summaries)
}

async fn projection_lag_summary(
    client: &Client,
    source: &ProjectionSource,
    metadata: &ProjectionRelationMetadata,
) -> Result<ProjectionLagSummary, OpsObservabilityError> {
    if !metadata.relation_exists(source.relation) {
        return Ok(ProjectionLagSummary {
            projection_name: source.projection_name.to_owned(),
            status: "unknown".to_owned(),
            row_count: None,
            stale_row_count: None,
            max_projection_lag_ms: None,
            latest_source_watermark_at: None,
            latest_projected_at: None,
            reason: Some("projection table is not present in the current schema".to_owned()),
        });
    }
    if !metadata.columns_exist(
        source.relation,
        &["source_watermark_at", "last_projected_at"],
    ) {
        return Ok(ProjectionLagSummary {
            projection_name: source.projection_name.to_owned(),
            status: "unknown".to_owned(),
            row_count: None,
            stale_row_count: None,
            max_projection_lag_ms: None,
            latest_source_watermark_at: None,
            latest_projected_at: None,
            reason: Some("projection freshness columns are not available".to_owned()),
        });
    }

    let has_projection_lag = metadata.columns_exist(source.relation, &["projection_lag_ms"]);
    let lag_expr = if has_projection_lag {
        "projection_lag_ms"
    } else {
        "GREATEST((EXTRACT(EPOCH FROM (last_projected_at - source_watermark_at)) * 1000)::bigint, 0)"
    };
    let query = format!(
        "
        SELECT
            COUNT(*)::bigint AS row_count,
            COUNT(*) FILTER (WHERE {lag_expr} >= $1)::bigint AS stale_row_count,
            MAX({lag_expr})::bigint AS max_projection_lag_ms,
            MAX(source_watermark_at) AS latest_source_watermark_at,
            MAX(last_projected_at) AS latest_projected_at
        FROM {}
        ",
        source.relation
    );
    let row = client
        .query_one(&query, &[&PROJECTION_LAG_WARNING_MS])
        .await
        .map_err(db_error)?;
    let row_count = row.get::<_, i64>("row_count");
    let max_projection_lag_ms = row.get::<_, Option<i64>>("max_projection_lag_ms");

    Ok(ProjectionLagSummary {
        projection_name: source.projection_name.to_owned(),
        status: classify_optional_age(
            row_count,
            max_projection_lag_ms,
            PROJECTION_LAG_WARNING_MS,
            PROJECTION_LAG_CRITICAL_MS,
        )
        .to_owned(),
        row_count: Some(row_count),
        stale_row_count: Some(row.get("stale_row_count")),
        max_projection_lag_ms,
        latest_source_watermark_at: row.get("latest_source_watermark_at"),
        latest_projected_at: row.get("latest_projected_at"),
        reason: None,
    })
}

async fn projection_relation_metadata(
    client: &Client,
) -> Result<ProjectionRelationMetadata, OpsObservabilityError> {
    let relations = PROJECTION_SOURCES
        .iter()
        .map(|source| source.relation)
        .collect::<Vec<_>>();
    let rows = client
        .query(
            "
            WITH requested AS (
                SELECT unnest($1::text[]) AS relation
            )
            SELECT
                requested.relation,
                columns.column_name
            FROM requested
            LEFT JOIN LATERAL (
                SELECT column_name
                FROM information_schema.columns
                WHERE table_schema = split_part(requested.relation, '.', 1)
                  AND table_name = split_part(requested.relation, '.', 2)
            ) columns ON TRUE
            ",
            &[&relations],
        )
        .await
        .map_err(db_error)?;

    let mut columns_by_relation = PROJECTION_SOURCES
        .iter()
        .map(|source| (source.relation.to_owned(), HashSet::new()))
        .collect::<HashMap<_, _>>();
    for row in rows {
        let relation = row.get::<_, String>("relation");
        if let Some(column_name) = row.get::<_, Option<String>>("column_name") {
            columns_by_relation
                .entry(relation)
                .or_default()
                .insert(column_name);
        }
    }

    Ok(ProjectionRelationMetadata {
        columns_by_relation,
    })
}

async fn operator_review_queue_summary(
    client: &Client,
) -> Result<OperatorReviewQueueSummary, OpsObservabilityError> {
    if !relation_exists(client, "dao.review_cases").await? {
        return Ok(OperatorReviewQueueSummary {
            status: "unknown".to_owned(),
            open_case_count: None,
            awaiting_evidence_count: None,
            appealed_case_count: None,
            oldest_opened_at: None,
            reason: Some("operator review tables are not present in the current schema".to_owned()),
        });
    }

    let row = client
        .query_one(
            "
            SELECT
                COUNT(*) FILTER (
                    WHERE review_status IN (
                        'open',
                        'triaged',
                        'under_review',
                        'awaiting_evidence',
                        'appealed'
                    )
                )::bigint AS open_case_count,
                COUNT(*) FILTER (WHERE review_status = 'awaiting_evidence')::bigint
                    AS awaiting_evidence_count,
                COUNT(*) FILTER (WHERE review_status = 'appealed')::bigint AS appealed_case_count,
                MIN(opened_at) FILTER (
                    WHERE review_status IN (
                        'open',
                        'triaged',
                        'under_review',
                        'awaiting_evidence',
                        'appealed'
                    )
                ) AS oldest_opened_at
            FROM dao.review_cases
            ",
            &[],
        )
        .await
        .map_err(db_error)?;

    let open_case_count = row.get::<_, i64>("open_case_count");
    let oldest_opened_at = row.get::<_, Option<chrono::DateTime<Utc>>>("oldest_opened_at");

    Ok(OperatorReviewQueueSummary {
        status: classify_optional_age(
            open_case_count,
            age_ms(oldest_opened_at),
            REVIEW_QUEUE_OLDEST_WARNING_MS,
            REVIEW_QUEUE_OLDEST_CRITICAL_MS,
        )
        .to_owned(),
        open_case_count: Some(open_case_count),
        awaiting_evidence_count: Some(row.get("awaiting_evidence_count")),
        appealed_case_count: Some(row.get("appealed_case_count")),
        oldest_opened_at,
        reason: None,
    })
}

async fn realm_review_trigger_summary(
    client: &Client,
) -> Result<RealmReviewTriggerSummary, OpsObservabilityError> {
    if !relation_exists(client, "dao.realm_review_triggers").await? {
        return Ok(RealmReviewTriggerSummary {
            status: "unknown".to_owned(),
            open_trigger_count: None,
            oldest_open_trigger_at: None,
            latest_redacted_reason_code: None,
            reason: Some(
                "realm review trigger table is not present in the current schema".to_owned(),
            ),
        });
    }

    let row = client
        .query_one(
            "
            SELECT
                COUNT(*) FILTER (WHERE trigger_state = 'open')::bigint AS open_trigger_count,
                MIN(created_at) FILTER (WHERE trigger_state = 'open') AS oldest_open_trigger_at,
                (
                    SELECT redacted_reason_code
                    FROM dao.realm_review_triggers
                    WHERE trigger_state = 'open'
                    ORDER BY updated_at DESC, created_at DESC
                    LIMIT 1
                ) AS latest_redacted_reason_code
            FROM dao.realm_review_triggers
            ",
            &[],
        )
        .await
        .map_err(db_error)?;

    let open_trigger_count = row.get::<_, i64>("open_trigger_count");
    let oldest_open_trigger_at =
        row.get::<_, Option<chrono::DateTime<Utc>>>("oldest_open_trigger_at");

    Ok(RealmReviewTriggerSummary {
        status: classify_optional_age(
            open_trigger_count,
            age_ms(oldest_open_trigger_at),
            REALM_REVIEW_TRIGGER_OLDEST_WARNING_MS,
            REALM_REVIEW_TRIGGER_OLDEST_CRITICAL_MS,
        )
        .to_owned(),
        open_trigger_count: Some(open_trigger_count),
        oldest_open_trigger_at,
        latest_redacted_reason_code: row.get("latest_redacted_reason_code"),
        reason: None,
    })
}

async fn orchestration_backlog_summary(
    client: &Client,
) -> Result<OrchestrationBacklogSummary, OpsObservabilityError> {
    let outbox_exists = relation_exists(client, "outbox.events").await?;
    let inbox_exists = relation_exists(client, "outbox.command_inbox").await?;
    if !outbox_exists || !inbox_exists {
        return Ok(OrchestrationBacklogSummary {
            status: "unknown".to_owned(),
            outbox_pending_count: None,
            outbox_processing_count: None,
            outbox_quarantined_count: None,
            inbox_pending_count: None,
            inbox_processing_count: None,
            inbox_quarantined_count: None,
            oldest_available_at: None,
            reason: Some("orchestration outbox/inbox tables are not present".to_owned()),
        });
    }

    let row = client
        .query_one(
            "
            WITH outbox_counts AS (
                SELECT
                    COUNT(*) FILTER (WHERE delivery_status = 'pending')::bigint AS pending_count,
                    COUNT(*) FILTER (WHERE delivery_status = 'processing')::bigint AS processing_count,
                    COUNT(*) FILTER (WHERE delivery_status = 'quarantined')::bigint AS quarantined_count,
                    MIN(available_at) FILTER (
                        WHERE delivery_status IN ('pending', 'processing')
                    ) AS oldest_available_at
                FROM outbox.events
            ),
            inbox_counts AS (
                SELECT
                    COUNT(*) FILTER (WHERE status = 'pending')::bigint AS pending_count,
                    COUNT(*) FILTER (WHERE status = 'processing')::bigint AS processing_count,
                    COUNT(*) FILTER (WHERE status = 'quarantined')::bigint AS quarantined_count,
                    MIN(available_at) FILTER (
                        WHERE status IN ('pending', 'processing')
                    ) AS oldest_available_at
                FROM outbox.command_inbox
            )
            SELECT
                outbox_counts.pending_count AS outbox_pending_count,
                outbox_counts.processing_count AS outbox_processing_count,
                outbox_counts.quarantined_count AS outbox_quarantined_count,
                inbox_counts.pending_count AS inbox_pending_count,
                inbox_counts.processing_count AS inbox_processing_count,
                inbox_counts.quarantined_count AS inbox_quarantined_count,
                (
                    SELECT MIN(value)
                    FROM (
                        VALUES
                            (outbox_counts.oldest_available_at),
                            (inbox_counts.oldest_available_at)
                    ) AS available_times(value)
                ) AS oldest_available_at
            FROM outbox_counts, inbox_counts
            ",
            &[],
        )
        .await
        .map_err(db_error)?;

    let outbox_pending_count = row.get::<_, i64>("outbox_pending_count");
    let outbox_processing_count = row.get::<_, i64>("outbox_processing_count");
    let inbox_pending_count = row.get::<_, i64>("inbox_pending_count");
    let inbox_processing_count = row.get::<_, i64>("inbox_processing_count");
    let active_backlog_count = outbox_pending_count
        + outbox_processing_count
        + inbox_pending_count
        + inbox_processing_count;
    let oldest_available_at = row.get::<_, Option<chrono::DateTime<Utc>>>("oldest_available_at");

    Ok(OrchestrationBacklogSummary {
        status: classify_optional_age(
            active_backlog_count,
            age_ms(oldest_available_at),
            ORCHESTRATION_BACKLOG_OLDEST_WARNING_MS,
            ORCHESTRATION_BACKLOG_OLDEST_CRITICAL_MS,
        )
        .to_owned(),
        outbox_pending_count: Some(outbox_pending_count),
        outbox_processing_count: Some(outbox_processing_count),
        outbox_quarantined_count: Some(row.get("outbox_quarantined_count")),
        inbox_pending_count: Some(inbox_pending_count),
        inbox_processing_count: Some(inbox_processing_count),
        inbox_quarantined_count: Some(row.get("inbox_quarantined_count")),
        oldest_available_at,
        reason: None,
    })
}

fn unsupported_metrics() -> Vec<OpsSliMetric> {
    vec![OpsSliMetric::unknown(
        "idempotency_replay_mismatch_count",
        "replay mismatches are rejected at writer boundaries, but this schema does not persist a dedicated mismatch counter",
    )]
}

fn classify_optional_age(
    row_count: i64,
    age_ms: Option<i64>,
    warning_ms: i64,
    critical_ms: i64,
) -> &'static str {
    if row_count == 0 {
        return "ok";
    }
    let Some(age_ms) = age_ms else {
        return "unknown";
    };
    if age_ms >= critical_ms {
        "critical"
    } else if age_ms >= warning_ms {
        "warning"
    } else {
        "ok"
    }
}

fn age_ms(timestamp: Option<chrono::DateTime<Utc>>) -> Option<i64> {
    timestamp.map(|timestamp| (Utc::now() - timestamp).num_milliseconds().max(0))
}

fn aggregate_snapshot_status(
    projection_lag: &[ProjectionLagSummary],
    operator_review_queue: &OperatorReviewQueueSummary,
    realm_review_triggers: &RealmReviewTriggerSummary,
    orchestration_backlog: &OrchestrationBacklogSummary,
) -> String {
    let mut worst_priority = 0;
    for status in projection_lag
        .iter()
        .map(|summary| summary.status.as_str())
        .chain(std::iter::once(operator_review_queue.status.as_str()))
        .chain(std::iter::once(realm_review_triggers.status.as_str()))
        .chain(std::iter::once(orchestration_backlog.status.as_str()))
    {
        worst_priority = worst_priority.max(status_priority(status));
    }

    match worst_priority {
        3 => "critical",
        2 => "warning",
        1 => "ok",
        _ => "unknown",
    }
    .to_owned()
}

fn status_priority(status: &str) -> i32 {
    match status {
        "critical" => 3,
        "warning" => 2,
        "ok" => 1,
        _ => 0,
    }
}

async fn relation_exists(client: &Client, relation: &str) -> Result<bool, OpsObservabilityError> {
    let row = client
        .query_one("SELECT to_regclass($1) IS NOT NULL AS exists", &[&relation])
        .await
        .map_err(db_error)?;
    Ok(row.get("exists"))
}

fn db_error(error: tokio_postgres::Error) -> OpsObservabilityError {
    let retryable = postgres_error_is_retryable(&error);
    OpsObservabilityError::Database {
        message: error.to_string(),
        retryable,
    }
}
