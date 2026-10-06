//! An attempt may end `ended`: a session that finished or paused without a build to land.

use sea_orm_migration::prelude::*;

const UP: &str = r"
ALTER TABLE assignments DROP CONSTRAINT assignments_outcome_check;
ALTER TABLE assignments ADD CONSTRAINT assignments_outcome_check
  CHECK (outcome IN ('landed', 'conflict', 'gate', 'blocked', 'failed', 'ended'));
";

/// A row ended `ended` has no older word, so it becomes the failure the old records called it.
const DOWN: &str = r"
UPDATE assignments SET outcome = 'failed' WHERE outcome = 'ended';
ALTER TABLE assignments DROP CONSTRAINT assignments_outcome_check;
ALTER TABLE assignments ADD CONSTRAINT assignments_outcome_check
  CHECK (outcome IN ('landed', 'conflict', 'gate', 'blocked', 'failed'));
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
