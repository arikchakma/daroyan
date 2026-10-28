use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{
    integer, json, text, text_null, timestamp, timestamp_null,
};

use crate::parts::{create_indexes, create_tables, drop_tables, make_id, make_index};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    fn use_transaction(&self) -> Option<bool> {
        Some(true)
    }

    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("alert_rules")
                    .add_column(json("options").default("{}"))
                    .to_owned(),
            )
            .await?;
        create_tables(manager, vec![create_alerts()]).await?;
        let copy = Query::insert()
            .into_table("alerts_next")
            .columns([
                "id",
                "rule_id",
                "server_id",
                "subject",
                "value",
                "status",
                "started_at",
                "resolved_at",
                "last_notified_at",
            ])
            .select_from(
                Query::select()
                    .columns([
                        "id",
                        "rule_id",
                        "server_id",
                        "subject",
                        "value",
                        "status",
                        "started_at",
                        "resolved_at",
                        "started_at",
                    ])
                    .from("alerts")
                    .to_owned(),
            )
            .map_err(|error| DbErr::Migration(error.to_string()))?
            .to_owned();
        manager.exec_stmt(copy).await?;
        drop_tables(manager, &["alerts"]).await?;
        manager
            .rename_table(Table::rename().table("alerts_next", "alerts").to_owned())
            .await?;
        create_indexes(
            manager,
            vec![
                make_index(
                    "alerts_status_started_at_idx",
                    "alerts",
                    &["status", "started_at"],
                ),
                make_index("alerts_rule_id_idx", "alerts", &["rule_id"]),
                make_index("alerts_server_id_idx", "alerts", &["server_id"]),
                make_index("alerts_check_id_idx", "alerts", &["check_id"]),
            ],
        )
        .await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Migration("alert levels cannot be rolled back".to_string()))
    }
}

fn create_alerts() -> TableCreateStatement {
    Table::create()
        .table("alerts_next")
        .col(make_id())
        .col(text("rule_id"))
        .col(text_null("server_id"))
        .col(text_null("check_id"))
        .col(text("subject"))
        .col(text("value"))
        .col(text("severity").default("critical"))
        .col(text("status"))
        .col(timestamp("started_at"))
        .col(timestamp_null("resolved_at"))
        .col(timestamp_null("last_notified_at"))
        .col(integer("reminders_sent").default(0))
        .to_owned()
}
