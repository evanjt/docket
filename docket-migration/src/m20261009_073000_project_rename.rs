//! A project's slug may change under its rows: every foreign key to `projects(slug)` follows the
//! change, and the event a rename writes has a kind.

use sea_orm_migration::prelude::*;

/// Recreates every foreign key that references `projects(slug)`, read from the catalogue so a table
/// added later is covered too, with the update rule given.
fn recreate_with(rule: &str) -> String {
    format!(
        r"
DO $$
DECLARE c RECORD;
BEGIN
  FOR c IN
    SELECT con.conname, cls.relname, att.attname
    FROM pg_constraint con
    JOIN pg_class cls ON cls.oid = con.conrelid
    JOIN pg_attribute att ON att.attrelid = con.conrelid AND att.attnum = con.conkey[1]
    WHERE con.contype = 'f' AND con.confrelid = 'projects'::regclass
  LOOP
    EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I, ADD CONSTRAINT %I FOREIGN KEY (%I) '
                   'REFERENCES projects(slug) {rule}',
                   c.relname, c.conname, c.conname, c.attname);
  END LOOP;
END $$;
"
    )
}

const KINDS_WITH_RENAMED: &str = r"
ALTER TABLE events DROP CONSTRAINT events_kind_check;
ALTER TABLE events ADD CONSTRAINT events_kind_check CHECK (kind IN ('opened', 'claimed', 'released',
  'waited', 'resumed', 'asked', 'replied', 'decided', 'closed', 'dropped', 'reopened', 'edited',
  'renumbered', 'claim_lost', 'queue', 'legacy', 'audited', 'lead', 'job_reported', 'remapped',
  'renamed'));
";

const KINDS_WITHOUT_RENAMED: &str = r"
DELETE FROM events WHERE kind='renamed';
ALTER TABLE events DROP CONSTRAINT events_kind_check;
ALTER TABLE events ADD CONSTRAINT events_kind_check CHECK (kind IN ('opened', 'claimed', 'released',
  'waited', 'resumed', 'asked', 'replied', 'decided', 'closed', 'dropped', 'reopened', 'edited',
  'renumbered', 'claim_lost', 'queue', 'legacy', 'audited', 'lead', 'job_reported', 'remapped'));
";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(&recreate_with("ON UPDATE CASCADE"))
            .await?;
        db.execute_unprepared(KINDS_WITH_RENAMED).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(KINDS_WITHOUT_RENAMED).await?;
        db.execute_unprepared(&recreate_with("ON UPDATE NO ACTION"))
            .await?;
        Ok(())
    }
}
