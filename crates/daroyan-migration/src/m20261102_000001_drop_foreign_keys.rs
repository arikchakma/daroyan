use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{
    boolean, integer, integer_null, json, text, text_null, timestamp, timestamp_null,
};

use crate::parts::{
    create_indexes, create_tables, drop_tables, make_created_at, make_id, make_index,
    make_unique, make_updated_at,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

struct Rebuild {
    table: &'static str,
    next: &'static str,
    columns: &'static [&'static str],
    create: fn(&'static str) -> TableCreateStatement,
    indexes: fn() -> Vec<IndexCreateStatement>,
}

const REBUILDS: [Rebuild; 11] = [
    Rebuild {
        table: "commands",
        next: "commands_next",
        columns: &[
            "id",
            "server_id",
            "action",
            "name",
            "status",
            "error",
            "created_at",
            "updated_at",
        ],
        create: create_commands,
        indexes: || {
            vec![make_index(
                "commands_server_id_created_at_idx",
                "commands",
                &["server_id", "created_at"],
            )]
        },
    },
    Rebuild {
        table: "alert_deliveries",
        next: "alert_deliveries_next",
        columns: &[
            "id",
            "rule_id",
            "alert_id",
            "integration_id",
            "integration_name",
            "message",
            "status",
            "error",
            "sent_at",
        ],
        create: create_alert_deliveries,
        indexes: || {
            vec![make_index(
                "alert_deliveries_rule_id_sent_at_idx",
                "alert_deliveries",
                &["rule_id", "sent_at"],
            )]
        },
    },
    Rebuild {
        table: "sessions",
        next: "sessions_next",
        columns: &[
            "id",
            "user_id",
            "token_hash",
            "user_agent",
            "ip_address",
            "created_at",
            "expires_at",
        ],
        create: create_sessions,
        indexes: || {
            vec![
                make_unique("sessions_token_hash_unique", "sessions", &["token_hash"]),
                make_index("sessions_user_id_idx", "sessions", &["user_id"]),
            ]
        },
    },
    Rebuild {
        table: "uptime_outages",
        next: "uptime_outages_next",
        columns: &["id", "check_id", "kind", "cause", "started_at", "resolved_at"],
        create: create_uptime_outages,
        indexes: || {
            vec![make_index(
                "uptime_outages_check_id_started_at_idx",
                "uptime_outages",
                &["check_id", "started_at"],
            )]
        },
    },
    Rebuild {
        table: "alerts",
        next: "alerts_next",
        columns: &[
            "id",
            "rule_id",
            "server_id",
            "check_id",
            "subject",
            "value",
            "severity",
            "status",
            "started_at",
            "resolved_at",
            "last_notified_at",
            "reminders_sent",
            "cron_id",
        ],
        create: create_alerts,
        indexes: || {
            vec![
                make_index(
                    "alerts_status_started_at_idx",
                    "alerts",
                    &["status", "started_at"],
                ),
                make_index("alerts_rule_id_idx", "alerts", &["rule_id"]),
                make_index("alerts_server_id_idx", "alerts", &["server_id"]),
                make_index("alerts_check_id_idx", "alerts", &["check_id"]),
                make_index("alerts_cron_id_idx", "alerts", &["cron_id"]),
            ]
        },
    },
    Rebuild {
        table: "alert_mutes",
        next: "alert_mutes_next",
        columns: &[
            "id",
            "rule_id",
            "server_id",
            "check_id",
            "ends_at",
            "reason",
            "creator_id",
            "created_at",
            "cron_id",
        ],
        create: create_alert_mutes,
        indexes: || {
            vec![make_index("alert_mutes_rule_id_idx", "alert_mutes", &["rule_id"])]
        },
    },
    Rebuild {
        table: "cron_failures",
        next: "cron_failures_next",
        columns: &["id", "cron_id", "kind", "cause", "started_at", "resolved_at"],
        create: create_cron_failures,
        indexes: || {
            vec![make_index(
                "cron_failures_cron_id_started_at_idx",
                "cron_failures",
                &["cron_id", "started_at"],
            )]
        },
    },
    Rebuild {
        table: "cron_open_runs",
        next: "cron_open_runs_next",
        columns: &["cron_id", "run_id", "started_at"],
        create: create_cron_open_runs,
        indexes: Vec::new,
    },
    Rebuild {
        table: "incident_updates",
        next: "incident_updates_next",
        columns: &[
            "id",
            "incident_id",
            "kind",
            "status",
            "body",
            "is_public",
            "creator_id",
            "created_at",
            "edited_at",
        ],
        create: create_incident_updates,
        indexes: || {
            vec![make_index(
                "incident_updates_incident_id_created_at_idx",
                "incident_updates",
                &["incident_id", "created_at"],
            )]
        },
    },
    Rebuild {
        table: "mcp_tokens",
        next: "mcp_tokens_next",
        columns: &[
            "id",
            "user_id",
            "name",
            "token_hash",
            "hint",
            "created_at",
            "last_used_at",
        ],
        create: create_mcp_tokens,
        indexes: || {
            vec![
                make_unique("mcp_tokens_token_hash_idx", "mcp_tokens", &["token_hash"]),
                make_index("mcp_tokens_user_id_idx", "mcp_tokens", &["user_id"]),
            ]
        },
    },
    Rebuild {
        table: "crons",
        next: "crons_next",
        columns: &[
            "id",
            "name",
            "ping_key",
            "schedule_kind",
            "expression",
            "interval_secs",
            "time_zone",
            "grace_secs",
            "max_duration_secs",
            "server_id",
            "command",
            "paused",
            "last_ping_at",
            "missed_slot_at",
            "created_at",
            "updated_at",
        ],
        create: create_crons,
        indexes: || vec![make_unique("crons_ping_key_idx", "crons", &["ping_key"])],
    },
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    fn use_transaction(&self) -> Option<bool> {
        Some(true)
    }

    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for rebuild in &REBUILDS {
            rebuild_table(manager, rebuild).await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

async fn rebuild_table(
    manager: &SchemaManager<'_>,
    rebuild: &Rebuild,
) -> Result<(), DbErr> {
    create_tables(manager, vec![(rebuild.create)(rebuild.next)]).await?;
    let copy = Query::insert()
        .into_table(rebuild.next)
        .columns(rebuild.columns.iter().copied())
        .select_from(
            Query::select()
                .columns(rebuild.columns.iter().copied())
                .from(rebuild.table)
                .to_owned(),
        )
        .map_err(|error| DbErr::Migration(error.to_string()))?
        .to_owned();
    manager.exec_stmt(copy).await?;
    drop_tables(manager, &[rebuild.table]).await?;
    manager
        .rename_table(Table::rename().table(rebuild.next, rebuild.table).to_owned())
        .await?;
    create_indexes(manager, (rebuild.indexes)()).await
}

fn create_commands(name: &'static str) -> TableCreateStatement {
    Table::create()
        .table(name)
        .col(make_id())
        .col(text("server_id"))
        .col(text("action"))
        .col(text("name"))
        .col(text("status"))
        .col(text_null("error"))
        .col(make_created_at())
        .col(make_updated_at())
        .to_owned()
}

fn create_alert_deliveries(name: &'static str) -> TableCreateStatement {
    Table::create()
        .table(name)
        .col(make_id())
        .col(text("rule_id"))
        .col(text_null("alert_id"))
        .col(text("integration_id"))
        .col(text("integration_name"))
        .col(text("message"))
        .col(text("status"))
        .col(text_null("error"))
        .col(timestamp("sent_at"))
        .to_owned()
}

fn create_sessions(name: &'static str) -> TableCreateStatement {
    Table::create()
        .table(name)
        .col(make_id())
        .col(text("user_id"))
        .col(text("token_hash"))
        .col(text_null("user_agent"))
        .col(text_null("ip_address"))
        .col(make_created_at())
        .col(timestamp("expires_at"))
        .to_owned()
}

fn create_uptime_outages(name: &'static str) -> TableCreateStatement {
    Table::create()
        .table(name)
        .col(make_id())
        .col(text("check_id"))
        .col(text("kind"))
        .col(text("cause"))
        .col(timestamp("started_at"))
        .col(timestamp_null("resolved_at"))
        .to_owned()
}

fn create_alerts(name: &'static str) -> TableCreateStatement {
    Table::create()
        .table(name)
        .col(make_id())
        .col(text("rule_id"))
        .col(text_null("server_id"))
        .col(text_null("check_id"))
        .col(text("subject"))
        .col(text("value"))
        .col(text("severity").default("critical"))
        .col(text("status"))
        .col(timestamp("started_at"))
        .col(timestamp_null("resolved_at"))
        .col(timestamp_null("last_notified_at"))
        .col(integer("reminders_sent").default(0))
        .col(text_null("cron_id"))
        .to_owned()
}

fn create_alert_mutes(name: &'static str) -> TableCreateStatement {
    Table::create()
        .table(name)
        .col(make_id())
        .col(text("rule_id"))
        .col(text_null("server_id"))
        .col(text_null("check_id"))
        .col(timestamp_null("ends_at"))
        .col(text_null("reason"))
        .col(text("creator_id"))
        .col(timestamp("created_at"))
        .col(text_null("cron_id"))
        .to_owned()
}

fn create_cron_failures(name: &'static str) -> TableCreateStatement {
    Table::create()
        .table(name)
        .col(make_id())
        .col(text("cron_id"))
        .col(text("kind"))
        .col(text("cause"))
        .col(timestamp("started_at"))
        .col(timestamp_null("resolved_at"))
        .to_owned()
}

fn create_cron_open_runs(name: &'static str) -> TableCreateStatement {
    Table::create()
        .table(name)
        .col(text("cron_id"))
        .col(text("run_id"))
        .col(timestamp("started_at"))
        .primary_key(Index::create().col("cron_id").col("run_id"))
        .to_owned()
}

fn create_incident_updates(name: &'static str) -> TableCreateStatement {
    Table::create()
        .table(name)
        .col(make_id())
        .col(text("incident_id"))
        .col(text("kind"))
        .col(text_null("status"))
        .col(json("body"))
        .col(boolean("is_public"))
        .col(text_null("creator_id"))
        .col(make_created_at())
        .col(timestamp_null("edited_at"))
        .to_owned()
}

fn create_mcp_tokens(name: &'static str) -> TableCreateStatement {
    Table::create()
        .table(name)
        .col(make_id())
        .col(text("user_id"))
        .col(text("name"))
        .col(text("token_hash"))
        .col(text("hint"))
        .col(make_created_at())
        .col(timestamp_null("last_used_at"))
        .to_owned()
}

fn create_crons(name: &'static str) -> TableCreateStatement {
    Table::create()
        .table(name)
        .col(make_id())
        .col(text("name"))
        .col(text("ping_key"))
        .col(text("schedule_kind"))
        .col(text_null("expression"))
        .col(integer_null("interval_secs"))
        .col(text("time_zone"))
        .col(integer("grace_secs"))
        .col(integer_null("max_duration_secs"))
        .col(text_null("server_id"))
        .col(text_null("command"))
        .col(boolean("paused").default(false))
        .col(timestamp_null("last_ping_at"))
        .col(timestamp_null("missed_slot_at"))
        .col(make_created_at())
        .col(make_updated_at())
        .to_owned()
}
