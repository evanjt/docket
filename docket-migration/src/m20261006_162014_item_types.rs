//! An item's type and priority as columns. The type is the key's own when it is one of the fixed
//! keys, else by the kind the project gave the key (`ItemType::of_stored`); the priority is the tier
//! its tags named, which then leave the tags. Every item whose file changes is left for the dump.

use sea_orm_migration::prelude::*;

use crate::item_types::assign;

const UP: &str = r#"
ALTER TABLE items ADD COLUMN type TEXT COLLATE "C" NOT NULL DEFAULT 'task'
  CHECK (type IN ('task', 'bug', 'question', 'investigation', 'plan'));
ALTER TABLE items ADD COLUMN priority TEXT COLLATE "C" NOT NULL DEFAULT 'normal'
  CHECK (priority IN ('critical', 'high', 'normal', 'low'));
CREATE INDEX items_type ON items(project, type);
"#;

const DOWN: &str = "DROP INDEX IF EXISTS items_type; \
    ALTER TABLE items DROP COLUMN IF EXISTS priority; ALTER TABLE items DROP COLUMN IF EXISTS type;";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        c.execute_unprepared(UP).await?;
        assign(c).await
    }

    /// The priority goes back into the tags the way an item carried it.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        c.execute_unprepared(
            "UPDATE items SET tags = (tags || to_jsonb(priority)) WHERE priority <> 'normal'",
        )
        .await?;
        c.execute_unprepared(DOWN).await?;
        Ok(())
    }
}
