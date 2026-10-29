//! Where a claim's finished build stands on its way to landing: the stage a `land` step leaves on
//! the open claim, kept on its assignment, and the event kinds that write it.

use sea_orm_migration::prelude::*;

const UP: &str = r"
ALTER TABLE assignments ADD COLUMN land JSONB;
ALTER TABLE events DROP CONSTRAINT events_kind_check;
ALTER TABLE events ADD CONSTRAINT events_kind_check CHECK (kind IN ('opened', 'claimed', 'released',
  'waited', 'resumed', 'asked', 'replied', 'decided', 'closed', 'dropped', 'reopened', 'edited',
  'renumbered', 'claim_lost', 'queue', 'legacy', 'audited', 'lead', 'job_reported', 'remapped',
  'renamed', 'land_queued', 'land_gating', 'land_failed', 'land_conflict'));
";

const DOWN: &str = r"
DELETE FROM events WHERE kind IN ('land_queued', 'land_gating', 'land_failed', 'land_conflict');
ALTER TABLE events DROP CONSTRAINT events_kind_check;
ALTER TABLE events ADD CONSTRAINT events_kind_check CHECK (kind IN ('opened', 'claimed', 'released',
  'waited', 'resumed', 'asked', 'replied', 'decided', 'closed', 'dropped', 'reopened', 'edited',
  'renumbered', 'claim_lost', 'queue', 'legacy', 'audited', 'lead', 'job_reported', 'remapped',
  'renamed'));
ALTER TABLE assignments DROP COLUMN IF EXISTS land;
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
