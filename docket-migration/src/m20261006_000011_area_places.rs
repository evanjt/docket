//! Each concept item becomes an area row and is dropped, and each item takes its area as the move onto
//! the core plans it (`docket_core::migrate::plan`): its nearest plan's up the parent edges, else its
//! own concept's, the oldest tie when it has several. An item neither places keeps no area here: the
//! placements step after it places the rest. An item that carries an area already keeps it. Every
//! item whose file changes is left for the dump.

use std::collections::BTreeMap;

use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

use docket_core::area::Area;
use docket_core::dump::{EventDump, ItemDump, ProjectDump};
use docket_core::migrate::{Change, Rows, Rules, plan};

use crate::statement;

/// Every project with a concept key.
const PROJECTS: &str = "SELECT slug, keys, themes, skills FROM projects \
    WHERE keys @> '[{\"kind\": \"concept\"}]' ORDER BY slug";

pub(crate) const AREAS: &str =
    "SELECT id, name, description, position FROM areas WHERE project=? ORDER BY position";

/// A project's items, with the fields the plan reads and the id of the parent.
const ITEMS: &str = "SELECT i.rid, i.id, i.title, i.state, i.theme, i.group_name, i.tags, i.body, \
    i.opened_at, i.updated_at, p.id FROM items i LEFT JOIN items p ON p.rid=i.parent_rid \
    WHERE i.project=? ORDER BY i.rid";

const RELATED: &str = "SELECT l.rid, t.id FROM links l JOIN items i ON i.rid=l.rid \
    JOIN items t ON t.rid=l.to_rid WHERE i.project=? AND l.kind='related' ORDER BY l.id";

/// The events that made and undid each `related` tie, which date it.
const LINKED: &str = "SELECT e.uid, e.at, e.note, i.id FROM events e JOIN items i ON i.rid=e.rid \
    WHERE e.project=? AND e.kind='edited' \
    AND (e.note LIKE 'link related %' OR e.note LIKE 'unlink related %') ORDER BY e.seq";

/// A project's areas into `project`, and its items with their `related` ties and the events that
/// date them, with each item's row id.
pub(crate) async fn project_rows<C: ConnectionTrait>(
    c: &C,
    project: &mut ProjectDump,
) -> Result<(Vec<ItemDump>, Vec<EventDump>, BTreeMap<String, i64>), DbErr> {
    let slug = project.slug.clone();
    for r in c
        .query_all_raw(statement(AREAS, vec![slug.clone().into()]))
        .await?
    {
        project.areas.push(Area {
            name: r.try_get_by_index(1)?,
            description: r.try_get_by_index(2)?,
            position: r.try_get_by_index(3)?,
            priority: None,
            history: false,
        });
    }
    let mut items: BTreeMap<i64, ItemDump> = BTreeMap::new();
    let mut rids = BTreeMap::new();
    for r in c
        .query_all_raw(statement(ITEMS, vec![slug.clone().into()]))
        .await?
    {
        let rid: i64 = r.try_get_by_index(0)?;
        let tags: serde_json::Value = r.try_get_by_index(6)?;
        let item = ItemDump {
            project: slug.clone(),
            id: r.try_get_by_index(1)?,
            title: r.try_get_by_index(2)?,
            state: r.try_get_by_index(3)?,
            theme: r.try_get_by_index(4)?,
            group: r.try_get_by_index(5)?,
            tags: serde_json::from_value(tags).unwrap_or_default(),
            body: r.try_get_by_index(7)?,
            opened_at: r.try_get_by_index(8)?,
            updated_at: r.try_get_by_index(9)?,
            parent: r.try_get_by_index(10)?,
            ..ItemDump::default()
        };
        rids.insert(item.id.clone(), rid);
        items.insert(rid, item);
    }
    for r in c
        .query_all_raw(statement(RELATED, vec![slug.clone().into()]))
        .await?
    {
        let rid: i64 = r.try_get_by_index(0)?;
        if let Some(i) = items.get_mut(&rid) {
            i.related.push(r.try_get_by_index(1)?);
        }
    }
    let mut events = Vec::new();
    for r in c
        .query_all_raw(statement(LINKED, vec![slug.clone().into()]))
        .await?
    {
        events.push(EventDump {
            project: slug.clone(),
            uid: r.try_get_by_index(0)?,
            at: r.try_get_by_index(1)?,
            kind: "edited".to_string(),
            note: r.try_get_by_index(2)?,
            item: r.try_get_by_index(3)?,
            ..EventDump::default()
        });
    }
    Ok((items.into_values().collect(), events, rids))
}

pub(crate) async fn pending<C: ConnectionTrait>(c: &C, slug: &str, rid: i64) -> Result<(), DbErr> {
    c.execute_raw(statement(
        "INSERT INTO pending_dump (rid, project) VALUES (?, ?) ON CONFLICT DO NOTHING",
        vec![rid.into(), slug.into()],
    ))
    .await?;
    Ok(())
}

/// An open standing item dropped as the `what` (`area` or `label`) it became, with the event that
/// says so.
pub(crate) async fn drop_concept<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    rid: i64,
    what: &str,
    became: &str,
    now: &str,
) -> Result<(), DbErr> {
    let why = format!("became {what} {became}");
    let dropped = c
        .execute_raw(statement(
            "UPDATE items SET state='dropped', turn=NULL, turn_note=NULL, asked_at=NULL, \
             resolution=?, claim_branch=NULL, claim_host=NULL, claim_since=NULL, \
             claim_runner=NULL, claim_job=NULL, claim_on=NULL, wait_on=NULL, wait_item=NULL, \
             wait_ref=NULL, wait_since=NULL, updated_at=? WHERE rid=? AND state='open'",
            vec![why.clone().into(), now.into(), rid.into()],
        ))
        .await?
        .rows_affected();
    if dropped > 0 {
        c.execute_raw(statement(
            "INSERT INTO events (uid, project, rid, at, host, branch, kind, note) \
             VALUES (?, ?, ?, ?, 'migration', NULL, 'dropped', ?)",
            vec![
                format!("{now}-migration-{what}-{rid}").into(),
                slug.into(),
                rid.into(),
                now.into(),
                why.into(),
            ],
        ))
        .await?;
    }
    Ok(())
}

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let c = manager.get_connection();
        let now = docket_core::clock::now();
        for p in c.query_all_raw(statement(PROJECTS, vec![])).await? {
            let mut project = ProjectDump {
                slug: p.try_get_by_index(0)?,
                keys: p.try_get_by_index(1)?,
                themes: p.try_get_by_index(2)?,
                skills: p.try_get_by_index(3)?,
                ..ProjectDump::default()
            };
            let (items, events, rids) = project_rows(c, &mut project).await?;
            let slug = project.slug.clone();
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
                ids.insert(r.try_get_by_index(1)?, r.try_get_by_index(0)?);
            }
            for a in changes.areas.iter().filter(|a| a.concept.is_some()) {
                let id: i64 = c
                    .query_one_raw(statement(
                        "INSERT INTO areas (project, name, description, position) \
                         VALUES (?, ?, ?, ?) RETURNING id",
                        vec![
                            slug.clone().into(),
                            a.name.clone().into(),
                            a.description.clone().into(),
                            <i64 as TryFrom<usize>>::try_from(a.position)
                                .unwrap_or(i64::MAX)
                                .into(),
                        ],
                    ))
                    .await?
                    .ok_or(DbErr::RecordNotInserted)?
                    .try_get_by_index(0)?;
                ids.insert(a.name.clone(), id);
            }
            for change in &changes.changes {
                match change {
                    Change::InArea { id, area } => {
                        let (Some(rid), Some(to)) = (rids.get(id), ids.get(area)) else {
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
                    Change::BecomesArea { id, area } => {
                        if let Some(rid) = rids.get(id) {
                            drop_concept(c, &slug, *rid, "area", area, &now).await?;
                            pending(c, &slug, *rid).await?;
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// The areas and the drops stay: the step that made `areas` removes them with the column.
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
