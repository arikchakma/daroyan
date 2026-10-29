use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::big_integer;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("ingest_tokens")
                    .add_column(big_integer("dropped_log_records").default(0).take())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("ingest_tokens")
                    .drop_column("dropped_log_records")
                    .to_owned(),
            )
            .await
    }
}
