use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{text, text_null, timestamp, timestamp_null};

use crate::parts::{create_tables, drop_tables};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_tables(manager, vec![create_cron_open_runs()]).await?;
        manager
            .exec_stmt(
                Query::insert()
                    .into_table("cron_open_runs")
                    .columns(["cron_id", "run_id", "started_at"])
                    .select_from(
                        Query::select()
                            .column("id")
                            .expr(Func::coalesce([Expr::col("run_id"), Expr::val("")]))
                            .column("run_started_at")
                            .from("crons")
                            .and_where(Expr::col("run_started_at").is_not_null())
                            .to_owned(),
                    )
                    .map_err(|error| DbErr::Migration(error.to_string()))?
                    .to_owned(),
            )
            .await?;
        for column in ["run_started_at", "run_id"] {
            manager
                .alter_table(Table::alter().table("crons").drop_column(column).to_owned())
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [timestamp_null("run_started_at"), text_null("run_id")] {
            let mut column = column;
            manager
                .alter_table(
                    Table::alter().table("crons").add_column(&mut column).to_owned(),
                )
                .await?;
        }
        drop_tables(manager, &["cron_open_runs"]).await
    }
}

fn create_cron_open_runs() -> TableCreateStatement {
    Table::create()
        .table("cron_open_runs")
        .col(text("cron_id"))
        .col(text("run_id"))
        .col(timestamp("started_at"))
        .primary_key(Index::create().col("cron_id").col("run_id"))
        .foreign_key(
            ForeignKey::create()
                .from("cron_open_runs", "cron_id")
                .to("crons", "id")
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}
