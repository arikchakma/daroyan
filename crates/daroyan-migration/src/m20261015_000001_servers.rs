use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{
    json, json_null, text, text_null, timestamp, timestamp_null,
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
        create_tables(
            manager,
            vec![create_servers(), create_install_tokens(), create_commands()],
        )
        .await?;
        create_indexes(manager, list_indexes()).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_tables(manager, &["commands", "install_tokens", "servers"]).await
    }
}

fn create_servers() -> TableCreateStatement {
    Table::create()
        .table("servers")
        .col(make_id())
        .col(text("name"))
        .col(json("labels"))
        .col(json("host"))
        .col(text("token_hash"))
        .col(json_null("last_report"))
        .col(timestamp_null("last_seen_at"))
        .col(make_created_at())
        .col(make_updated_at())
        .to_owned()
}

fn create_install_tokens() -> TableCreateStatement {
    Table::create()
        .table("install_tokens")
        .col(make_id())
        .col(text("token_hash"))
        .col(timestamp("expires_at"))
        .col(make_created_at())
        .to_owned()
}

fn create_commands() -> TableCreateStatement {
    Table::create()
        .table("commands")
        .col(make_id())
        .col(text("server_id"))
        .col(text("action"))
        .col(text("name"))
        .col(text("status"))
        .col(text_null("error"))
        .col(make_created_at())
        .col(make_updated_at())
        .foreign_key(
            ForeignKey::create()
                .from("commands", "server_id")
                .to("servers", "id")
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn list_indexes() -> Vec<IndexCreateStatement> {
    vec![
        make_unique("servers_token_hash_key", "servers", &["token_hash"]),
        make_unique("install_tokens_token_hash_key", "install_tokens", &["token_hash"]),
        make_index(
            "commands_server_id_created_at_idx",
            "commands",
            &["server_id", "created_at"],
        ),
    ]
}
