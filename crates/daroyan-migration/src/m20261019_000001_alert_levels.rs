use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
ALTER TABLE "alert_rules" ADD COLUMN "options" json_text NOT NULL DEFAULT '{}';
CREATE TABLE "alerts_next" (
    "id" text NOT NULL PRIMARY KEY,
    "rule_id" text NOT NULL,
    "server_id" text NULL,
    "check_id" text NULL,
    "subject" text NOT NULL,
    "value" text NOT NULL,
    "severity" text NOT NULL DEFAULT 'critical',
    "status" text NOT NULL,
    "started_at" timestamp_text NOT NULL,
    "resolved_at" timestamp_text NULL,
    "last_notified_at" timestamp_text NULL,
    "reminders_sent" integer NOT NULL DEFAULT 0,
    FOREIGN KEY ("rule_id") REFERENCES "alert_rules" ("id") ON DELETE CASCADE,
    FOREIGN KEY ("server_id") REFERENCES "servers" ("id") ON DELETE CASCADE,
    FOREIGN KEY ("check_id") REFERENCES "uptime_checks" ("id") ON DELETE CASCADE
);
INSERT INTO "alerts_next" ("id", "rule_id", "server_id", "subject", "value", "status", "started_at", "resolved_at", "last_notified_at")
    SELECT "id", "rule_id", "server_id", "subject", "value", "status", "started_at", "resolved_at", "started_at" FROM "alerts";
DROP TABLE "alerts";
ALTER TABLE "alerts_next" RENAME TO "alerts";
CREATE INDEX "alerts_status_started_at_idx" ON "alerts" ("status", "started_at");
CREATE INDEX "alerts_rule_id_idx" ON "alerts" ("rule_id");
CREATE INDEX "alerts_server_id_idx" ON "alerts" ("server_id");
CREATE INDEX "alerts_check_id_idx" ON "alerts" ("check_id");
"#;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(UP).await.map(|_| ())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Migration("alert levels cannot be rolled back".to_string()))
    }
}
