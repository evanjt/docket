//! The directories a machine's remote command puts before PATH.

use sea_orm_migration::prelude::*;

const UP: &str = r"
ALTER TABLE machines ADD COLUMN path_prefix TEXT;
";

const DOWN: &str = "ALTER TABLE machines DROP COLUMN IF EXISTS path_prefix;";

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
