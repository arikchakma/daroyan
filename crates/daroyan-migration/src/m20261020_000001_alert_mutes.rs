use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{text, text_null, timestamp, timestamp_null};

use crate::parts::{create_indexes, create_tables, drop_tables, make_id, make_index};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_tables(manager, vec![create_alert_mutes()]).await?;
        create_indexes(
            manager,
            vec![make_index("alert_mutes_rule_id_idx", "alert_mutes", &["rule_id"])],
        )
        .await?;
        manager
            .alter_table(
                Table::alter()
                    .table("alert_rules")
                    .add_column(timestamp_null("quiet_until"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_tables(manager, &["alert_mutes"]).await?;
        manager
            .alter_table(
                Table::alter()
                    .table("alert_rules")
                    .drop_column("quiet_until")
                    .to_owned(),
            )
            .await
    }
}

fn create_alert_mutes() -> TableCreateStatement {
    Table::create()
        .table("alert_mutes")
        .col(make_id())
        .col(text("rule_id"))
        .col(text_null("server_id"))
        .col(text_null("check_id"))
        .col(timestamp_null("ends_at"))
        .col(text_null("reason"))
        .col(text("muted_by"))
        .col(timestamp("created_at"))
        .to_owned()
}
