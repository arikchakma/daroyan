pub use sea_orm_migration::prelude::*;

pub mod duckdb;
mod m20261015_000001_servers;
mod m20261016_000001_alerts;
mod m20261017_000001_users;
mod m20261018_000001_uptime;
mod m20261019_000001_alert_levels;
mod m20261020_000001_alert_mutes;
mod m20261021_000001_crons;
mod m20261022_000001_cron_open_runs;
mod m20261023_000001_status_pages;
mod m20261024_000001_outages;
mod m20261025_000001_incidents;
mod m20261026_000001_rule_incidents;
mod m20261027_000001_ingest_tokens;
mod m20261028_000001_mcp_tokens;
mod m20261029_000001_members;
mod m20261030_000001_snake_case;
mod m20261031_000001_install_token_uses;
mod m20261101_000001_install_token_keys;
mod m20261102_000001_drop_foreign_keys;
mod m20261103_000001_delivery_threads;
mod parts;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261015_000001_servers::Migration),
            Box::new(m20261016_000001_alerts::Migration),
            Box::new(m20261017_000001_users::Migration),
            Box::new(m20261018_000001_uptime::Migration),
            Box::new(m20261019_000001_alert_levels::Migration),
            Box::new(m20261020_000001_alert_mutes::Migration),
            Box::new(m20261021_000001_crons::Migration),
            Box::new(m20261022_000001_cron_open_runs::Migration),
            Box::new(m20261023_000001_status_pages::Migration),
            Box::new(m20261024_000001_outages::Migration),
            Box::new(m20261025_000001_incidents::Migration),
            Box::new(m20261026_000001_rule_incidents::Migration),
            Box::new(m20261027_000001_ingest_tokens::Migration),
            Box::new(m20261028_000001_mcp_tokens::Migration),
            Box::new(m20261029_000001_members::Migration),
            Box::new(m20261030_000001_snake_case::Migration),
            Box::new(m20261031_000001_install_token_uses::Migration),
            Box::new(m20261101_000001_install_token_keys::Migration),
            Box::new(m20261102_000001_drop_foreign_keys::Migration),
            Box::new(m20261103_000001_delivery_threads::Migration),
        ]
    }
}
