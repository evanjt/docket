//! A condition wait already stored becomes a task on the owner's turn and a dependency on it, as a
//! new `wait --until` does. A plan's gate stays as it is. The task is titled with the condition,
//! takes the waiter's theme, opens at the moment the wait began and is related to the waiter both
//! ways; it takes the project's `T` key, or its first work key. A project with no work key keeps its
//! condition waits as they are.

use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

use docket_core::rules::GATE;

use crate::statement;

/// Each open item waiting on a condition other than a plan's gate, in key and number order, with the
/// work keys of its project.
const WAITS: &str = "SELECT i.rid, i.project, i.title, i.theme, i.wait_ref, i.wait_since, \
    (SELECT COALESCE(array_agg(k->>'key' ORDER BY (k->>'key' <> 'T'), ord), '{}') \
       FROM jsonb_array_elements(j.keys) WITH ORDINALITY AS e(k, ord) WHERE k->>'kind'='work') \
    FROM items i JOIN projects j ON j.slug=i.project \
    WHERE i.state='open' AND i.wait_on='condition' AND i.wait_ref IS NOT NULL AND i.wait_ref <> ? \
    ORDER BY i.project, i.key, i.num";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        let mut waiters = Vec::new();
        for r in c.query_all_raw(statement(WAITS, vec![GATE.into()])).await? {
            let keys: Vec<String> = r.try_get_by_index(6)?;
            let Some(key) = keys.into_iter().next() else {
                continue;
            };
            waiters.push((
                r.try_get_by_index::<i64>(0)?,
                r.try_get_by_index::<String>(1)?,
                r.try_get_by_index::<String>(2)?,
                r.try_get_by_index::<Option<String>>(3)?,
                r.try_get_by_index::<String>(4)?,
                r.try_get_by_index::<String>(5)?,
                key,
            ));
        }
        for (rid, project, waiter, theme, condition, since, key) in waiters {
            let body = format!(
                "{waiter} waits until this holds. Close it when it does, or drop it if it no longer matters."
            );
            let task = c
                .query_one_raw(statement(
                    "INSERT INTO items (project, key, num, title, state, turn, turn_note, asked_at, body, \
                     theme, opened_at, updated_at) \
                     VALUES (?, ?, (SELECT COALESCE(MAX(num), 0) + 1 FROM items WHERE project=? AND key=?), \
                     ?, 'open', 'user', ?, ?, ?, ?, ?, ?) RETURNING rid, id",
                    vec![
                        project.clone().into(),
                        key.clone().into(),
                        project.clone().into(),
                        key.into(),
                        condition.clone().into(),
                        condition.clone().into(),
                        since.clone().into(),
                        body.clone().into(),
                        theme.into(),
                        since.clone().into(),
                        since.clone().into(),
                    ],
                ))
                .await?
                .ok_or(DbErr::RecordNotInserted)?;
            let (new_rid, label): (i64, String) =
                (task.try_get_by_index(0)?, task.try_get_by_index(1)?);
            c.execute_raw(statement(
                "INSERT INTO events (uid, project, rid, at, host, branch, kind, note) \
                 VALUES (?, ?, ?, ?, 'migration', NULL, 'opened', ?)",
                vec![
                    format!("{since}-migration-{new_rid}").into(),
                    project.clone().into(),
                    new_rid.into(),
                    since.clone().into(),
                    condition.chars().take(120).collect::<String>().into(),
                ],
            ))
            .await?;
            c.execute_raw(statement(
                "INSERT INTO search (rid, id, title, body, files) VALUES (?, ?, ?, ?, '')",
                vec![new_rid.into(), label.into(), condition.into(), body.into()],
            ))
            .await?;
            for (a, b) in [(rid, new_rid), (new_rid, rid)] {
                c.execute_raw(statement(
                    "INSERT INTO links (rid, kind, to_rid) VALUES (?, 'related', ?)",
                    vec![a.into(), b.into()],
                ))
                .await?;
            }
            c.execute_raw(statement(
                "INSERT INTO dependencies (rid, on_rid, created_at) VALUES (?, ?, ?)",
                vec![rid.into(), new_rid.into(), since.into()],
            ))
            .await?;
            c.execute_raw(statement(
                "UPDATE items SET wait_on='item', wait_item=?, wait_ref=(SELECT id FROM items WHERE rid=?) \
                 WHERE rid=?",
                vec![new_rid.into(), new_rid.into(), rid.into()],
            ))
            .await?;
        }
        crate::assignments::rebuild_all(c).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
