use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait,
};
use serde_json::Value;

use crate::db::Database;
use crate::db::entities::servers::{ActiveModel, Column, Entity, Model};
use crate::db::entities::{alert_mutes, alerts, commands, crons};
use crate::db::ids::ServerId;

pub struct NewServer {
    pub id: String,
    pub name: String,
    pub labels: Value,
    pub host: Value,
    pub token_hash: String,
}

pub struct ServerReport {
    pub labels: Option<Value>,
    pub host: Option<Value>,
    pub last_report: Value,
}

impl Database {
    pub async fn create_server(&self, server: NewServer) -> Result<Model, DbErr> {
        let now = Utc::now();
        ActiveModel {
            id: Set(server.id),
            name: Set(server.name),
            labels: Set(server.labels),
            host: Set(server.host),
            token_hash: Set(server.token_hash),
            last_report: Set(None),
            last_seen_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn find_server(&self, id: &ServerId) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id(id.as_str()).one(&self.conn).await
    }

    pub async fn find_server_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::TokenHash.eq(token_hash))
            .one(&self.conn)
            .await
    }

    pub async fn list_servers(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .order_by_asc(Column::Name)
            .order_by_asc(Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn list_server_hosts(&self) -> Result<Vec<(String, Value)>, DbErr> {
        Entity::find()
            .select_only()
            .columns([Column::Id, Column::Host])
            .order_by_asc(Column::Name)
            .order_by_asc(Column::Id)
            .into_tuple()
            .all(&self.conn)
            .await
    }

    pub async fn update_server_report(
        &self,
        id: &ServerId,
        report: ServerReport,
    ) -> Result<(), DbErr> {
        let now = Utc::now();
        let mut server = ActiveModel {
            id: Set(id.to_string()),
            last_report: Set(Some(report.last_report)),
            last_seen_at: Set(Some(now)),
            updated_at: Set(now),
            ..Default::default()
        };
        if let Some(labels) = report.labels {
            server.labels = Set(labels);
        }
        if let Some(host) = report.host {
            server.host = Set(host);
        }
        server.update(&self.conn).await.map(|_| ())
    }

    pub async fn rename_server(&self, id: &ServerId, name: &str) -> Result<Model, DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            name: Set(name.to_string()),
            updated_at: Set(Utc::now()),
            ..Default::default()
        }
        .update(&self.conn)
        .await
    }

    pub async fn delete_server(&self, id: &ServerId) -> Result<bool, DbErr> {
        let txn = self.conn.begin().await?;
        commands::Entity::delete_many()
            .filter(commands::Column::ServerId.eq(id.as_str()))
            .exec(&txn)
            .await?;
        alerts::Entity::delete_many()
            .filter(alerts::Column::ServerId.eq(id.as_str()))
            .exec(&txn)
            .await?;
        alert_mutes::Entity::delete_many()
            .filter(alert_mutes::Column::ServerId.eq(id.as_str()))
            .exec(&txn)
            .await?;
        crons::Entity::update_many()
            .col_expr(crons::Column::ServerId, Expr::value(Option::<String>::None))
            .filter(crons::Column::ServerId.eq(id.as_str()))
            .exec(&txn)
            .await?;
        let deleted = Entity::delete_by_id(id.as_str()).exec(&txn).await?;
        txn.commit().await?;
        Ok(deleted.rows_affected > 0)
    }
}
