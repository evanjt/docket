//! Every item holds an area. The column refuses a row with none, and the step is refused while an
//! item still holds none, naming each, so the placement step runs first.

use sea_orm_migration::prelude::*;

use crate::statement;

/// Each item with no area, as `project id`.
const UNPLACED: &str = "SELECT project || ' ' || id FROM items WHERE area_id IS NULL \
    ORDER BY project, key, num";

const UP: &str = "ALTER TABLE items ALTER COLUMN area_id SET NOT NULL";

const DOWN: &str = "ALTER TABLE items ALTER COLUMN area_id DROP NOT NULL";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        let mut unplaced = Vec::new();
        for r in c.query_all_raw(statement(UNPLACED, vec![])).await? {
            unplaced.push(r.try_get_by_index::<String>(0)?);
        }
        if !unplaced.is_empty() {
            return Err(DbErr::Migration(format!(
                "{} hold no area: place each before the column is made required: {}",
                unplaced.len(),
                unplaced.join(", ")
            )));
        }
        c.execute_unprepared(UP).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(DOWN).await?;
        Ok(())
    }
}
