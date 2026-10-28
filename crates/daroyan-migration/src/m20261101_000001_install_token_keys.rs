use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{integer, integer_null, text, timestamp, timestamp_null};

use crate::parts::{
    create_indexes, create_tables, drop_tables, make_created_at, make_id, make_unique,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    fn use_transaction(&self) -> Option<bool> {
        Some(true)
    }

    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_tables(manager, vec![create_install_tokens("install_tokens_new")]).await?;
        let copy = Query::insert()
            .into_table("install_tokens_new")
            .columns([
                "id",
                "name",
                "token_hash",
                "hint",
                "expires_at",
                "max_uses",
                "uses",
                "created_at",
            ])
            .select_from(
                Query::select()
                    .column("id")
                    .expr(Expr::val("Install command"))
                    .column("token_hash")
                    .expr(Expr::val(""))
                    .columns(["expires_at", "max_uses", "uses", "created_at"])
                    .from("install_tokens")
                    .to_owned(),
            )
            .map_err(|error| DbErr::Migration(error.to_string()))?
            .to_owned();
        manager.exec_stmt(copy).await?;
        swap_table(manager, "install_tokens_new").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_tables(manager, vec![create_old_install_tokens()]).await?;
        let copy = Query::insert()
            .into_table("install_tokens_old")
            .columns(["id", "token_hash", "expires_at", "created_at", "max_uses", "uses"])
            .select_from(
                Query::select()
                    .columns(["id", "token_hash"])
                    .expr(Func::coalesce([
                        Expr::col("expires_at"),
                        Expr::col("created_at"),
                    ]))
                    .column("created_at")
                    .expr(Func::coalesce([Expr::col("max_uses"), Expr::col("uses")]))
                    .column("uses")
                    .from("install_tokens")
                    .to_owned(),
            )
            .map_err(|error| DbErr::Migration(error.to_string()))?
            .to_owned();
        manager.exec_stmt(copy).await?;
        swap_table(manager, "install_tokens_old").await
    }
}

async fn swap_table(
    manager: &SchemaManager<'_>,
    from: &'static str,
) -> Result<(), DbErr> {
    drop_tables(manager, &["install_tokens"]).await?;
    manager
        .rename_table(Table::rename().table(from, "install_tokens").to_owned())
        .await?;
    create_indexes(
        manager,
        vec![make_unique(
            "install_tokens_token_hash_key",
            "install_tokens",
            &["token_hash"],
        )],
    )
    .await
}

fn create_install_tokens(name: &'static str) -> TableCreateStatement {
    Table::create()
        .table(name)
        .col(make_id())
        .col(text("name"))
        .col(text("token_hash"))
        .col(text("hint"))
        .col(timestamp_null("expires_at"))
        .col(integer_null("max_uses"))
        .col(integer("uses").default(0))
        .col(make_created_at())
        .col(timestamp_null("last_used_at"))
        .to_owned()
}

fn create_old_install_tokens() -> TableCreateStatement {
    Table::create()
        .table("install_tokens_old")
        .col(make_id())
        .col(text("token_hash"))
        .col(timestamp("expires_at"))
        .col(make_created_at())
        .col(integer("max_uses").default(1))
        .col(integer("uses").default(0))
        .to_owned()
}
