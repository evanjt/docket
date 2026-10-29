//! The word a job's end left on its claim: the first word of its report, so every read that makes a
//! status word tells a finished build from a running or a failed one.

use sea_orm_migration::prelude::*;

const UP: &str = r#"
ALTER TABLE assignments ADD COLUMN job_report TEXT COLLATE "C";
"#;

const DOWN: &str = r"
ALTER TABLE assignments DROP COLUMN IF EXISTS job_report;
";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        c.execute_unprepared(UP).await?;
        crate::assignments::fill_reports(c).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(DOWN).await?;
        Ok(())
    }
}
