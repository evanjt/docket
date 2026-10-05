//! A plan no longer holds a wait while anything under it is open: its state is read from its open
//! members. The condition wait a plan stored for that is cleared from each open item that holds it.

use sea_orm_migration::prelude::*;

/// The words a plan's wait stored for the condition it held.
const PLAN_CONDITION: &str = "everything it opened is closed";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_raw(crate::statement(
                "UPDATE items SET wait_on=NULL, wait_item=NULL, wait_ref=NULL, wait_since=NULL \
                 WHERE wait_on='condition' AND wait_ref=?",
                vec![PLAN_CONDITION.into()],
            ))
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
