use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{
    boolean, integer, integer_null, text, text_null, timestamp, timestamp_null,
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
        create_tables(manager, vec![create_uptime_checks(), create_uptime_incidents()])
            .await?;
        create_indexes(manager, list_indexes()).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_tables(manager, &["uptime_incidents", "uptime_checks"]).await
    }
}

fn create_uptime_checks() -> TableCreateStatement {
    Table::create()
        .table("uptime_checks")
        .col(make_id())
        .col(text("name"))
        .col(text("kind"))
        .col(text("target"))
        .col(text("method"))
        .col(integer_null("expected_status"))
        .col(text_null("body_contains"))
        .col(integer("timeout_secs"))
        .col(integer("interval_secs"))
        .col(boolean("paused"))
        .col(make_created_at())
        .col(make_updated_at())
        .to_owned()
}

fn create_uptime_incidents() -> TableCreateStatement {
    Table::create()
        .table("uptime_incidents")
        .col(make_id())
        .col(text("check_id"))
        .col(text("kind"))
        .col(text("cause"))
        .col(timestamp("started_at"))
        .col(timestamp_null("resolved_at"))
        .foreign_key(
            ForeignKey::create()
                .from("uptime_incidents", "check_id")
                .to("uptime_checks", "id")
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn list_indexes() -> Vec<IndexCreateStatement> {
    vec![make_index(
        "uptime_incidents_check_id_started_at_idx",
        "uptime_incidents",
        &["check_id", "started_at"],
    )]
}
