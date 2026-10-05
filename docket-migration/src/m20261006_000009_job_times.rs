//! When a job began and ended and how it exited, kept on the assignment row of its attempt.

use sea_orm_migration::prelude::*;

const UP: &str = r#"
ALTER TABLE assignments ADD COLUMN job_started_at TEXT COLLATE "C";
ALTER TABLE assignments ADD COLUMN job_ended_at TEXT COLLATE "C";
ALTER TABLE assignments ADD COLUMN job_exit INTEGER;
"#;

const DOWN: &str = r"
ALTER TABLE assignments DROP COLUMN IF EXISTS job_exit;
ALTER TABLE assignments DROP COLUMN IF EXISTS job_ended_at;
ALTER TABLE assignments DROP COLUMN IF EXISTS job_started_at;
";

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
