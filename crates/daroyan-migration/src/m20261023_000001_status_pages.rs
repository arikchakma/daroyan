use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{boolean, json, text, text_null};

use crate::parts::{
    create_indexes, create_tables, drop_tables, make_created_at, make_id, make_unique,
    make_updated_at,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_tables(manager, vec![create_status_pages()]).await?;
        create_indexes(
            manager,
            vec![make_unique("status_pages_slug_idx", "status_pages", &["slug"])],
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_tables(manager, &["status_pages"]).await
    }
}

fn create_status_pages() -> TableCreateStatement {
    Table::create()
        .table("status_pages")
        .col(make_id())
        .col(text("name"))
        .col(text("slug"))
        .col(text_null("logo_url"))
        .col(boolean("is_public").default(true))
        .col(json("sections"))
        .col(make_created_at())
        .col(make_updated_at())
        .to_owned()
}
