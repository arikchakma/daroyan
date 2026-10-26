use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, Statement};
use serde_json::{Map, Value};

#[derive(DeriveMigrationName)]
pub struct Migration;

const JSON_COLUMNS: [(&str, &str); 8] = [
    ("alert_rules", "condition"),
    ("alert_rules", "scope"),
    ("alert_rules", "options"),
    ("incidents", "responders"),
    ("incidents", "follow_ups"),
    ("integrations", "config"),
    ("servers", "host"),
    ("servers", "last_report"),
];

const COMMAND_ACTIONS: [(&str, &str); 3] = [
    ("startProcess", "start_process"),
    ("stopProcess", "stop_process"),
    ("restartProcess", "restart_process"),
];

const UP: &str = r#"
ALTER TABLE "incident_updates" RENAME COLUMN "author_id" TO "creator_id";
ALTER TABLE "alert_mutes" RENAME COLUMN "muted_by" TO "creator_id";
UPDATE "alert_mutes" SET "creator_id" = (SELECT "id" FROM "users" ORDER BY "created_at" LIMIT 1);
"#;

const DOWN: &str = r#"
ALTER TABLE "alert_mutes" RENAME COLUMN "creator_id" TO "muted_by";
ALTER TABLE "incident_updates" RENAME COLUMN "creator_id" TO "author_id";
"#;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(UP).await?;
        rewrite_json(manager, to_snake).await?;
        rename_actions(manager, COMMAND_ACTIONS).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rename_actions(manager, COMMAND_ACTIONS.map(|(old, new)| (new, old))).await?;
        rewrite_json(manager, to_camel).await?;
        manager.get_connection().execute_unprepared(DOWN).await.map(|_| ())
    }
}

fn to_snake(key: &str) -> String {
    let mut snake = String::with_capacity(key.len() + 4);
    for character in key.chars() {
        if character.is_ascii_uppercase() {
            snake.push('_');
            snake.push(character.to_ascii_lowercase());
        } else {
            snake.push(character);
        }
    }
    snake
}

fn to_camel(key: &str) -> String {
    let mut camel = String::with_capacity(key.len());
    let mut is_upper = false;
    for character in key.chars() {
        if character == '_' {
            is_upper = true;
        } else if is_upper {
            camel.push(character.to_ascii_uppercase());
            is_upper = false;
        } else {
            camel.push(character);
        }
    }
    camel
}

fn rename_keys(value: Value, convert: fn(&str) -> String) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, item)| {
                    let item = match item {
                        Value::String(text) if key == "kind" => {
                            Value::String(convert(&text))
                        }
                        item => rename_keys(item, convert),
                    };
                    (convert(&key), item)
                })
                .collect::<Map<String, Value>>(),
        ),
        Value::Array(items) => Value::Array(
            items.into_iter().map(|item| rename_keys(item, convert)).collect(),
        ),
        other => other,
    }
}

async fn rewrite_json(
    manager: &SchemaManager<'_>,
    convert: fn(&str) -> String,
) -> Result<(), DbErr> {
    let db = manager.get_connection();
    let backend = db.get_database_backend();
    for (table, column) in JSON_COLUMNS {
        let rows = db
            .query_all_raw(Statement::from_string(
                backend,
                format!(
                    r#"SELECT "id", CAST("{column}" AS TEXT) AS "value" FROM "{table}" WHERE "{column}" IS NOT NULL"#
                ),
            ))
            .await?;
        for row in rows {
            let id: String = row.try_get("", "id")?;
            let text: String = row.try_get("", "value")?;
            let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
            let renamed = rename_keys(value.clone(), convert);
            if renamed == value {
                continue;
            }
            db.execute_raw(Statement::from_sql_and_values(
                backend,
                format!(r#"UPDATE "{table}" SET "{column}" = ? WHERE "id" = ?"#),
                [renamed.to_string().into(), id.into()],
            ))
            .await?;
        }
    }
    Ok(())
}

async fn rename_actions(
    manager: &SchemaManager<'_>,
    actions: [(&str, &str); 3],
) -> Result<(), DbErr> {
    let db = manager.get_connection();
    let backend = db.get_database_backend();
    for (old, new) in actions {
        db.execute_raw(Statement::from_sql_and_values(
            backend,
            r#"UPDATE "commands" SET "action" = ? WHERE "action" = ?"#,
            [new.into(), old.into()],
        ))
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sea_orm_migration::sea_orm::{ConnectionTrait, Database, Statement};
    use serde_json::{Value, json};

    use super::Migration;
    use crate::{MigrationName, Migrator, MigratorTrait};

    const SEED: &str = r#"
INSERT INTO "users" ("id", "email", "name", "google_id", "created_at") VALUES ('usr_1', 'a@x.test', 'Arik Chakma', 'g1', '2026-10-01 00:00:00');
INSERT INTO "servers" ("id", "name", "labels", "host", "token_hash", "last_report") VALUES ('srv_1', 'web', '["prod"]',
 '{"name":"web","labels":["prod"],"platformVersion":"24.04","cpuCores":4,"memTotal":8,"privateIp":null}',
 'hash', '{"state":{"takenAt":1,"memUsed":2,"disks":[{"diskTotal":3}],"nics":[{"netInSpeed":4}]},"processes":[{"name":"nginx","status":"running","startedAt":5}],"ports":[{"latencyMs":6}]}');
INSERT INTO "commands" ("id", "server_id", "action", "name", "status") VALUES ('cmd_1', 'srv_1', 'restartProcess', 'nginx', 'sent');
INSERT INTO "integrations" ("id", "name", "kind", "config") VALUES ('int_1', 'Slack', 'slack',
 '{"kind":"slack","apiUrl":"u","botToken":"t","channelId":"c","channelName":"n"}');
INSERT INTO "alert_rules" ("id", "name", "enabled", "condition", "for_secs", "scope", "integration_ids", "options")
 VALUES ('rul_1', 'Logs', 1, '{"kind":"logs","level":"error","count":3,"withinSecs":60,"service":null,"text":null}', 0,
 '{"kind":"servers","serverIds":["srv_1"]}', '["int_1"]',
 '{"windowSecs":60,"noData":"keep","quietHours":[{"days":[1],"from":"22:00","to":"06:00"}],"timeZone":"UTC"}');
INSERT INTO "alert_rules" ("id", "name", "enabled", "condition", "for_secs", "scope", "integration_ids")
 VALUES ('rul_2', 'Down', 1, '{"kind":"processStopped","name":"nginx"}', 0, '{"kind":"all"}', '[]');
INSERT INTO "alert_mutes" ("id", "rule_id", "muted_by", "created_at") VALUES ('mut_1', 'rul_1', 'Arik', '2026-10-20 00:00:00');
INSERT INTO "incidents" ("id", "kind", "title", "status", "started_at", "components", "related", "responders", "channel_ids", "hidden_page_ids", "follow_ups", "postmortem")
 VALUES ('inc_1', 'incident', 'Down', 'resolved', '2026-10-20 00:00:00', '[]', '[]',
 '[{"userId":"usr_1","role":"Lead"}]', '[]', '[]', '[{"id":"f1","body":"Fix","ownerId":"usr_1","isDone":false}]',
 '{"type":"doc","content":[{"type":"bulletList"}]}');
INSERT INTO "incident_updates" ("id", "incident_id", "kind", "body", "is_public", "author_id")
 VALUES ('upd_1', 'inc_1', 'update', '{"type":"doc","content":[{"type":"hardBreak"}]}', 1, 'usr_1');
"#;

    async fn read(db: &impl ConnectionTrait, sql: &str) -> Value {
        let row = db
            .query_one_raw(Statement::from_string(db.get_database_backend(), sql))
            .await
            .unwrap()
            .unwrap();
        let text: String = row.try_get("", "value").unwrap();
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    }

    #[tokio::test]
    async fn renames_stored_keys_and_columns() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let before = Migrator::migrations()
            .iter()
            .position(|migration| migration.name() == Migration.name())
            .unwrap();
        Migrator::up(&db, Some(u32::try_from(before).unwrap())).await.unwrap();
        db.execute_unprepared(SEED).await.unwrap();
        Migrator::up(&db, None).await.unwrap();

        let value = |column: &str, table: &str, id: &str| {
            format!(
                r#"SELECT CAST("{column}" AS TEXT) AS "value" FROM "{table}" WHERE "id" = '{id}'"#
            )
        };
        assert_eq!(
            read(&db, &value("host", "servers", "srv_1")).await,
            json!({"name":"web","labels":["prod"],"platform_version":"24.04","cpu_cores":4,"mem_total":8,"private_ip":null})
        );
        assert_eq!(
            read(&db, &value("last_report", "servers", "srv_1")).await,
            json!({"state":{"taken_at":1,"mem_used":2,"disks":[{"disk_total":3}],"nics":[{"net_in_speed":4}]},"processes":[{"name":"nginx","status":"running","started_at":5}],"ports":[{"latency_ms":6}]})
        );
        assert_eq!(
            read(&db, &value("action", "commands", "cmd_1")).await,
            "restart_process"
        );
        assert_eq!(
            read(&db, &value("config", "integrations", "int_1")).await,
            json!({"kind":"slack","api_url":"u","bot_token":"t","channel_id":"c","channel_name":"n"})
        );
        assert_eq!(
            read(&db, &value("condition", "alert_rules", "rul_1")).await,
            json!({"kind":"logs","level":"error","count":3,"within_secs":60,"service":null,"text":null})
        );
        assert_eq!(
            read(&db, &value("scope", "alert_rules", "rul_1")).await,
            json!({"kind":"servers","server_ids":["srv_1"]})
        );
        assert_eq!(
            read(&db, &value("options", "alert_rules", "rul_1")).await,
            json!({"window_secs":60,"no_data":"keep","quiet_hours":[{"days":[1],"from":"22:00","to":"06:00"}],"time_zone":"UTC"})
        );
        assert_eq!(
            read(&db, &value("condition", "alert_rules", "rul_2")).await,
            json!({"kind":"process_stopped","name":"nginx"})
        );
        assert_eq!(
            read(&db, &value("responders", "incidents", "inc_1")).await,
            json!([{"user_id":"usr_1","role":"Lead"}])
        );
        assert_eq!(
            read(&db, &value("follow_ups", "incidents", "inc_1")).await,
            json!([{"id":"f1","body":"Fix","owner_id":"usr_1","is_done":false}])
        );
        assert_eq!(
            read(&db, &value("postmortem", "incidents", "inc_1")).await,
            json!({"type":"doc","content":[{"type":"bulletList"}]})
        );
        assert_eq!(
            read(&db, &value("body", "incident_updates", "upd_1")).await,
            json!({"type":"doc","content":[{"type":"hardBreak"}]})
        );

        assert_eq!(
            read(&db, &value("creator_id", "alert_mutes", "mut_1")).await,
            "usr_1"
        );
        assert_eq!(
            read(&db, &value("creator_id", "incident_updates", "upd_1")).await,
            "usr_1"
        );

        let after = Migrator::migrations().len() - before;
        Migrator::down(&db, Some(u32::try_from(after).unwrap()))
            .await
            .unwrap();
        assert_eq!(
            read(&db, &value("author_id", "incident_updates", "upd_1")).await,
            "usr_1"
        );
        assert_eq!(
            read(&db, &value("action", "commands", "cmd_1")).await,
            "restartProcess"
        );
        assert_eq!(
            read(&db, &value("condition", "alert_rules", "rul_2")).await,
            json!({"kind":"processStopped","name":"nginx"})
        );
    }
}
