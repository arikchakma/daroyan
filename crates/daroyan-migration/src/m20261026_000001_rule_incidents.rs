use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{boolean, json, text_null};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            boolean("declare_incident").default(false).take(),
            text_null("declare_severity"),
            json("declare_hidden_page_ids").default("[]").take(),
        ] {
            manager
                .alter_table(
                    Table::alter().table("alert_rules").add_column(column).to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in ["declare_hidden_page_ids", "declare_severity", "declare_incident"]
        {
            manager
                .alter_table(
                    Table::alter().table("alert_rules").drop_column(column).to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
