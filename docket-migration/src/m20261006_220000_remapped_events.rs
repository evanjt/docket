//! The event a sha remap writes to each item it rewrites.

use sea_orm_migration::prelude::*;

const UP: &str = r"
ALTER TABLE events DROP CONSTRAINT events_kind_check;
ALTER TABLE events ADD CONSTRAINT events_kind_check CHECK (kind IN ('opened', 'claimed', 'released',
  'waited', 'resumed', 'asked', 'replied', 'decided', 'closed', 'dropped', 'reopened', 'edited',
  'renumbered', 'claim_lost', 'queue', 'legacy', 'audited', 'lead', 'job_reported', 'remapped'));
";

const DOWN: &str = r"
DELETE FROM events WHERE kind='remapped';
ALTER TABLE events DROP CONSTRAINT events_kind_check;
ALTER TABLE events ADD CONSTRAINT events_kind_check CHECK (kind IN ('opened', 'claimed', 'released',
  'waited', 'resumed', 'asked', 'replied', 'decided', 'closed', 'dropped', 'reopened', 'edited',
  'renumbered', 'claim_lost', 'queue', 'legacy', 'audited', 'lead', 'job_reported'));
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
