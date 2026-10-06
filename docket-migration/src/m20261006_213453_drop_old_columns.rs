//! The columns the core replaced go: an item's claim and turn (its assignment rows), its wait (its
//! dependencies), its scope, group, theme, rank and tags (its release, priority and labels) and its
//! sync conflict mark, with the checks and the index on them, and a project's key list and themes
//! (docket fixes the keys, and labels carry the themes). Refused while an item still carries a sync
//! conflict, which a person resolves first.

use sea_orm_migration::prelude::*;

use crate::statement;

/// Each item still marked as carrying a sync conflict, as `project id`.
const CONFLICTED: &str =
    "SELECT project || ' ' || id FROM items WHERE conflict <> 0 ORDER BY project, key, num";

/// Dropping a column drops every check and index that reads it.
const UP: &str = "
ALTER TABLE items
  DROP COLUMN turn, DROP COLUMN turn_note, DROP COLUMN asked_at,
  DROP COLUMN claim_branch, DROP COLUMN claim_host, DROP COLUMN claim_since,
  DROP COLUMN claim_runner, DROP COLUMN claim_job, DROP COLUMN claim_on,
  DROP COLUMN wait_on, DROP COLUMN wait_item, DROP COLUMN wait_ref, DROP COLUMN wait_since,
  DROP COLUMN scope, DROP COLUMN group_name, DROP COLUMN theme, DROP COLUMN rank,
  DROP COLUMN tags, DROP COLUMN conflict;
ALTER TABLE projects DROP COLUMN keys, DROP COLUMN themes;
";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        let mut conflicted = Vec::new();
        for r in c.query_all_raw(statement(CONFLICTED, vec![])).await? {
            conflicted.push(r.try_get_by_index::<String>(0)?);
        }
        if !conflicted.is_empty() {
            return Err(DbErr::Migration(format!(
                "{} carry a sync conflict in their body: resolve each and clear its mark before \
                 the columns are dropped: {}",
                conflicted.len(),
                conflicted.join(", ")
            )));
        }
        c.execute_unprepared(UP).await?;
        Ok(())
    }

    /// The columns stay dropped: what they held lives in the rows that replaced them.
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
