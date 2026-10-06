//! Every item holds an area. Each item still holding none is placed first in its project's history
//! area `unsorted`, made when the project has none by that name, with an event that names the move;
//! then the column refuses a row with none.

use std::collections::BTreeMap;

use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

use docket_core::migrate::{UNSORTED, UNSORTED_ABOUT};

use crate::m20261006_000011_area_places::pending;
use crate::statement;

/// Each item with no area: its row id and its project.
const UNPLACED: &str = "SELECT rid, project FROM items WHERE area_id IS NULL ORDER BY project, rid";

/// The project's area by the name `unsorted` in any case.
const SORTED: &str = "SELECT id FROM areas WHERE project=? AND lower(name)=?";

/// A new history area after the project's last.
const MAKE: &str = "INSERT INTO areas (project, name, description, position, history) \
    VALUES (?, ?, ?, (SELECT COALESCE(MAX(position) + 1, 0) FROM areas WHERE project=?), true) \
    RETURNING id";

const UP: &str = "ALTER TABLE items ALTER COLUMN area_id SET NOT NULL";

const DOWN: &str = "ALTER TABLE items ALTER COLUMN area_id DROP NOT NULL";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        let now = docket_core::clock::now();
        let mut areas: BTreeMap<String, i64> = BTreeMap::new();
        for r in c.query_all_raw(statement(UNPLACED, vec![])).await? {
            let rid: i64 = r.try_get_by_index(0)?;
            let project: String = r.try_get_by_index(1)?;
            let area = if let Some(id) = areas.get(&project) {
                *id
            } else {
                let found = c
                    .query_one_raw(statement(
                        SORTED,
                        vec![project.clone().into(), UNSORTED.into()],
                    ))
                    .await?;
                let id: i64 = match found {
                    Some(row) => row.try_get_by_index(0)?,
                    None => c
                        .query_one_raw(statement(
                            MAKE,
                            vec![
                                project.clone().into(),
                                UNSORTED.into(),
                                UNSORTED_ABOUT.into(),
                                project.clone().into(),
                            ],
                        ))
                        .await?
                        .ok_or(DbErr::RecordNotInserted)?
                        .try_get_by_index(0)?,
                };
                areas.insert(project.clone(), id);
                id
            };
            c.execute_raw(statement(
                "UPDATE items SET area_id=? WHERE rid=?",
                vec![area.into(), rid.into()],
            ))
            .await?;
            c.execute_raw(statement(
                "INSERT INTO events (uid, project, rid, at, host, branch, kind, note) \
                 VALUES (?, ?, ?, ?, 'migration', NULL, 'edited', ?)",
                vec![
                    format!("{now}-migration-unsorted-{rid}").into(),
                    project.clone().into(),
                    rid.into(),
                    now.clone().into(),
                    format!("placed in {UNSORTED}: it held no area").into(),
                ],
            ))
            .await?;
            pending(c, &project, rid).await?;
        }
        c.execute_unprepared(UP).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(DOWN).await?;
        Ok(())
    }
}
