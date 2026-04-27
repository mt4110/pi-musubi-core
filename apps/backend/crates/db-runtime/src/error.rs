use std::{fmt, io, path::PathBuf};

pub type Result<T> = std::result::Result<T, DbRuntimeError>;

#[derive(Debug)]
pub enum DbRuntimeError {
    MissingEnv {
        name: &'static str,
    },
    InvalidEnv {
        name: &'static str,
        value: String,
        reason: &'static str,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
    AcquireTimeout,
    Database(tokio_postgres::Error),
    MigrationDirectoryMissing(PathBuf),
    InvalidMigrationFileName(String),
    MigrationLockUnavailable,
    MigrationFailed {
        migration_id: String,
        message: String,
    },
    FailedMigrationPresent {
        migration_id: String,
        message: Option<String>,
    },
    ChecksumDrift {
        migration_id: String,
        applied_checksum: String,
        local_checksum: String,
    },
    UnexpectedAppliedMigration {
        migration_id: String,
    },
    BootstrapRequired,
    PendingMigrations {
        count: usize,
    },
    ResetNotLocal {
        reason: &'static str,
    },
    ResetConfirmationRequired,
}

impl fmt::Display for DbRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEnv { name } => write!(f, "missing required environment variable {name}"),
            Self::InvalidEnv {
                name,
                value,
                reason,
            } => write!(f, "invalid value for {name}={value:?}: {reason}"),
            Self::Io { path, source } => write!(f, "failed to read {}: {source}", path.display()),
            Self::AcquireTimeout => write!(f, "timed out acquiring writer database connection"),
            Self::Database(source) => write!(f, "database error: {source}"),
            Self::MigrationDirectoryMissing(path) => {
                write!(f, "migration directory does not exist: {}", path.display())
            }
            Self::InvalidMigrationFileName(name) => {
                write!(f, "invalid migration file name: {name}")
            }
            Self::MigrationLockUnavailable => write!(f, "migration lock is held by another runner"),
            Self::MigrationFailed {
                migration_id,
                message,
            } => write!(f, "migration {migration_id} failed: {message}"),
            Self::FailedMigrationPresent {
                migration_id,
                message,
            } => write!(
                f,
                "migration {migration_id} has a recorded failed status{}",
                message
                    .as_ref()
                    .map(|value| format!(": {value}"))
                    .unwrap_or_default()
            ),
            Self::ChecksumDrift {
                migration_id,
                applied_checksum,
                local_checksum,
            } => write!(
                f,
                "checksum drift for {migration_id}: applied={applied_checksum}, local={local_checksum}"
            ),
            Self::UnexpectedAppliedMigration { migration_id } => write!(
                f,
                "database has applied migration {migration_id}, but the local codebase does not"
            ),
            Self::BootstrapRequired => {
                write!(f, "migration tracking table is missing; run db bootstrap")
            }
            Self::PendingMigrations { count } => {
                write!(f, "{count} pending migration(s); run db migrate")
            }
            Self::ResetNotLocal { reason } => write!(f, "refusing local reset: {reason}"),
            Self::ResetConfirmationRequired => write!(
                f,
                "refusing local reset without --confirm-reset-local or MUSUBI_CONFIRM_RESET_LOCAL=reset-local"
            ),
        }
    }
}

impl std::error::Error for DbRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Database(source) => Some(source),
            _ => None,
        }
    }
}

impl From<tokio_postgres::Error> for DbRuntimeError {
    fn from(source: tokio_postgres::Error) -> Self {
        Self::Database(source)
    }
}
