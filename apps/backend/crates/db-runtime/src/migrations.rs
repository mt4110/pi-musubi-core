use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use tokio_postgres::{Client, Row};

use crate::{DbConfig, DbRuntimeError, Result, connect_writer};

const MIGRATION_LOCK_KEY: i64 = 411_000_008;
const TRACKING_TABLE: &str = "public.musubi_schema_migrations";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedMigration {
    pub migration_id: String,
    pub checksum: String,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChecksumDrift {
    pub migration_id: String,
    pub applied_checksum: String,
    pub local_checksum: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationStatusReport {
    pub bootstrap_required: bool,
    pub migration_lock_available: bool,
    pub applied: Vec<AppliedMigration>,
    pub unexpected_applied: Vec<AppliedMigration>,
    pub pending: Vec<String>,
    pub failed: Vec<AppliedMigration>,
    pub checksum_drifts: Vec<ChecksumDrift>,
}

impl MigrationStatusReport {
    pub fn is_current(&self) -> bool {
        !self.bootstrap_required
            && self.migration_lock_available
            && self.unexpected_applied.is_empty()
            && self.pending.is_empty()
            && self.failed.is_empty()
            && self.checksum_drifts.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapOutcome {
    pub tracking_table: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationOutcome {
    pub applied: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupCheck {
    pub required_latest_schema: bool,
    pub status: MigrationStatusReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalResetConfirmation {
    Confirmed,
    Missing,
}

#[derive(Clone, Debug)]
pub struct MigrationRunner {
    migrations_dir: PathBuf,
    applied_by: String,
}

#[derive(Clone, Debug)]
struct LocalMigration {
    migration_id: String,
    checksum: String,
    sql: String,
}

impl MigrationRunner {
    pub fn new(migrations_dir: impl Into<PathBuf>) -> Self {
        Self {
            migrations_dir: migrations_dir.into(),
            applied_by: "musubi-ops".to_owned(),
        }
    }

    pub async fn bootstrap(&self, config: &DbConfig) -> Result<BootstrapOutcome> {
        let client = connect_writer(config, "musubi-ops db bootstrap").await?;
        bootstrap_tracking(&client).await?;
        Ok(BootstrapOutcome {
            tracking_table: TRACKING_TABLE,
        })
    }

    pub async fn status(&self, config: &DbConfig) -> Result<MigrationStatusReport> {
        let client = connect_writer(config, "musubi-ops db status").await?;
        self.status_with_client(&client, true).await
    }

    pub async fn status_without_lock_probe(
        &self,
        config: &DbConfig,
    ) -> Result<MigrationStatusReport> {
        let client = connect_writer(config, "musubi-ops db status").await?;
        self.status_with_client(&client, false).await
    }

    pub async fn migrate(&self, config: &DbConfig) -> Result<MigrationOutcome> {
        let mut client = connect_writer(config, "musubi-ops db migrate").await?;
        bootstrap_tracking(&client).await?;

        if !try_advisory_lock(&client).await? {
            return Err(DbRuntimeError::MigrationLockUnavailable);
        }

        let result = self.migrate_locked(&mut client).await;
        let release_result = release_advisory_lock(&client).await;

        match (result, release_result) {
            (Ok(outcome), Ok(())) => Ok(outcome),
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(error)) | (Err(_), Err(error)) => Err(error),
        }
    }

    pub async fn verify_startup(&self, config: &DbConfig) -> Result<StartupCheck> {
        let client = connect_writer(config, "musubi-backend startup").await?;
        let status = self.status_with_client(&client, true).await?;
        if config.require_latest_schema {
            ensure_current(&status)?;
        }

        Ok(StartupCheck {
            required_latest_schema: config.require_latest_schema,
            status,
        })
    }

    pub async fn reset_local(
        &self,
        config: &DbConfig,
        confirmation: LocalResetConfirmation,
    ) -> Result<()> {
        ensure_local_reset_allowed(config, confirmation)?;
        let client = connect_writer(config, "musubi-ops db reset-local").await?;

        if !try_advisory_lock(&client).await? {
            return Err(DbRuntimeError::MigrationLockUnavailable);
        }

        let result = client
            .batch_execute(
                "
                DROP SCHEMA IF EXISTS projection CASCADE;
                DROP SCHEMA IF EXISTS outbox CASCADE;
                DROP SCHEMA IF EXISTS ledger CASCADE;
                DROP SCHEMA IF EXISTS dao CASCADE;
                DROP SCHEMA IF EXISTS core CASCADE;
                DROP TABLE IF EXISTS public.musubi_schema_migrations;
                ",
            )
            .await
            .map_err(DbRuntimeError::from);
        let release_result = release_advisory_lock(&client).await;

        match (result, release_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (Ok(()), Err(error)) | (Err(_), Err(error)) => Err(error),
        }
    }

    async fn migrate_locked(&self, client: &mut Client) -> Result<MigrationOutcome> {
        let status = self.status_with_client(client, true).await?;
        ensure_no_unexpected_failed_or_drift(&status)?;

        let local_migrations = self.load_migrations()?;
        let applied_map = status
            .applied
            .iter()
            .filter(|migration| migration.status == "applied")
            .map(|migration| (migration.migration_id.as_str(), migration.checksum.as_str()))
            .collect::<BTreeMap<_, _>>();
        let mut applied = Vec::new();

        for migration in local_migrations {
            if applied_map.contains_key(migration.migration_id.as_str()) {
                continue;
            }

            let transaction = client.transaction().await?;
            if let Err(error) = transaction.batch_execute(&migration.sql).await {
                let message = error.to_string();
                let _ = transaction.rollback().await;
                record_failed_migration(client, &migration, &message, &self.applied_by).await?;
                return Err(DbRuntimeError::MigrationFailed {
                    migration_id: migration.migration_id,
                    message,
                });
            }
            transaction
                .execute(
                    "
                    INSERT INTO public.musubi_schema_migrations (
                        migration_id,
                        checksum,
                        applied_by,
                        status
                    )
                    VALUES ($1, $2, $3, 'applied')
                    ON CONFLICT (migration_id) DO UPDATE
                    SET checksum = EXCLUDED.checksum,
                        applied_at = CURRENT_TIMESTAMP,
                        applied_by = EXCLUDED.applied_by,
                        status = 'applied',
                        error_message = NULL
                    ",
                    &[
                        &migration.migration_id,
                        &migration.checksum,
                        &self.applied_by,
                    ],
                )
                .await?;
            transaction.commit().await?;
            applied.push(migration.migration_id);
        }

        Ok(MigrationOutcome { applied })
    }

    fn load_migrations(&self) -> Result<Vec<LocalMigration>> {
        if !self.migrations_dir.is_dir() {
            return Err(DbRuntimeError::MigrationDirectoryMissing(
                self.migrations_dir.clone(),
            ));
        }

        let mut paths = fs::read_dir(&self.migrations_dir)
            .map_err(|source| DbRuntimeError::Io {
                path: self.migrations_dir.clone(),
                source,
            })?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("sql"))
            .collect::<Vec<_>>();
        paths.sort();

        paths
            .into_iter()
            .map(|path| load_migration_file(&path))
            .collect()
    }

    async fn status_with_client(
        &self,
        client: &Client,
        probe_migration_lock: bool,
    ) -> Result<MigrationStatusReport> {
        let local_migrations = self.load_migrations()?;
        if !tracking_table_exists(client).await? {
            let migration_lock_available = if probe_migration_lock {
                check_advisory_lock_available(client).await?
            } else {
                true
            };
            return Ok(MigrationStatusReport {
                bootstrap_required: true,
                migration_lock_available,
                applied: Vec::new(),
                unexpected_applied: Vec::new(),
                pending: local_migrations
                    .into_iter()
                    .map(|migration| migration.migration_id)
                    .collect(),
                failed: Vec::new(),
                checksum_drifts: Vec::new(),
            });
        }

        let migration_lock_available = if probe_migration_lock {
            check_advisory_lock_available(client).await?
        } else {
            true
        };
        let rows = client
            .query(
                "
                SELECT migration_id, checksum, status, error_message
                FROM public.musubi_schema_migrations
                ORDER BY migration_id
                ",
                &[],
            )
            .await?;
        let applied = rows
            .into_iter()
            .map(map_applied_migration)
            .collect::<Vec<_>>();

        Ok(build_status_report(
            local_migrations,
            applied,
            false,
            migration_lock_available,
        ))
    }
}

fn build_status_report(
    local_migrations: Vec<LocalMigration>,
    applied: Vec<AppliedMigration>,
    bootstrap_required: bool,
    migration_lock_available: bool,
) -> MigrationStatusReport {
    if bootstrap_required {
        return MigrationStatusReport {
            bootstrap_required: true,
            migration_lock_available,
            applied,
            unexpected_applied: Vec::new(),
            pending: local_migrations
                .into_iter()
                .map(|migration| migration.migration_id)
                .collect(),
            failed: Vec::new(),
            checksum_drifts: Vec::new(),
        };
    }

    let local_ids = local_migrations
        .iter()
        .map(|migration| migration.migration_id.as_str())
        .collect::<BTreeSet<_>>();
    let applied_map = applied
        .iter()
        .map(|migration| (migration.migration_id.as_str(), migration))
        .collect::<BTreeMap<_, _>>();
    let mut pending = Vec::new();
    let mut checksum_drifts = Vec::new();
    let unexpected_applied = applied
        .iter()
        .filter(|migration| {
            migration.status == "applied" && !local_ids.contains(migration.migration_id.as_str())
        })
        .cloned()
        .collect();

    for migration in local_migrations {
        match applied_map.get(migration.migration_id.as_str()) {
            Some(applied)
                if applied.status == "applied" && applied.checksum == migration.checksum => {}
            Some(applied) if applied.status == "applied" => {
                checksum_drifts.push(ChecksumDrift {
                    migration_id: migration.migration_id,
                    applied_checksum: applied.checksum.clone(),
                    local_checksum: migration.checksum,
                });
            }
            Some(_) | None => pending.push(migration.migration_id),
        }
    }

    let failed = applied
        .iter()
        .filter(|migration| migration.status == "failed")
        .cloned()
        .collect();

    MigrationStatusReport {
        bootstrap_required: false,
        migration_lock_available,
        applied,
        unexpected_applied,
        pending,
        failed,
        checksum_drifts,
    }
}

fn ensure_current(status: &MigrationStatusReport) -> Result<()> {
    if status.bootstrap_required {
        return Err(DbRuntimeError::BootstrapRequired);
    }
    if !status.migration_lock_available {
        return Err(DbRuntimeError::MigrationLockUnavailable);
    }
    ensure_no_unexpected_failed_or_drift(status)?;
    if !status.pending.is_empty() {
        return Err(DbRuntimeError::PendingMigrations {
            count: status.pending.len(),
        });
    }
    Ok(())
}

fn ensure_no_unexpected_failed_or_drift(status: &MigrationStatusReport) -> Result<()> {
    if let Some(migration) = status.unexpected_applied.first() {
        return Err(DbRuntimeError::UnexpectedAppliedMigration {
            migration_id: migration.migration_id.clone(),
        });
    }
    if let Some(migration) = status.failed.first() {
        return Err(DbRuntimeError::FailedMigrationPresent {
            migration_id: migration.migration_id.clone(),
            message: migration.error_message.clone(),
        });
    }
    if let Some(drift) = status.checksum_drifts.first() {
        return Err(DbRuntimeError::ChecksumDrift {
            migration_id: drift.migration_id.clone(),
            applied_checksum: drift.applied_checksum.clone(),
            local_checksum: drift.local_checksum.clone(),
        });
    }
    Ok(())
}

async fn bootstrap_tracking(client: &Client) -> Result<()> {
    client
        .batch_execute(
            "
            CREATE EXTENSION IF NOT EXISTS pgcrypto;

            CREATE TABLE IF NOT EXISTS public.musubi_schema_migrations (
                migration_id TEXT PRIMARY KEY,
                checksum TEXT NOT NULL,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                applied_by TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('applied', 'failed')),
                error_message TEXT
            );
            ",
        )
        .await?;
    Ok(())
}

async fn tracking_table_exists(client: &Client) -> Result<bool> {
    let exists = client
        .query_one(
            "SELECT to_regclass('public.musubi_schema_migrations') IS NOT NULL AS exists",
            &[],
        )
        .await?
        .get::<_, bool>("exists");
    Ok(exists)
}

async fn try_advisory_lock(client: &Client) -> Result<bool> {
    Ok(client
        .query_one(
            "SELECT pg_try_advisory_lock($1) AS locked",
            &[&MIGRATION_LOCK_KEY],
        )
        .await?
        .get::<_, bool>("locked"))
}

async fn release_advisory_lock(client: &Client) -> Result<()> {
    client
        .query_one(
            "SELECT pg_advisory_unlock($1) AS unlocked",
            &[&MIGRATION_LOCK_KEY],
        )
        .await?;
    Ok(())
}

async fn check_advisory_lock_available(client: &Client) -> Result<bool> {
    let locked = try_advisory_lock(client).await?;
    if locked {
        release_advisory_lock(client).await?;
    }
    Ok(locked)
}

async fn record_failed_migration(
    client: &Client,
    migration: &LocalMigration,
    message: &str,
    applied_by: &str,
) -> Result<()> {
    client
        .execute(
            "
            INSERT INTO public.musubi_schema_migrations (
                migration_id,
                checksum,
                applied_by,
                status,
                error_message
            )
            VALUES ($1, $2, $3, 'failed', $4)
            ON CONFLICT (migration_id) DO UPDATE
            SET checksum = EXCLUDED.checksum,
                applied_at = CURRENT_TIMESTAMP,
                applied_by = EXCLUDED.applied_by,
                status = 'failed',
                error_message = EXCLUDED.error_message
            ",
            &[
                &migration.migration_id,
                &migration.checksum,
                &applied_by,
                &message,
            ],
        )
        .await?;
    Ok(())
}

fn load_migration_file(path: &Path) -> Result<LocalMigration> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| DbRuntimeError::InvalidMigrationFileName(path.display().to_string()))?;
    validate_migration_file_name(file_name)?;
    let sql = fs::read_to_string(path).map_err(|source| DbRuntimeError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let migration_id = file_name.trim_end_matches(".sql").to_owned();
    let checksum = checksum_hex(sql.as_bytes());

    Ok(LocalMigration {
        migration_id,
        checksum,
        sql,
    })
}

fn validate_migration_file_name(file_name: &str) -> Result<()> {
    let Some(stem) = file_name.strip_suffix(".sql") else {
        return Err(DbRuntimeError::InvalidMigrationFileName(
            file_name.to_owned(),
        ));
    };
    let Some((prefix, _rest)) = stem.split_once('_') else {
        return Err(DbRuntimeError::InvalidMigrationFileName(
            file_name.to_owned(),
        ));
    };
    if prefix.len() < 4 || !prefix.chars().all(|value| value.is_ascii_digit()) {
        return Err(DbRuntimeError::InvalidMigrationFileName(
            file_name.to_owned(),
        ));
    }
    Ok(())
}

fn checksum_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn map_applied_migration(row: Row) -> AppliedMigration {
    AppliedMigration {
        migration_id: row.get("migration_id"),
        checksum: row.get("checksum"),
        status: row.get("status"),
        error_message: row.get("error_message"),
    }
}

fn ensure_local_reset_allowed(
    config: &DbConfig,
    confirmation: LocalResetConfirmation,
) -> Result<()> {
    if !config.app_env.is_local() {
        return Err(DbRuntimeError::ResetNotLocal {
            reason: "APP_ENV must be local",
        });
    }
    if !database_url_points_to_local_host(&config.database_url) {
        return Err(DbRuntimeError::ResetNotLocal {
            reason: "DATABASE_URL host must be localhost, 127.0.0.1, ::1, or postgres",
        });
    }
    if confirmation != LocalResetConfirmation::Confirmed {
        return Err(DbRuntimeError::ResetConfirmationRequired);
    }
    Ok(())
}

fn database_url_points_to_local_host(database_url: &str) -> bool {
    let Some(host) = database_host(database_url) else {
        return false;
    };
    matches!(
        host.as_str(),
        "localhost" | "127.0.0.1" | "::1" | "postgres"
    )
}

fn database_host(database_url: &str) -> Option<String> {
    let after_authority = database_url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(database_url);
    let authority = after_authority.split('/').next()?;
    let host_port = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    if let Some(stripped) = host_port.strip_prefix('[') {
        return stripped.split_once(']').map(|(host, _)| host.to_owned());
    }
    Some(host_port.split(':').next()?.to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{AppEnvironment, DbPoolConfig};

    fn local_config(database_url: &str) -> DbConfig {
        DbConfig {
            app_env: AppEnvironment::Local,
            database_url: database_url.to_owned(),
            pool: DbPoolConfig {
                min_connections: 2,
                max_connections: 16,
                acquire_timeout: std::time::Duration::from_millis(3_000),
                statement_timeout: std::time::Duration::from_millis(5_000),
                idle_timeout: std::time::Duration::from_millis(30_000),
            },
            require_latest_schema: true,
            migrations_dir: PathBuf::from("./migrations"),
        }
    }

    fn local_migration(migration_id: &str, checksum: &str) -> LocalMigration {
        LocalMigration {
            migration_id: migration_id.to_owned(),
            checksum: checksum.to_owned(),
            sql: String::new(),
        }
    }

    fn applied_migration(migration_id: &str, checksum: &str) -> AppliedMigration {
        AppliedMigration {
            migration_id: migration_id.to_owned(),
            checksum: checksum.to_owned(),
            status: "applied".to_owned(),
            error_message: None,
        }
    }

    #[test]
    fn migration_file_names_must_be_orderable() {
        assert!(validate_migration_file_name("0001_create_core_schema.sql").is_ok());
        assert!(validate_migration_file_name("202604120001_add_runtime.sql").is_ok());
        assert!(validate_migration_file_name("create_core_schema.sql").is_err());
        assert!(validate_migration_file_name("0001.sql").is_err());
    }

    #[test]
    fn local_reset_requires_local_env_local_host_and_confirmation() {
        let config = local_config("postgres://musubi:musubi@127.0.0.1:55432/musubi_dev");
        assert!(ensure_local_reset_allowed(&config, LocalResetConfirmation::Confirmed).is_ok());

        let config = local_config("postgres://musubi:musubi@db.example.com:5432/musubi_dev");
        assert!(matches!(
            ensure_local_reset_allowed(&config, LocalResetConfirmation::Confirmed),
            Err(DbRuntimeError::ResetNotLocal { .. })
        ));

        let config = local_config("postgres://musubi:musubi@localhost:55432/musubi_dev");
        assert!(matches!(
            ensure_local_reset_allowed(&config, LocalResetConfirmation::Missing),
            Err(DbRuntimeError::ResetConfirmationRequired)
        ));
    }

    #[test]
    fn startup_status_requires_all_migrations_current() {
        let status = MigrationStatusReport {
            bootstrap_required: false,
            migration_lock_available: true,
            applied: Vec::new(),
            unexpected_applied: Vec::new(),
            pending: vec!["0001_create_core_schema".to_owned()],
            failed: Vec::new(),
            checksum_drifts: Vec::new(),
        };

        assert!(matches!(
            ensure_current(&status),
            Err(DbRuntimeError::PendingMigrations { count: 1 })
        ));
    }

    #[test]
    fn status_reports_db_applied_migrations_missing_from_local_files() {
        let status = build_status_report(
            vec![local_migration("0001_create_core_schema", "local-0001")],
            vec![
                applied_migration("0001_create_core_schema", "local-0001"),
                applied_migration("0002_removed_from_checkout", "db-0002"),
            ],
            false,
            true,
        );

        assert_eq!(
            status
                .unexpected_applied
                .iter()
                .map(|migration| migration.migration_id.as_str())
                .collect::<Vec<_>>(),
            vec!["0002_removed_from_checkout"]
        );
        assert!(!status.is_current());
        assert!(matches!(
            ensure_current(&status),
            Err(DbRuntimeError::UnexpectedAppliedMigration { migration_id })
                if migration_id == "0002_removed_from_checkout"
        ));
    }
}
