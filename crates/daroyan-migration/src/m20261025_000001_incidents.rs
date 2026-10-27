use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{
    boolean, json, json_null, text, text_null, timestamp, timestamp_null,
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
        create_tables(manager, vec![create_incidents(), create_incident_updates()])
            .await?;
        create_indexes(
            manager,
            vec![
                make_index("incidents_resolved_at_idx", "incidents", &["resolved_at"]),
                make_index(
                    "incident_updates_incident_id_created_at_idx",
                    "incident_updates",
                    &["incident_id", "created_at"],
                ),
            ],
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_tables(manager, &["incident_updates", "incidents"]).await
    }
}

fn create_incidents() -> TableCreateStatement {
    Table::create()
        .table("incidents")
        .col(make_id())
        .col(text("kind"))
        .col(text("title"))
        .col(text("status"))
        .col(text_null("severity"))
        .col(text_null("lead_id"))
        .col(timestamp("started_at"))
        .col(timestamp_null("resolved_at"))
        .col(timestamp_null("starts_at"))
        .col(timestamp_null("ends_at"))
        .col(boolean("auto_start").default(true))
        .col(boolean("auto_finish").default(true))
        .col(boolean("remind_before").default(true))
        .col(timestamp_null("reminded_at"))
        .col(json("components"))
        .col(json("related"))
        .col(json("responders"))
        .col(json("channel_ids"))
        .col(json("hidden_page_ids"))
        .col(json("follow_ups"))
        .col(json_null("postmortem"))
        .col(timestamp_null("postmortem_published_at"))
        .col(make_created_at())
        .col(make_updated_at())
        .to_owned()
}

fn create_incident_updates() -> TableCreateStatement {
    Table::create()
        .table("incident_updates")
        .col(make_id())
        .col(text("incident_id"))
        .col(text("kind"))
        .col(text_null("status"))
        .col(json("body"))
        .col(boolean("is_public"))
        .col(text_null("author_id"))
        .col(make_created_at())
        .col(timestamp_null("edited_at"))
        .to_owned()
}
