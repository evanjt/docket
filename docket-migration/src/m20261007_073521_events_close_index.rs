//! The metrics read each item's last close or drop from `events` alone, by project and kind.

use sea_orm_migration::prelude::*;

const UP: &str = r"
CREATE INDEX events_project_kind ON events(project, kind, rid, at);
";

const DOWN: &str = r"
DROP INDEX events_project_kind;
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
