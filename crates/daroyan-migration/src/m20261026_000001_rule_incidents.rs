use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
ALTER TABLE "alert_rules" ADD COLUMN "declare_incident" boolean NOT NULL DEFAULT FALSE;
ALTER TABLE "alert_rules" ADD COLUMN "declare_severity" text NULL;
ALTER TABLE "alert_rules" ADD COLUMN "declare_hidden_page_ids" json_text NOT NULL DEFAULT '[]';
"#;

const DOWN: &str = r#"
ALTER TABLE "alert_rules" DROP COLUMN "declare_hidden_page_ids";
ALTER TABLE "alert_rules" DROP COLUMN "declare_severity";
ALTER TABLE "alert_rules" DROP COLUMN "declare_incident";
"#;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(UP).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(DOWN).await.map(|_| ())
    }
}
