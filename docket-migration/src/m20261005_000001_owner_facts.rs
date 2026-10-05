//! The owner-level value of each agent setting, read by every project that does not set its own.

use sea_orm_migration::prelude::*;

const UP: &str = r#"
CREATE TABLE owner_facts (
  key TEXT COLLATE "C" PRIMARY KEY,
  value TEXT COLLATE "C" NOT NULL,
  updated_at TEXT COLLATE "C" NOT NULL
);
"#;

const DOWN: &str = "DROP TABLE IF EXISTS owner_facts;";

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
