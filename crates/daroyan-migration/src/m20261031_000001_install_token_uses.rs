use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::integer;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in
            [integer("max_uses").default(1).take(), integer("uses").default(0).take()]
        {
            manager
                .alter_table(
                    Table::alter().table("install_tokens").add_column(column).to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in ["uses", "max_uses"] {
            manager
                .alter_table(
                    Table::alter().table("install_tokens").drop_column(column).to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
