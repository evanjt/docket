//! The event a finished job's report writes to the open claim of its item.

use sea_orm_migration::prelude::*;

const UP: &str = r"
ALTER TABLE events DROP CONSTRAINT events_kind_check;
ALTER TABLE events ADD CONSTRAINT events_kind_check CHECK (kind IN ('opened', 'claimed', 'released',
  'waited', 'resumed', 'asked', 'replied', 'decided', 'closed', 'dropped', 'reopened', 'edited',
  'renumbered', 'claim_lost', 'queue', 'legacy', 'audited', 'lead', 'job_reported'));
";

const DOWN: &str = r"
DELETE FROM events WHERE kind='job_reported';
ALTER TABLE events DROP CONSTRAINT events_kind_check;
ALTER TABLE events ADD CONSTRAINT events_kind_check CHECK (kind IN ('opened', 'claimed', 'released',
  'waited', 'resumed', 'asked', 'replied', 'decided', 'closed', 'dropped', 'reopened', 'edited',
  'renumbered', 'claim_lost', 'queue', 'legacy', 'audited', 'lead'));
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
