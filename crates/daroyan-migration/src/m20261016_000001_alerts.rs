use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{
    boolean, integer, json, text, text_null, timestamp, timestamp_null,
};

use crate::parts::{
    create_indexes, create_tables, drop_tables, make_created_at, make_id, make_index,
    make_updated_at,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_tables(
            manager,
            vec![
                create_integrations(),
                create_alert_rules(),
                create_alerts(),
                create_alert_deliveries(),
            ],
        )
        .await?;
        create_indexes(manager, list_indexes()).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_tables(
            manager,
            &["alert_deliveries", "alerts", "alert_rules", "integrations"],
        )
        .await
    }
}

fn create_integrations() -> TableCreateStatement {
    Table::create()
        .table("integrations")
        .col(make_id())
        .col(text("name"))
        .col(text("kind"))
        .col(json("config"))
        .col(text_null("last_status"))
        .col(text_null("last_error"))
        .col(timestamp_null("last_sent_at"))
        .col(make_created_at())
        .col(make_updated_at())
        .to_owned()
}

fn create_alert_rules() -> TableCreateStatement {
    Table::create()
        .table("alert_rules")
        .col(make_id())
        .col(text("name"))
        .col(boolean("enabled"))
        .col(json("condition"))
        .col(integer("for_secs"))
        .col(json("scope"))
        .col(json("integration_ids"))
        .col(timestamp_null("last_fired_at"))
        .col(make_created_at())
        .col(make_updated_at())
        .to_owned()
}

fn create_alerts() -> TableCreateStatement {
    Table::create()
        .table("alerts")
        .col(make_id())
        .col(text("rule_id"))
        .col(text("server_id"))
        .col(text("subject"))
        .col(text("value"))
        .col(text("status"))
        .col(timestamp("started_at"))
        .col(timestamp_null("resolved_at"))
        .foreign_key(
            ForeignKey::create()
                .from("alerts", "rule_id")
                .to("alert_rules", "id")
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .from("alerts", "server_id")
                .to("servers", "id")
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn create_alert_deliveries() -> TableCreateStatement {
    Table::create()
        .table("alert_deliveries")
        .col(make_id())
        .col(text("rule_id"))
        .col(text_null("alert_id"))
        .col(text("integration_id"))
        .col(text("integration_name"))
        .col(text("message"))
        .col(text("status"))
        .col(text_null("error"))
        .col(timestamp("sent_at"))
        .foreign_key(
            ForeignKey::create()
                .from("alert_deliveries", "rule_id")
                .to("alert_rules", "id")
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn list_indexes() -> Vec<IndexCreateStatement> {
    vec![
        make_index("alerts_status_started_at_idx", "alerts", &["status", "started_at"]),
        make_index("alerts_rule_id_idx", "alerts", &["rule_id"]),
        make_index("alerts_server_id_idx", "alerts", &["server_id"]),
        make_index(
            "alert_deliveries_rule_id_sent_at_idx",
            "alert_deliveries",
            &["rule_id", "sent_at"],
        ),
    ]
}
