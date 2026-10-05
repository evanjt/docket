//! What an ask needs of the owner, kept on the assignment row it opens.

use sea_orm_migration::prelude::*;

const UP: &str = r#"
ALTER TABLE assignments ADD COLUMN need TEXT COLLATE "C"
  CHECK (need IS NULL OR need IN ('hold', 'access', 'act', 'judge'));
"#;

const DOWN: &str = "ALTER TABLE assignments DROP COLUMN IF EXISTS need;";

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
