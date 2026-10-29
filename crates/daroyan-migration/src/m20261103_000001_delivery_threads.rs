use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::text_null;

use crate::parts::{create_indexes, make_index};

const INDEX: &str = "alert_deliveries_alert_id_integration_id_idx";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in
            [text_null("chat_channel_id").take(), text_null("chat_message_id").take()]
        {
            manager
                .alter_table(
                    Table::alter()
                        .table("alert_deliveries")
                        .add_column(column)
                        .to_owned(),
                )
                .await?;
        }

        create_indexes(
            manager,
            vec![make_index(INDEX, "alert_deliveries", &["alert_id", "integration_id"])],
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(Index::drop().name(INDEX).table("alert_deliveries").to_owned())
            .await?;

        for column in ["chat_message_id", "chat_channel_id"] {
            manager
                .alter_table(
                    Table::alter()
                        .table("alert_deliveries")
                        .drop_column(column)
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }
}
