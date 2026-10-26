use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder,
};

use crate::db::Database;
use crate::db::entities::cron_open_runs::{ActiveModel, Column, Entity, Model};

pub const MAX_OPEN_RUNS: usize = 50;

impl Database {
    pub async fn open_cron_run(
        &self,
        cron_id: &str,
        run_id: &str,
        started_at: DateTime<Utc>,
    ) -> Result<(), DbErr> {
        self.delete_cron_run(cron_id, run_id).await?;
        ActiveModel {
            cron_id: Set(cron_id.to_string()),
            run_id: Set(run_id.to_string()),
            started_at: Set(started_at),
        }
        .insert(&self.conn)
        .await?;
        let runs: Vec<Model> = Entity::find()
            .filter(Column::CronId.eq(cron_id))
            .order_by_desc(Column::StartedAt)
            .all(&self.conn)
            .await?;
        for run in runs.into_iter().skip(MAX_OPEN_RUNS) {
            self.delete_cron_run(&run.cron_id, &run.run_id).await?;
        }
        Ok(())
    }

    pub async fn take_cron_run(
        &self,
        cron_id: &str,
        run_id: Option<&str>,
    ) -> Result<Option<Model>, DbErr> {
        let mut query = Entity::find().filter(Column::CronId.eq(cron_id));
        if let Some(run_id) = run_id {
            query = query.filter(Column::RunId.eq(run_id));
        }
        let run = query.order_by_desc(Column::StartedAt).one(&self.conn).await?;
        if let Some(run) = &run {
            self.delete_cron_run(&run.cron_id, &run.run_id).await?;
        }
        Ok(run)
    }

    pub async fn list_open_cron_runs(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find().order_by_asc(Column::StartedAt).all(&self.conn).await
    }

    pub async fn delete_cron_run(
        &self,
        cron_id: &str,
        run_id: &str,
    ) -> Result<(), DbErr> {
        Entity::delete_many()
            .filter(Column::CronId.eq(cron_id))
            .filter(Column::RunId.eq(run_id))
            .exec(&self.conn)
            .await
            .map(|_| ())
    }
}
