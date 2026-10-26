use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
ALTER TABLE "install_tokens" ADD COLUMN "max_uses" integer NOT NULL DEFAULT 1;
ALTER TABLE "install_tokens" ADD COLUMN "uses" integer NOT NULL DEFAULT 0;
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
                r#"ALTER TABLE "install_tokens" DROP COLUMN "uses";
ALTER TABLE "install_tokens" DROP COLUMN "max_uses";"#,
            )
            .await
            .map(|_| ())
    }
}
