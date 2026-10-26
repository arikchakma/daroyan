use sea_orm_migration::prelude::*;

use crate::parts::make_index;

const RENAMES: [(&str, &str, &str, &str, &str); 2] = [
    (
        "uptime_incidents",
        "uptime_outages",
        "uptime_incidents_check_id_started_at_idx",
        "uptime_outages_check_id_started_at_idx",
        "check_id",
    ),
    (
        "cron_incidents",
        "cron_failures",
        "cron_incidents_cron_id_started_at_idx",
        "cron_failures_cron_id_started_at_idx",
        "cron_id",
    ),
];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (from, to, from_index, to_index, column) in RENAMES {
            rename(manager, (from, from_index), (to, to_index), column).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (from, to, from_index, to_index, column) in RENAMES {
            rename(manager, (to, to_index), (from, from_index), column).await?;
        }
        Ok(())
    }
}

async fn rename(
    manager: &SchemaManager<'_>,
    (from, from_index): (&'static str, &'static str),
    (to, to_index): (&'static str, &'static str),
    column: &'static str,
) -> Result<(), DbErr> {
    manager
        .drop_index(Index::drop().name(from_index).table(from).to_owned())
        .await?;
    manager
        .rename_table(Table::rename().table(from, to).to_owned())
        .await?;
    manager
        .create_index(make_index(to_index, to, &[column, "started_at"]))
        .await
}
