//! The usage limits a runner reported on a machine, each with the reset it named.

use sea_orm_migration::prelude::*;

const UP: &str = r"
ALTER TABLE machines ADD COLUMN limits JSONB NOT NULL DEFAULT '{}';  -- {runner: reset stamp}
";

const DOWN: &str = "ALTER TABLE machines DROP COLUMN IF EXISTS limits;";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(UP).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(DOWN).await?;
        Ok(())
    }
}
