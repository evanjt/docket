//! The machines jobs run on, each project's lead claim, and the event a lead claim writes.

use sea_orm_migration::prelude::*;

const UP: &str = r#"
CREATE TABLE machines (
  name TEXT COLLATE "C" PRIMARY KEY,         -- the host its key names
  ssh TEXT COLLATE "C" NOT NULL,             -- the address the other machines reach it at
  slots BIGINT NOT NULL CHECK (slots BETWEEN 1 AND 64),
  runners JSONB NOT NULL DEFAULT '[]',       -- [runner]
  note TEXT COLLATE "C",
  updated_at TEXT COLLATE "C" NOT NULL
);
CREATE TABLE leads (
  project TEXT COLLATE "C" PRIMARY KEY REFERENCES projects(slug),
  host TEXT COLLATE "C" NOT NULL,
  session TEXT COLLATE "C" NOT NULL,
  branch TEXT COLLATE "C",
  since TEXT COLLATE "C" NOT NULL,
  renewed_at TEXT COLLATE "C" NOT NULL
);
ALTER TABLE events DROP CONSTRAINT events_kind_check;
ALTER TABLE events ADD CONSTRAINT events_kind_check CHECK (kind IN ('opened', 'claimed', 'released',
  'waited', 'resumed', 'asked', 'replied', 'decided', 'closed', 'dropped', 'reopened', 'edited',
  'renumbered', 'claim_lost', 'queue', 'legacy', 'audited', 'lead'));
"#;

const DOWN: &str = r"
DELETE FROM events WHERE kind='lead';
ALTER TABLE events DROP CONSTRAINT events_kind_check;
ALTER TABLE events ADD CONSTRAINT events_kind_check CHECK (kind IN ('opened', 'claimed', 'released',
  'waited', 'resumed', 'asked', 'replied', 'decided', 'closed', 'dropped', 'reopened', 'edited',
  'renumbered', 'claim_lost', 'queue', 'legacy', 'audited'));
DROP TABLE IF EXISTS leads, machines;
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
