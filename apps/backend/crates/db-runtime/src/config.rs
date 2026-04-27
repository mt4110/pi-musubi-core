use std::{path::PathBuf, time::Duration};

use crate::{DbRuntimeError, Result};

const DEFAULT_DATABASE_MAX_CONNECTIONS: u32 = 16;
const DEFAULT_DATABASE_MIN_CONNECTIONS: u32 = 2;
const DEFAULT_DATABASE_ACQUIRE_TIMEOUT_MS: u64 = 3_000;
const DEFAULT_DATABASE_STATEMENT_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_DATABASE_IDLE_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MIGRATIONS_DIR: &str = "./migrations";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppEnvironment {
    Local,
    Test,
    Staging,
    Prod,
}

impl AppEnvironment {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "test" => Ok(Self::Test),
            "staging" => Ok(Self::Staging),
            "prod" | "production" => Ok(Self::Prod),
            _ => Err(DbRuntimeError::InvalidEnv {
                name: "APP_ENV",
                value: value.to_owned(),
                reason: "expected local, test, staging, or prod",
            }),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Test => "test",
            Self::Staging => "staging",
            Self::Prod => "prod",
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DbPoolConfig {
    pub min_connections: u32,
    pub max_connections: u32,
    pub acquire_timeout: Duration,
    pub statement_timeout: Duration,
    pub idle_timeout: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DbConfig {
    pub app_env: AppEnvironment,
    pub database_url: String,
    pub pool: DbPoolConfig,
    pub require_latest_schema: bool,
    pub migrations_dir: PathBuf,
}

impl DbConfig {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub fn from_lookup(mut lookup: impl FnMut(&'static str) -> Option<String>) -> Result<Self> {
        let app_env =
            required("APP_ENV", &mut lookup).and_then(|value| AppEnvironment::parse(&value))?;
        let database_url = required("DATABASE_URL", &mut lookup)?;
        let max_connections = optional_u32(
            "DATABASE_MAX_CONNECTIONS",
            DEFAULT_DATABASE_MAX_CONNECTIONS,
            &mut lookup,
        )?;
        let min_connections = optional_u32(
            "DATABASE_MIN_CONNECTIONS",
            DEFAULT_DATABASE_MIN_CONNECTIONS,
            &mut lookup,
        )?;
        if min_connections > max_connections {
            return Err(DbRuntimeError::InvalidEnv {
                name: "DATABASE_MIN_CONNECTIONS",
                value: min_connections.to_string(),
                reason: "must be less than or equal to DATABASE_MAX_CONNECTIONS",
            });
        }

        let acquire_timeout = optional_duration_ms(
            "DATABASE_ACQUIRE_TIMEOUT_MS",
            DEFAULT_DATABASE_ACQUIRE_TIMEOUT_MS,
            &mut lookup,
        )?;
        let statement_timeout = optional_duration_ms(
            "DATABASE_STATEMENT_TIMEOUT_MS",
            DEFAULT_DATABASE_STATEMENT_TIMEOUT_MS,
            &mut lookup,
        )?;
        let idle_timeout = optional_duration_ms(
            "DATABASE_IDLE_TIMEOUT_MS",
            DEFAULT_DATABASE_IDLE_TIMEOUT_MS,
            &mut lookup,
        )?;
        let require_latest_schema = optional_bool("REQUIRE_LATEST_SCHEMA", true, &mut lookup)?;
        let migrations_dir = lookup("MIGRATIONS_DIR")
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_MIGRATIONS_DIR));

        Ok(Self {
            app_env,
            database_url,
            pool: DbPoolConfig {
                min_connections,
                max_connections,
                acquire_timeout,
                statement_timeout,
                idle_timeout,
            },
            require_latest_schema,
            migrations_dir,
        })
    }
}

fn required(
    name: &'static str,
    lookup: &mut impl FnMut(&'static str) -> Option<String>,
) -> Result<String> {
    lookup(name)
        .filter(|value| !value.trim().is_empty())
        .ok_or(DbRuntimeError::MissingEnv { name })
}

fn optional_u32(
    name: &'static str,
    default_value: u32,
    lookup: &mut impl FnMut(&'static str) -> Option<String>,
) -> Result<u32> {
    let Some(value) = lookup(name).filter(|value| !value.trim().is_empty()) else {
        return Ok(default_value);
    };

    value
        .parse::<u32>()
        .map_err(|_| DbRuntimeError::InvalidEnv {
            name,
            value,
            reason: "expected an unsigned integer",
        })
}

fn optional_duration_ms(
    name: &'static str,
    default_value_ms: u64,
    lookup: &mut impl FnMut(&'static str) -> Option<String>,
) -> Result<Duration> {
    let Some(value) = lookup(name).filter(|value| !value.trim().is_empty()) else {
        return Ok(Duration::from_millis(default_value_ms));
    };

    let millis = value
        .parse::<u64>()
        .map_err(|_| DbRuntimeError::InvalidEnv {
            name,
            value,
            reason: "expected milliseconds as an unsigned integer",
        })?;
    Ok(Duration::from_millis(millis))
}

fn optional_bool(
    name: &'static str,
    default_value: bool,
    lookup: &mut impl FnMut(&'static str) -> Option<String>,
) -> Result<bool> {
    let Some(value) = lookup(name).filter(|value| !value.trim().is_empty()) else {
        return Ok(default_value);
    };

    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Ok(true),
        "0" | "false" | "no" => Ok(false),
        _ => Err(DbRuntimeError::InvalidEnv {
            name,
            value,
            reason: "expected true or false",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(values: &[(&'static str, &'static str)], name: &'static str) -> Option<String> {
        values
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_owned())
    }

    #[test]
    fn config_defaults_are_conservative() {
        let values = [
            ("APP_ENV", "local"),
            (
                "DATABASE_URL",
                "postgres://musubi:musubi@127.0.0.1:55432/musubi_dev",
            ),
        ];
        let config = DbConfig::from_lookup(|name| lookup(&values, name)).unwrap();

        assert_eq!(config.app_env, AppEnvironment::Local);
        assert_eq!(config.pool.min_connections, 2);
        assert_eq!(config.pool.max_connections, 16);
        assert_eq!(config.pool.acquire_timeout, Duration::from_millis(3_000));
        assert_eq!(config.pool.statement_timeout, Duration::from_millis(5_000));
        assert_eq!(config.pool.idle_timeout, Duration::from_millis(30_000));
        assert!(config.require_latest_schema);
        assert_eq!(config.migrations_dir, PathBuf::from("./migrations"));
    }

    #[test]
    fn config_rejects_min_greater_than_max() {
        let values = [
            ("APP_ENV", "local"),
            (
                "DATABASE_URL",
                "postgres://musubi:musubi@127.0.0.1:55432/musubi_dev",
            ),
            ("DATABASE_MIN_CONNECTIONS", "17"),
            ("DATABASE_MAX_CONNECTIONS", "16"),
        ];

        assert!(matches!(
            DbConfig::from_lookup(|name| lookup(&values, name)),
            Err(DbRuntimeError::InvalidEnv {
                name: "DATABASE_MIN_CONNECTIONS",
                ..
            })
        ));
    }

    #[test]
    fn config_requires_app_env_and_database_url() {
        assert!(matches!(
            DbConfig::from_lookup(|_| None),
            Err(DbRuntimeError::MissingEnv { name: "APP_ENV" })
        ));
    }
}
