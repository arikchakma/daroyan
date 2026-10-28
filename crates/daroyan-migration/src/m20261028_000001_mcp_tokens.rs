use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{text, timestamp_null};

use crate::parts::{
    create_indexes, create_tables, drop_tables, make_created_at, make_id, make_index,
    make_unique,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_tables(manager, vec![create_mcp_tokens()]).await?;
        create_indexes(
            manager,
            vec![
                make_unique("mcp_tokens_token_hash_idx", "mcp_tokens", &["token_hash"]),
                make_index("mcp_tokens_user_id_idx", "mcp_tokens", &["user_id"]),
            ],
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_tables(manager, &["mcp_tokens"]).await
    }
}

fn create_mcp_tokens() -> TableCreateStatement {
    Table::create()
        .table("mcp_tokens")
        .col(make_id())
        .col(text("user_id"))
        .col(text("name"))
        .col(text("token_hash"))
        .col(text("hint"))
        .col(make_created_at())
        .col(timestamp_null("last_used_at"))
        .to_owned()
}
