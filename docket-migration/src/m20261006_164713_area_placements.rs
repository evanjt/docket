//! Each item the concept step left with no area takes it as the move onto the core plans it
//! (`docket_core::migrate::plan`): the area of its latest placement by an agent (`decide --area`), as
//! written, a plan passing its area down to its children; a closed item nothing places goes to
//! `unsorted`, made only in a project that has one and marked `history`. An area a placement names
//! and the project lacks is a new row, its `about` the description. An open item nothing places is
//! left with no area, which the dry run lists. Only items with no area are written, and every item
//! whose file changes is left for the dump.

use std::collections::BTreeMap;

use sea_orm_migration::prelude::*;

use docket_core::dump::EventDump;
use docket_core::migrate::{Change, OldProject, Rows, Rules, plan};

use crate::m20261006_000011_area_places::{AREAS, pending, project_rows};
use crate::statement;

const UP: &str = "ALTER TABLE areas ADD COLUMN history BOOLEAN NOT NULL DEFAULT false";

const DOWN: &str = "ALTER TABLE areas DROP COLUMN IF EXISTS history";

const PROJECTS: &str = "SELECT slug, keys, themes, skills FROM projects ORDER BY slug";

/// The decisions recorded on a project's items, which carry the agents' placements.
const DECIDED: &str = "SELECT e.uid, e.at, e.data, i.id FROM events e JOIN items i ON i.rid=e.rid \
    WHERE e.project=? AND e.kind='decided' AND e.data IS NOT NULL ORDER BY e.seq";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        c.execute_unprepared(UP).await?;
        for p in c.query_all_raw(statement(PROJECTS, vec![])).await? {
            let mut project = OldProject {
                slug: p.try_get_by_index(0)?,
                keys: p.try_get_by_index(1)?,
                themes: p.try_get_by_index(2)?,
                skills: p.try_get_by_index(3)?,
                ..OldProject::default()
            };
            let (items, mut events, rids) = project_rows(c, &mut project).await?;
            let slug = project.slug.clone();
            for r in c
                .query_all_raw(statement(DECIDED, vec![slug.clone().into()]))
                .await?
            {
                let data: serde_json::Value = r.try_get_by_index(2)?;
                events.push(EventDump {
                    project: slug.clone(),
                    uid: r.try_get_by_index(0)?,
                    at: r.try_get_by_index(1)?,
                    kind: "decided".to_string(),
                    data: Some(data.to_string()),
                    item: r.try_get_by_index(3)?,
                    ..EventDump::default()
                });
            }
            let rows = Rows {
                project: &project,
                items: items.iter().collect(),
                events: events.iter().collect(),
            };
            let changes = plan(&rows, Rules::default());
            let mut ids: BTreeMap<String, i64> = BTreeMap::new();
            for r in c
                .query_all_raw(statement(AREAS, vec![slug.clone().into()]))
                .await?
            {
                let name: String = r.try_get_by_index(1)?;
                ids.insert(name.to_lowercase(), r.try_get_by_index(0)?);
            }
            for a in &changes.areas {
                if ids.contains_key(&a.name.to_lowercase()) {
                    continue;
                }
                let about = Some(a.description.clone()).filter(|d| !d.is_empty());
                let id: i64 = c
                    .query_one_raw(statement(
                        "INSERT INTO areas (project, name, description, position, history) \
                         VALUES (?, ?, ?, ?, ?) RETURNING id",
                        vec![
                            slug.clone().into(),
                            a.name.clone().into(),
                            about.into(),
                            <i64 as TryFrom<usize>>::try_from(a.position)
                                .unwrap_or(i64::MAX)
                                .into(),
                            a.history.into(),
                        ],
                    ))
                    .await?
                    .ok_or(DbErr::RecordNotInserted)?
                    .try_get_by_index(0)?;
                ids.insert(a.name.to_lowercase(), id);
            }
            for change in &changes.changes {
                let Change::InArea { id, area } = change else {
                    continue;
                };
                let (Some(rid), Some(to)) = (rids.get(id), ids.get(&area.to_lowercase())) else {
                    continue;
                };
                let set = c
                    .execute_raw(statement(
                        "UPDATE items SET area_id=? WHERE rid=? AND area_id IS NULL",
                        vec![(*to).into(), (*rid).into()],
                    ))
                    .await?
                    .rows_affected();
                if set > 0 {
                    pending(c, &slug, *rid).await?;
                }
            }
        }
        Ok(())
    }

    /// The flag goes; the areas stay, and the step that made `areas` removes them with the table.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(DOWN).await?;
        Ok(())
    }
}
