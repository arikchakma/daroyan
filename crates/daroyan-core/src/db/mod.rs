#[allow(clippy::all, clippy::pedantic, unused_qualifications)]
pub mod entities;
pub mod ids;
pub mod queries;

use std::time::Duration;

use sea_orm::sqlx::sqlite::{SqliteJournalMode, SqliteSynchronous};
use sea_orm::{ConnectOptions, DatabaseConnection, DbErr};
use tracing::log::LevelFilter;

use crate::config::Config;

#[derive(Clone, Debug)]
pub struct Database {
    pub conn: DatabaseConnection,
}

pub async fn connect_database(config: &Config) -> Result<Database, DbErr> {
    let mut options = ConnectOptions::new(&config.database_url);
    options
        .max_connections(8)
        .sqlx_logging_level(LevelFilter::Debug)
        .sqlx_slow_statements_logging_settings(LevelFilter::Warn, Duration::from_secs(1))
        .map_sqlx_sqlite_opts(|options| {
            options
                .journal_mode(SqliteJournalMode::Wal)
                .synchronous(SqliteSynchronous::Normal)
                .busy_timeout(Duration::from_secs(5))
                .foreign_keys(true)
        });
    let conn = sea_orm::Database::connect(options).await?;
    Ok(Database { conn })
}
