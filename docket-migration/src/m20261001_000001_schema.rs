//! The first schema: projects, items, events, links, the search index and the local tables.

use sea_orm_migration::prelude::*;

const UP: &str = include_str!("schema.sql");

const DOWN: &str = "DROP TABLE IF EXISTS meta, chores, pending_dump, search, links, events, items, \
                    roots, projects";

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
