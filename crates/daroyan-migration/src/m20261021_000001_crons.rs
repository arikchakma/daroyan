use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{
    boolean, integer, integer_null, text, text_null, timestamp, timestamp_null,
};

use crate::parts::{
    create_indexes, create_tables, drop_tables, make_created_at, make_id, make_index,
    make_unique, make_updated_at,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_tables(manager, vec![create_crons(), create_cron_incidents()]).await?;
        let db = manager.get_connection();
        db.execute_unprepared(
            r#"ALTER TABLE "alerts" ADD COLUMN "cron_id" text NULL REFERENCES "crons" ("id") ON DELETE CASCADE"#,
        )
        .await?;
        db.execute_unprepared(
            r#"ALTER TABLE "alert_mutes" ADD COLUMN "cron_id" text NULL REFERENCES "crons" ("id") ON DELETE CASCADE"#,
        )
        .await?;
        create_indexes(manager, list_indexes()).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop().name("alerts_cron_id_idx").table("alerts").to_owned(),
            )
            .await?;
        for table in ["alert_mutes", "alerts"] {
            manager
                .alter_table(
                    Table::alter().table(table).drop_column("cron_id").to_owned(),
                )
                .await?;
        }
        drop_tables(manager, &["cron_incidents", "crons"]).await
    }
}

fn create_crons() -> TableCreateStatement {
    Table::create()
        .table("crons")
        .col(make_id())
        .col(text("name"))
        .col(text("ping_key"))
        .col(text("schedule_kind"))
        .col(text_null("expression"))
        .col(integer_null("interval_secs"))
        .col(text("time_zone"))
        .col(integer("grace_secs"))
        .col(integer_null("max_duration_secs"))
        .col(text_null("server_id"))
        .col(text_null("command"))
        .col(boolean("paused").default(false))
        .col(timestamp_null("last_ping_at"))
        .col(timestamp_null("run_started_at"))
        .col(text_null("run_id"))
        .col(timestamp_null("missed_slot_at"))
        .col(make_created_at())
        .col(make_updated_at())
        .foreign_key(
            ForeignKey::create()
                .from("crons", "server_id")
                .to("servers", "id")
                .on_delete(ForeignKeyAction::SetNull),
        )
        .to_owned()
}

fn create_cron_incidents() -> TableCreateStatement {
    Table::create()
        .table("cron_incidents")
        .col(make_id())
        .col(text("cron_id"))
        .col(text("kind"))
        .col(text("cause"))
        .col(timestamp("started_at"))
        .col(timestamp_null("resolved_at"))
        .foreign_key(
            ForeignKey::create()
                .from("cron_incidents", "cron_id")
                .to("crons", "id")
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn list_indexes() -> Vec<IndexCreateStatement> {
    vec![
        make_unique("crons_ping_key_idx", "crons", &["ping_key"]),
        make_index(
            "cron_incidents_cron_id_started_at_idx",
            "cron_incidents",
            &["cron_id", "started_at"],
        ),
        make_index("alerts_cron_id_idx", "alerts", &["cron_id"]),
    ]
}
