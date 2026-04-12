use tokio_postgres::{Client, NoTls};

use crate::{DbConfig, DbRuntimeError, Result};

pub async fn connect_writer(config: &DbConfig, application_name: &str) -> Result<Client> {
    let (client, connection) = tokio::time::timeout(
        config.pool.acquire_timeout,
        tokio_postgres::connect(&config.database_url, NoTls),
    )
    .await
    .map_err(|_| DbRuntimeError::AcquireTimeout)??;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("writer database connection error: {error}");
        }
    });

    configure_session(&client, config, application_name).await?;
    Ok(client)
}

async fn configure_session(
    client: &Client,
    config: &DbConfig,
    application_name: &str,
) -> Result<()> {
    client
        .execute(
            "SELECT set_config('application_name', $1, false)",
            &[&application_name],
        )
        .await?;
    let statement_timeout = format!("{}ms", config.pool.statement_timeout.as_millis());
    client
        .execute(
            "SELECT set_config('statement_timeout', $1, false)",
            &[&statement_timeout],
        )
        .await?;
    Ok(())
}
