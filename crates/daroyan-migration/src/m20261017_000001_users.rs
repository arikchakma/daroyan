use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{text, text_null, timestamp};

use crate::parts::{
    create_indexes, create_tables, drop_tables, make_created_at, make_id, make_index,
    make_unique, make_updated_at,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_tables(manager, vec![create_users(), create_sessions()]).await?;
        create_indexes(manager, list_indexes()).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_tables(manager, &["sessions", "users"]).await
    }
}

fn create_users() -> TableCreateStatement {
    Table::create()
        .table("users")
        .col(make_id())
        .col(text("email"))
        .col(text("name"))
        .col(text_null("avatar_url"))
        .col(text("google_id"))
        .col(make_created_at())
        .col(make_updated_at())
        .to_owned()
}

fn create_sessions() -> TableCreateStatement {
    Table::create()
        .table("sessions")
        .col(make_id())
        .col(text("user_id"))
        .col(text("token_hash"))
        .col(text_null("user_agent"))
        .col(text_null("ip_address"))
        .col(make_created_at())
        .col(timestamp("expires_at"))
        .foreign_key(
            ForeignKey::create()
                .from("sessions", "user_id")
                .to("users", "id")
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn list_indexes() -> Vec<IndexCreateStatement> {
    vec![
        make_unique("users_email_unique", "users", &["email"]),
        make_unique("users_google_id_unique", "users", &["google_id"]),
        make_unique("sessions_token_hash_unique", "sessions", &["token_hash"]),
        make_index("sessions_user_id_idx", "sessions", &["user_id"]),
    ]
}
