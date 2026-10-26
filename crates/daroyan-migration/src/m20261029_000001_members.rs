use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{text, text_null};

use crate::parts::{
    create_indexes, create_tables, drop_tables, make_created_at, make_id, make_unique,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_tables(manager, vec![create_members()]).await?;
        create_indexes(
            manager,
            vec![make_unique("members_email_idx", "members", &["email"])],
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_tables(manager, &["members"]).await
    }
}

fn create_members() -> TableCreateStatement {
    Table::create()
        .table("members")
        .col(make_id())
        .col(text("email"))
        .col(text("role"))
        .col(text_null("inviter_id"))
        .col(make_created_at())
        .to_owned()
}
