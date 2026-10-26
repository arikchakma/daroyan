use std::error::Error;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use daroyan_migration::duckdb::migrate_duckdb;
use daroyan_migration::{Migrator, MigratorTrait};
use sea_orm_migration::sea_orm::Database;

#[derive(Parser)]
#[command(about = "Run the SQLite and DuckDB migrations")]
struct Args {
    #[arg(
        long,
        env = "DATABASE_URL",
        default_value = "sqlite://data/daroyan.db?mode=rwc"
    )]
    database_url: String,
    #[arg(long, env = "DUCKDB_PATH", default_value = "data/daroyan.duckdb")]
    duckdb_path: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Up {
        #[arg(long)]
        steps: Option<u32>,
    },
    Down {
        #[arg(long, default_value_t = 1)]
        steps: u32,
    },
    Status,
    Fresh,
    Duckdb,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter("info,sqlx=warn")
        .with_target(false)
        .without_time()
        .init();
    let args = Args::parse();
    if let Command::Duckdb = args.command {
        return Ok(migrate_duckdb(&args.duckdb_path)?);
    }

    let db = Database::connect(&args.database_url).await?;
    match args.command {
        Command::Up { steps } => Migrator::up(&db, steps).await?,
        Command::Down { steps } => Migrator::down(&db, Some(steps)).await?,
        Command::Status => Migrator::status(&db).await?,
        Command::Fresh => Migrator::fresh(&db).await?,
        Command::Duckdb => {}
    }
    Ok(())
}
