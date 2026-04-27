use std::path::PathBuf;

use musubi_db_runtime::{DbConfig, MigrationRunner};

fn lookup(database_url: &str, migrations_dir: &str, name: &'static str) -> Option<String> {
    match name {
        "APP_ENV" => Some("test".to_owned()),
        "DATABASE_URL" => Some(database_url.to_owned()),
        "MIGRATIONS_DIR" => Some(migrations_dir.to_owned()),
        "REQUIRE_LATEST_SCHEMA" => Some("true".to_owned()),
        _ => None,
    }
}

#[tokio::test]
async fn bootstrap_migrate_status_and_startup_check_are_idempotent() {
    let Ok(database_url) = std::env::var("MUSUBI_TEST_DATABASE_URL") else {
        return;
    };
    let migrations_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .canonicalize()
        .expect("migrations directory should resolve");
    let migrations_dir = migrations_dir
        .to_str()
        .expect("migrations directory should be utf-8")
        .to_owned();
    let config = DbConfig::from_lookup(|name| lookup(&database_url, &migrations_dir, name))
        .expect("test db config should parse");
    let runner = MigrationRunner::new(config.migrations_dir.clone());

    runner
        .bootstrap(&config)
        .await
        .expect("bootstrap should create migration tracking");
    runner
        .migrate(&config)
        .await
        .expect("first migrate should apply or no-op");
    let second = runner
        .migrate(&config)
        .await
        .expect("second migrate should be a no-op");
    let status = runner.status(&config).await.expect("status should load");
    let startup = runner
        .verify_startup(&config)
        .await
        .expect("startup check should accept current schema");

    assert!(second.applied.is_empty());
    assert!(status.is_current());
    assert!(startup.required_latest_schema);
    assert!(startup.status.is_current());
}
