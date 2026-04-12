use musubi_db_runtime::{DbConfig, LocalResetConfirmation, MigrationRunner, MigrationStatusReport};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), OpsError> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = parse_db_command(&args).inspect_err(|_| print_usage())?;

    if command == DbCommand::Help {
        print_usage();
        return Ok(());
    }

    let config = DbConfig::from_env()?;
    let runner = MigrationRunner::new(config.migrations_dir.clone());

    match command {
        DbCommand::Bootstrap => {
            let outcome = runner.bootstrap(&config).await?;
            println!("bootstrap ok: tracking_table={}", outcome.tracking_table);
        }
        DbCommand::Migrate => {
            let outcome = runner.migrate(&config).await?;
            if outcome.applied.is_empty() {
                println!("migrate ok: no pending migrations");
            } else {
                println!("migrate ok: applied {}", outcome.applied.join(", "));
            }
        }
        DbCommand::Status => {
            let status = runner.status(&config).await?;
            print_status(&status);
        }
        DbCommand::ResetLocal { confirmed } => {
            let confirmation = if confirmed
                || std::env::var("MUSUBI_CONFIRM_RESET_LOCAL").ok().as_deref()
                    == Some("reset-local")
            {
                LocalResetConfirmation::Confirmed
            } else {
                LocalResetConfirmation::Missing
            };
            runner.reset_local(&config, confirmation).await?;
            println!("reset-local ok: local schemas and migration tracking removed");
        }
        DbCommand::Help => unreachable!("help exits before loading DB config"),
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DbCommand {
    Bootstrap,
    Migrate,
    Status,
    ResetLocal { confirmed: bool },
    Help,
}

#[derive(Debug, PartialEq, Eq)]
enum CliUsageError {
    MissingDbNamespace,
    MissingDbCommand,
    UnknownDbCommand(String),
    UnexpectedArgument(String),
}

#[derive(Debug)]
enum OpsError {
    Db(musubi_db_runtime::DbRuntimeError),
    Usage(CliUsageError),
}

impl From<musubi_db_runtime::DbRuntimeError> for OpsError {
    fn from(error: musubi_db_runtime::DbRuntimeError) -> Self {
        Self::Db(error)
    }
}

impl From<CliUsageError> for OpsError {
    fn from(error: CliUsageError) -> Self {
        Self::Usage(error)
    }
}

impl std::fmt::Display for OpsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(error) => write!(formatter, "{error}"),
            Self::Usage(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::fmt::Display for CliUsageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDbNamespace => write!(formatter, "expected db command namespace"),
            Self::MissingDbCommand => write!(formatter, "missing db command"),
            Self::UnknownDbCommand(command) => write!(formatter, "unknown db command: {command}"),
            Self::UnexpectedArgument(argument) => {
                write!(formatter, "unexpected argument: {argument}")
            }
        }
    }
}

fn parse_db_command(args: &[String]) -> Result<DbCommand, CliUsageError> {
    if args.first().map(String::as_str) != Some("db") {
        return Err(CliUsageError::MissingDbNamespace);
    }

    let Some(command) = args.get(1).map(String::as_str) else {
        return Err(CliUsageError::MissingDbCommand);
    };

    match command {
        "help" | "--help" if args.len() == 2 => Ok(DbCommand::Help),
        "bootstrap" if args.len() == 2 => Ok(DbCommand::Bootstrap),
        "migrate" if args.len() == 2 => Ok(DbCommand::Migrate),
        "status" if args.len() == 2 => Ok(DbCommand::Status),
        "reset-local" if args.len() == 2 => Ok(DbCommand::ResetLocal { confirmed: false }),
        "reset-local" if args.len() == 3 && args[2] == "--confirm-reset-local" => {
            Ok(DbCommand::ResetLocal { confirmed: true })
        }
        "bootstrap" | "migrate" | "status" | "help" | "--help" | "reset-local" => {
            Err(CliUsageError::UnexpectedArgument(
                args.get(2).cloned().unwrap_or_else(|| command.to_owned()),
            ))
        }
        other => Err(CliUsageError::UnknownDbCommand(other.to_owned())),
    }
}

fn print_status(status: &MigrationStatusReport) {
    println!("db reachable: yes");
    println!("bootstrap required: {}", status.bootstrap_required);
    println!(
        "migration lock available: {}",
        status.migration_lock_available
    );
    println!("applied migrations: {}", status.applied.len());
    println!("unexpected applied: {}", status.unexpected_applied.len());
    println!("pending migrations: {}", status.pending.len());
    println!("failed migrations: {}", status.failed.len());
    println!("checksum drift: {}", status.checksum_drifts.len());

    if !status.unexpected_applied.is_empty() {
        println!("unexpected applied:");
        for migration in &status.unexpected_applied {
            println!("  - {}", migration.migration_id);
        }
    }
    if !status.pending.is_empty() {
        println!("pending:");
        for migration_id in &status.pending {
            println!("  - {migration_id}");
        }
    }
    if !status.failed.is_empty() {
        println!("failed:");
        for migration in &status.failed {
            println!(
                "  - {}{}",
                migration.migration_id,
                migration
                    .error_message
                    .as_ref()
                    .map(|message| format!(": {message}"))
                    .unwrap_or_default()
            );
        }
    }
    if !status.checksum_drifts.is_empty() {
        println!("checksum drift:");
        for drift in &status.checksum_drifts {
            println!(
                "  - {} applied={} local={}",
                drift.migration_id, drift.applied_checksum, drift.local_checksum
            );
        }
    }
}

fn print_usage() {
    eprintln!(
        "usage:
        cargo run -p musubi-ops -- db bootstrap
        cargo run -p musubi-ops -- db migrate
        cargo run -p musubi-ops -- db status
        cargo run -p musubi-ops -- db reset-local --confirm-reset-local"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn db_help_succeeds_while_unknown_or_missing_db_commands_fail() {
        assert_eq!(
            parse_db_command(&args(&["db", "help"])),
            Ok(DbCommand::Help)
        );
        assert_eq!(
            parse_db_command(&args(&["db", "--help"])),
            Ok(DbCommand::Help)
        );
        assert!(matches!(
            parse_db_command(&args(&["db", "migratee"])),
            Err(CliUsageError::UnknownDbCommand(command)) if command == "migratee"
        ));
        assert!(matches!(
            parse_db_command(&args(&["db"])),
            Err(CliUsageError::MissingDbCommand)
        ));
    }
}
