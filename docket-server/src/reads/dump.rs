//! `GET /dump`: the rows the dump repository needs after a cursor, read in one snapshot.
//!
//! The cursor is the last event `seq` written. An item a write changes without an event of its own
//! links to or waits on one with an event in that write, so the items after a cursor are those with
//! an event and their neighbours. Every project is sent, since setting a key logs no event.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Query, State};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait, FromQueryResult,
    QueryOrder, TransactionTrait, Value,
};
use serde::Deserialize;

use docket_core::dump::{DumpPage, EventDump, ItemDump, ProjectDump};

use crate::entities::{item, project};
use crate::reads::public::{Failure, internal, sql};
use crate::store::to_item;

#[derive(Deserialize)]
pub struct Since {
    #[serde(default)]
    since: i64,
}

/// The rids changed after one seq and up to another: those with an event, and their neighbours.
const CHANGED: &str = "SELECT rid FROM events WHERE seq > ? AND seq <= ? AND rid IS NOT NULL \
     UNION SELECT l.rid FROM links l JOIN events e ON l.to_rid = e.rid \
       WHERE e.seq > ? AND e.seq <= ? AND l.kind IN ('related', 'opened') \
     UNION SELECT i.rid FROM items i JOIN events e ON i.wait_item = e.rid \
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
    data: Option<String>,
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
        let values = bounds.iter().cycle().take(6).map(|v| (*v).into()).collect();
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
    let tx = db.begin().await?;
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

async fn projects(tx: &DatabaseTransaction) -> Result<Vec<ProjectDump>, DbErr> {
    let rows = project::Entity::find()
        .order_by_asc(project::Column::Slug)
        .all(tx)
        .await?;
    Ok(rows
        .into_iter()
        .map(|p| ProjectDump {
            slug: p.slug,
            keys: p.keys,
            remotes: p.remotes,
            themes: p.themes,
            cite_roots: p.cite_roots,
            repos: p.repos,
            fleet_repo: p.fleet_repo,
            integration_ref: p.integration_ref,
            worktree_hint: p.worktree_hint,
            test_hint: p.test_hint,
            skills: p.skills,
            created_at: p.created_at,
            updated_at: p.updated_at,
        })
        .collect())
}

async fn items(tx: &DatabaseTransaction, scope: &Scope) -> Result<Vec<ItemDump>, DbErr> {
    let (rids, values) = scope.rids();
    let text = format!(
        "SELECT i.*, s.id AS superseded_id FROM items i LEFT JOIN items s ON s.rid = i.superseded_by \
         WHERE i.rid IN ({rids}) ORDER BY i.project, i.key, i.num"
    );
    let rows = tx.query_all_raw(sql(&text, values.clone())).await?;
    let mut links = links(tx, &rids, values).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let superseded: Option<String> = row.try_get("", "superseded_id")?;
        let stored = to_item(item::Model::from_query_result(&row, "")?);
        let (related, opened) = links.remove(&stored.rid).unwrap_or_default();
        out.push(item_dump(stored, superseded, related, opened));
    }
    Ok(out)
}

/// `{rid: (related ids, opened ids)}` for the items of a scope.
async fn links(
    tx: &DatabaseTransaction,
    rids: &str,
    values: Vec<Value>,
) -> Result<HashMap<i64, (Vec<String>, Vec<String>)>, DbErr> {
    let text = format!(
        "SELECT l.rid, l.kind, t.id FROM links l JOIN items t ON t.rid = l.to_rid \
         WHERE l.kind IN ('related', 'opened') AND l.rid IN ({rids})"
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

fn item_dump(
    r: docket_core::item::Item,
    superseded_by: Option<String>,
    related: Vec<String>,
    opened: Vec<String>,
) -> ItemDump {
    ItemDump {
        project: r.project,
        id: r.id,
        title: r.title,
        state: r.state,
        turn: r.turn,
        turn_note: r.turn_note,
        asked_at: r.asked_at,
        claim_branch: r.claim_branch,
        claim_host: r.claim_host,
        claim_since: r.claim_since,
        claim_runner: r.claim_runner,
        claim_job: r.claim_job,
        claim_on: r.claim_on,
        wait_on: r.wait_on,
        wait_ref: r.wait_ref,
        wait_since: r.wait_since,
        decision: r.decision,
        decided_at: r.decided_at,
        resolution: r.resolution,
        superseded_by,
        scope: r.scope,
        complexity: r.complexity,
        group: r.group_name,
        theme: r.theme,
        rank: r.rank,
        tags: r.tags,
        related,
        opened,
        opened_at: r.opened_at,
        updated_at: r.updated_at,
        body: r.body,
    }
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
            data: e.data,
        })
        .collect())
}

#[cfg(test)]
#[path = "../tests/dump.rs"]
mod tests;
