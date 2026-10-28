use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::text_null;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("commands")
                    .add_column(text_null("creator_id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter().table("commands").drop_column("creator_id").to_owned(),
            )
            .await
    }
}
