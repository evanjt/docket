//! An assignment's assignee is the agent or the owner, as `assignment::ASSIGNEES` lists them.

use docket_core::assignment::ASSIGNEES;
use sea_orm::Statement;
use sea_orm_migration::prelude::*;

const DOWN: &str = "ALTER TABLE assignments DROP CONSTRAINT IF EXISTS assignments_assignee_check;";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        let list = ASSIGNEES.map(|a| format!("'{a}'")).join(", ");
        let outside = c
            .query_one_raw(Statement::from_string(
                manager.get_database_backend(),
                format!("SELECT COUNT(*) FROM assignments WHERE assignee NOT IN ({list})"),
            ))
            .await?
            .map(|r| r.try_get_by_index::<i64>(0))
            .transpose()?
            .unwrap_or(0);
        if outside > 0 {
            return Err(DbErr::Migration(format!(
                "{outside} assignments have an assignee outside {list}"
            )));
        }
        c.execute_unprepared(&format!(
            "ALTER TABLE assignments ADD CONSTRAINT assignments_assignee_check CHECK (assignee IN ({list}));"
        ))
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(DOWN).await?;
        Ok(())
    }
}
