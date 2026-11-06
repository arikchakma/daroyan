use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::timestamp_null;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("servers")
                    .add_column(timestamp_null("starred_at"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter().table("servers").drop_column("starred_at").to_owned(),
            )
            .await
    }
}
