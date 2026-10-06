//! The claim and the owner's turn are the item's open assignment row. Each item's open row is made
//! to agree with the claim and turn columns, which were the store until now, and the checks that
//! tie those columns to the item's state go, so nothing writes them again until they are dropped.

use sea_orm_migration::prelude::*;

/// Every check on `items` that reads a claim or turn column.
const UP: &str = r"
DO $$
DECLARE c record;
BEGIN
  FOR c IN SELECT conname FROM pg_constraint
    WHERE conrelid = 'items'::regclass AND contype = 'c'
      AND (pg_get_constraintdef(oid) LIKE '%claim\_%' OR pg_get_constraintdef(oid) ~ '\mturn\M')
  LOOP
    EXECUTE format('ALTER TABLE items DROP CONSTRAINT %I', c.conname);
  END LOOP;
END $$;
";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        crate::assignments::agree_all(c).await?;
        c.execute_unprepared(UP).await?;
        Ok(())
    }

    /// The checks stay dropped: the columns no longer follow the open rows, so a row would fail them.
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
