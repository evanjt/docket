//! Dependencies as rows: an item may depend on any number of items. Each wait on an item becomes one
//! row, except the wait that would close a cycle of holds, which is kept as a `related` link and
//! cleared. A plan is held by each open item it opened; the waits are taken in key and number order.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

use crate::statement;

const UP: &str = r#"
CREATE TABLE dependencies (
  rid BIGINT NOT NULL REFERENCES items(rid) ON DELETE CASCADE,
  on_rid BIGINT NOT NULL REFERENCES items(rid) ON DELETE CASCADE,
  created_at TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (rid, on_rid),
  CHECK (rid <> on_rid)
);

CREATE INDEX dependencies_on ON dependencies(on_rid);
"#;

const DOWN: &str = "DROP TABLE IF EXISTS dependencies;";

/// Each open plan and the open items it opened.
const GATES: &str = "SELECT l.to_rid, l.rid FROM links l \
    JOIN items p ON p.rid=l.to_rid JOIN items m ON m.rid=l.rid JOIN projects j ON j.slug=p.project \
    WHERE l.kind='opened' AND p.state='open' AND m.state='open' \
      AND EXISTS (SELECT 1 FROM jsonb_array_elements(j.keys) k WHERE k->>'key'=p.key AND k->>'kind'='audit') \
    ORDER BY l.to_rid, l.rid";

/// Each open item's wait on another, in key and number order.
const WAITS: &str = "SELECT rid, wait_item, wait_since FROM items \
    WHERE state='open' AND wait_on='item' AND wait_item IS NOT NULL ORDER BY project, key, num";

/// Whether `to` is reached from `from` along the holds.
fn reaches(holds: &BTreeMap<i64, Vec<i64>>, from: i64, to: i64) -> bool {
    let mut todo = VecDeque::from([from]);
    let mut seen = BTreeSet::from([from]);
    while let Some(x) = todo.pop_front() {
        if x == to {
            return true;
        }
        for n in holds.get(&x).into_iter().flatten() {
            if seen.insert(*n) {
                todo.push_back(*n);
            }
        }
    }
    false
}

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        c.execute_unprepared(UP).await?;
        let mut holds: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        for r in c.query_all_raw(statement(GATES, vec![])).await? {
            let (plan, member): (i64, i64) = (r.try_get_by_index(0)?, r.try_get_by_index(1)?);
            holds.entry(plan).or_default().push(member);
        }
        for r in c.query_all_raw(statement(WAITS, vec![])).await? {
            let (rid, on): (i64, i64) = (r.try_get_by_index(0)?, r.try_get_by_index(1)?);
            let since: String = r.try_get_by_index(2)?;
            if reaches(&holds, on, rid) {
                for (a, b) in [(rid, on), (on, rid)] {
                    c.execute_raw(statement(
                        "INSERT INTO links (rid, kind, to_rid) SELECT ?, 'related', ? WHERE NOT EXISTS \
                         (SELECT 1 FROM links WHERE rid=? AND kind='related' AND to_rid=?)",
                        vec![a.into(), b.into(), a.into(), b.into()],
                    ))
                    .await?;
                }
                c.execute_raw(statement(
                    "UPDATE items SET wait_on=NULL, wait_item=NULL, wait_ref=NULL, wait_since=NULL \
                     WHERE rid=?",
                    vec![rid.into()],
                ))
                .await?;
                continue;
            }
            holds.entry(rid).or_default().push(on);
            c.execute_raw(statement(
                "INSERT INTO dependencies (rid, on_rid, created_at) VALUES (?, ?, ?)",
                vec![rid.into(), on.into(), since.into()],
            ))
            .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(DOWN).await?;
        Ok(())
    }
}
