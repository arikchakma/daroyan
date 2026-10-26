use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
};

use crate::commands::STATUS_SENT;
use crate::db::Database;
use crate::db::entities::commands::{ActiveModel, Column, Entity, Model};
use crate::db::ids::{CommandId, ServerId};

pub struct NewCommand {
    pub id: String,
    pub server_id: String,
    pub action: String,
    pub name: String,
    pub status: String,
}

impl Database {
    pub async fn create_command(&self, command: NewCommand) -> Result<Model, DbErr> {
        let now = Utc::now();
        ActiveModel {
            id: Set(command.id),
            server_id: Set(command.server_id),
            action: Set(command.action),
            name: Set(command.name),
            status: Set(command.status),
            error: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn find_command(
        &self,
        server_id: &ServerId,
        id: &CommandId,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id(id.as_str())
            .filter(Column::ServerId.eq(server_id.as_str()))
            .one(&self.conn)
            .await
    }

    pub async fn update_command_status(
        &self,
        server_id: &ServerId,
        id: &CommandId,
        status: &str,
        error: Option<String>,
    ) -> Result<(), DbErr> {
        Entity::update_many()
            .filter(Column::Id.eq(id.as_str()))
            .filter(Column::ServerId.eq(server_id.as_str()))
            .filter(Column::Status.eq(STATUS_SENT))
            .set(ActiveModel {
                status: Set(status.to_string()),
                error: Set(error),
                updated_at: Set(Utc::now()),
                ..Default::default()
            })
            .exec(&self.conn)
            .await
            .map(|_| ())
    }
}
