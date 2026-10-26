use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
CREATE TABLE "alert_mutes" (
    "id" text NOT NULL PRIMARY KEY,
    "rule_id" text NOT NULL,
    "server_id" text NULL,
    "check_id" text NULL,
    "ends_at" timestamp_text NULL,
    "reason" text NULL,
    "muted_by" text NOT NULL,
    "created_at" timestamp_text NOT NULL,
    FOREIGN KEY ("rule_id") REFERENCES "alert_rules" ("id") ON DELETE CASCADE,
    FOREIGN KEY ("server_id") REFERENCES "servers" ("id") ON DELETE CASCADE,
    FOREIGN KEY ("check_id") REFERENCES "uptime_checks" ("id") ON DELETE CASCADE
);
CREATE INDEX "alert_mutes_rule_id_idx" ON "alert_mutes" ("rule_id");
ALTER TABLE "alert_rules" ADD COLUMN "quiet_until" timestamp_text NULL;
"#;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(UP).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"DROP TABLE "alert_mutes"; ALTER TABLE "alert_rules" DROP COLUMN "quiet_until";"#,
            )
            .await
            .map(|_| ())
    }
}
