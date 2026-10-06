//! `GET /dump`: the rows the dump repository needs after a cursor, read in one snapshot.
//!
//! The cursor is the last event `seq` written. Writes commit one at a time (`Tx::begin`), so no event
//! below the cursor becomes visible after a page is read. An item a write changes without an event of
//! its own links to or waits on one with an event in that write, so the items after a cursor are those
//! with an event and their neighbours. Every project is sent, since a project write logs no event.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Query, State};
use sea_orm::{
    AccessMode, ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait,
    FromQueryResult, IsolationLevel, QueryOrder, TransactionTrait, Value,
};
use serde::Deserialize;

use docket_core::dump::{Dependency, DumpPage, EventDump, ItemDump, ProjectDump};
use docket_core::jsontext;

use crate::entities::{item, project};
use crate::reads::public::{Failure, internal, sql};

#[derive(Deserialize)]
pub struct Since {
    #[serde(default)]
    since: i64,
}

/// The rids changed after one seq and up to another: those with an event, and their neighbours.
const CHANGED: &str = "SELECT rid FROM events WHERE seq > ? AND seq <= ? AND rid IS NOT NULL \
     UNION SELECT l.rid FROM links l JOIN events e ON l.to_rid = e.rid \
       WHERE e.seq > ? AND e.seq <= ? AND l.kind IN ('related', 'origin') \
     UNION SELECT i.rid FROM items i JOIN events e ON i.parent_rid = e.rid \
       WHERE e.seq > ? AND e.seq <= ? \
     UNION SELECT d.rid FROM dependencies d JOIN items o ON o.rid = d.on_rid \
       JOIN events e ON e.rid = o.rid OR e.rid = o.superseded_by \
       WHERE e.seq > ? AND e.seq <= ?";

#[derive(FromQueryResult)]
struct EventRow {
    project: String,
    uid: String,
    at: String,
    host: String,
    branch: Option<String>,
    kind: String,
    note: Option<String>,
    item: Option<String>,
    data: Option<serde_json::Value>,
}

#[derive(FromQueryResult)]
struct LinkRow {
    rid: i64,
    kind: String,
    id: String,
}

/// Which items a page carries: every one, or those changed in `(since, cursor]`.
struct Scope {
    full: bool,
    since: i64,
    cursor: i64,
}

impl Scope {
    /// A `SELECT rid` for the items, with its bound values.
    fn rids(&self) -> (String, Vec<Value>) {
        if self.full {
            return ("SELECT rid FROM items".to_string(), vec![]);
        }
        let bounds = [self.since, self.cursor];
        let values = bounds.iter().cycle().take(8).map(|v| (*v).into()).collect();
        (CHANGED.to_string(), values)
    }
}

/// Everything changed after `since`, or every row when `since` is 0, with the cursor to ask from next.
///
/// # Errors
/// 500 when the database fails.
pub async fn dump(
    State(db): State<DatabaseConnection>,
    Query(q): Query<Since>,
) -> Result<Json<DumpPage>, Failure> {
    page(&db, q.since).await.map(Json).map_err(|e| internal(&e))
}

async fn page(db: &DatabaseConnection, since: i64) -> Result<DumpPage, DbErr> {
    let tx = snapshot(db).await?;
    let cursor: i64 = crate::store::scalar(&tx, "SELECT COALESCE(MAX(seq), 0) FROM events", vec![])
        .await?
        .unwrap_or(0);
    let scope = Scope {
        full: since <= 0,
        since,
        cursor,
    };
    let page = DumpPage {
        cursor,
        full: scope.full,
        projects: projects(&tx).await?,
        items: items(&tx, &scope).await?,
        events: events(&tx, &scope).await?,
    };
    tx.rollback().await?;
    Ok(page)
}

/// A read transaction that sees one snapshot for all its queries.
async fn snapshot(db: &DatabaseConnection) -> Result<DatabaseTransaction, DbErr> {
    let level = Some(IsolationLevel::RepeatableRead);
    db.begin_with_config(level, Some(AccessMode::ReadOnly))
        .await
}

async fn projects(tx: &DatabaseTransaction) -> Result<Vec<ProjectDump>, DbErr> {
    let rows = project::Entity::find()
        .order_by_asc(project::Column::Slug)
        .all(tx)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for p in rows {
        let releases = crate::verbs::releases::listed(tx, &p.slug).await?.all();
        let areas = crate::verbs::areas::listed(tx, &p.slug).await?.all();
        let labels = crate::verbs::labels::listed(tx, &p.slug).await?;
        let publications = crate::publications::listed(tx, &p.slug)
            .await
            .map_err(|e| DbErr::Custom(format!("{}: {e:?}", p.slug)))?
            .publications;
        out.push(ProjectDump {
            slug: p.slug,
            remotes: p.remotes,
            cite_roots: p.cite_roots,
            repos: p.repos,
            fleet_repo: p.fleet_repo,
            integration_ref: p.integration_ref,
            worktree_hint: p.worktree_hint,
            test_hint: p.test_hint,
            skills: p.skills,
            created_at: p.created_at,
            updated_at: p.updated_at,
            releases,
            areas,
            labels,
            publications,
        });
    }
    Ok(out)
}

async fn items(tx: &DatabaseTransaction, scope: &Scope) -> Result<Vec<ItemDump>, DbErr> {
    let (rids, values) = scope.rids();
    let text = format!(
        "SELECT i.*, s.id AS superseded_id, r.name AS release_name, a.name AS area_name, p.id AS parent_id \
         FROM items i LEFT JOIN items s ON s.rid = i.superseded_by \
         LEFT JOIN releases r ON r.id = i.release_id LEFT JOIN areas a ON a.id = i.area_id \
         LEFT JOIN items p ON p.rid = i.parent_rid \
         WHERE i.rid IN ({rids}) ORDER BY i.project, i.key, i.num"
    );
    let rows = tx.query_all_raw(sql(&text, values.clone())).await?;
    let mut links = links(tx, &rids, values.clone()).await?;
    let mut labels = labels(tx, &rids, values.clone()).await?;
    let mut depends = depends(tx, &rids, values.clone()).await?;
    let mut attempts = docket_migration::assignments::of_items(tx, &rids, values).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let superseded: Option<String> = row.try_get("", "superseded_id")?;
        let release: Option<String> = row.try_get("", "release_name")?;
        let area: Option<String> = row.try_get("", "area_name")?;
        let parent: Option<String> = row.try_get("", "parent_id")?;
        let r = item::Model::from_query_result(&row, "")?;
        let (related, origin) = links.remove(&r.rid).unwrap_or_default();
        out.push(ItemDump {
            depends: depends.remove(&r.rid),
            labels: labels.remove(&r.rid),
            assignments: attempts.remove(&r.rid),
            origin: (!origin.is_empty()).then_some(origin),
            project: r.project,
            id: r.id,
            title: r.title,
            state: r.state,
            decision: r.decision,
            decided_at: r.decided_at,
            resolution: r.resolution,
            superseded_by: superseded,
            complexity: r.complexity,
            release,
            area,
            item_type: r.item_type,
            priority: r.priority,
            related,
            parent,
            opened_at: r.opened_at,
            updated_at: r.updated_at,
            body: r.body,
        });
    }
    Ok(out)
}

/// `{rid: (related ids, origin ids)}` for the items of a scope.
async fn links(
    tx: &DatabaseTransaction,
    rids: &str,
    values: Vec<Value>,
) -> Result<HashMap<i64, (Vec<String>, Vec<String>)>, DbErr> {
    let text = format!(
        "SELECT l.rid, l.kind, t.id FROM links l JOIN items t ON t.rid = l.to_rid \
         WHERE l.kind IN ('related', 'origin') AND l.rid IN ({rids}) ORDER BY l.rid, l.kind, l.to_rid"
    );
    let rows = LinkRow::find_by_statement(sql(&text, values))
        .all(tx)
        .await?;
    let mut out: HashMap<i64, (Vec<String>, Vec<String>)> = HashMap::new();
    for l in rows {
        let entry = out.entry(l.rid).or_default();
        if l.kind == "related" {
            entry.0.push(l.id);
        } else {
            entry.1.push(l.id);
        }
    }
    Ok(out)
}

/// `{rid: the names of the labels it was given}` for the items of a scope.
async fn labels(
    tx: &DatabaseTransaction,
    rids: &str,
    values: Vec<Value>,
) -> Result<HashMap<i64, Vec<String>>, DbErr> {
    let text = format!(
        "SELECT il.rid, l.name FROM item_labels il JOIN labels l ON l.id = il.label_id \
         WHERE il.rid IN ({rids}) ORDER BY il.rid, l.name"
    );
    let mut out: HashMap<i64, Vec<String>> = HashMap::new();
    for r in tx.query_all_raw(sql(&text, values)).await? {
        out.entry(r.try_get_by_index(0)?)
            .or_default()
            .push(r.try_get_by_index(1)?);
    }
    Ok(out)
}

/// `{rid: what it depends on}` for the items of a scope, oldest first.
async fn depends(
    tx: &DatabaseTransaction,
    rids: &str,
    values: Vec<Value>,
) -> Result<HashMap<i64, Vec<Dependency>>, DbErr> {
    let text = format!(
        "SELECT d.rid, t.id, d.created_at FROM dependencies d JOIN items t ON t.rid = d.on_rid \
         WHERE d.rid IN ({rids}) ORDER BY d.rid, d.created_at, t.key, t.num"
    );
    let mut out: HashMap<i64, Vec<Dependency>> = HashMap::new();
    for r in tx.query_all_raw(sql(&text, values)).await? {
        out.entry(r.try_get_by_index(0)?)
            .or_default()
            .push(Dependency {
                on: r.try_get_by_index(1)?,
                created_at: r.try_get_by_index(2)?,
            });
    }
    Ok(out)
}

async fn events(tx: &DatabaseTransaction, scope: &Scope) -> Result<Vec<EventDump>, DbErr> {
    let since = if scope.full { 0 } else { scope.since };
    let text = "SELECT e.project, e.uid, e.at, e.host, e.branch, e.kind, e.note, e.data, i.id AS item \
                FROM events e LEFT JOIN items i ON i.rid = e.rid WHERE e.seq > ? AND e.seq <= ? \
                ORDER BY e.seq";
    let rows = EventRow::find_by_statement(sql(text, vec![since.into(), scope.cursor.into()]))
        .all(tx)
        .await?;
    Ok(rows
        .into_iter()
        .map(|e| EventDump {
            project: e.project,
            uid: e.uid,
            at: e.at,
            host: e.host,
            branch: e.branch,
            kind: e.kind,
            note: e.note,
            item: e.item,
            data: e.data.map(|d| jsontext::dumps(&d, true)),
        })
        .collect())
}

#[cfg(test)]
#[path = "../tests/dump.rs"]
mod tests;
