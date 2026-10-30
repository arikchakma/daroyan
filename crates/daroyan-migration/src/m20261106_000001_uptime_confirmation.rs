use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::integer;

const COLUMNS: [(&str, i32); 3] =
    [("failure_threshold", 3), ("success_threshold", 2), ("retry_interval_secs", 30)];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (column, default) in COLUMNS {
            manager
                .alter_table(
                    Table::alter()
                        .table("uptime_checks")
                        .add_column(integer(column).default(default).take())
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (column, _) in COLUMNS {
            manager
                .alter_table(
                    Table::alter().table("uptime_checks").drop_column(column).to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
