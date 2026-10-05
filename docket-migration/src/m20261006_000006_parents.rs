//! One parent plan per item, and an `origin` link for what spawned it; the `opened` links split
//! between them by `parents::split`.

use sea_orm_migration::prelude::*;

use crate::parents;

const UP: &str = "
ALTER TABLE items ADD COLUMN parent_rid BIGINT REFERENCES items(rid) DEFERRABLE;
ALTER TABLE items ADD CONSTRAINT items_parent_check CHECK (parent_rid IS NULL OR parent_rid <> rid);
CREATE INDEX items_parent ON items(parent_rid);
";

const DOWN: &str = "
ALTER TABLE links DROP CONSTRAINT links_kind_check;
INSERT INTO links (rid, kind, to_rid) SELECT rid, 'opened', parent_rid FROM items WHERE parent_rid IS NOT NULL;
UPDATE links SET kind='opened' WHERE kind='origin';
ALTER TABLE links ADD CONSTRAINT links_kind_check
  CHECK (kind IN ('related', 'opened', 'cites_file', 'cites_test'));
ALTER TABLE items DROP COLUMN parent_rid;
";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        c.execute_unprepared(UP).await?;
        parents::admit_opened(c).await?;
        parents::split(c).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(DOWN).await?;
        Ok(())
    }
}
