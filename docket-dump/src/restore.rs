//! A Postgres database rebuilt from a dump checkout, for disaster recovery: into an empty one only.

use std::path::Path;

use sea_orm::{
    ConnectionTrait, Database, DatabaseConnection, DatabaseTransaction, TransactionTrait, Value,
};
use serde_json::{Value as Json, json};

use docket_core::dump::{ItemDump, events_path, item_from, item_path, parse_item, project_path};
use docket_core::text::{citations, split_id};
use docket_migration::statement;

use crate::tree;

const LIST_COLUMNS: [&str; 5] = ["keys", "remotes", "themes", "cite_roots", "repos"];
const TEXT_COLUMNS: [&str; 6] = [
    "fleet_repo",
    "integration_ref",
    "worktree_hint",
    "test_hint",
    "created_at",
    "updated_at",
];

/// The rows a restore wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Restored {
    pub projects: usize,
    pub items: usize,
    pub events: usize,
}

/// Every project, item, link and event of the checkout written into the Postgres database at `url`,
/// in one transaction, as the Python restore writes them. The schema is migrated first.
///
/// # Errors
/// The database already holds items, a dump file does not parse, or a reference names an item
/// the checkout does not hold.
pub async fn restore(repo: &Path, url: &str) -> Result<Restored, String> {
    let conn = open(url).await?;
    let held: i64 = scalar(&conn, "SELECT COUNT(*) FROM items", vec![]).await?;
    if held > 0 {
        return Err(format!(
            "the database already holds {held} items; a restore only fills an empty database"
        ));
    }
    let tx = conn.begin().await.map_err(|e| e.to_string())?;
    let mut counts = Restored::default();
    for slug in tree::projects(repo)? {
        restore_project(&tx, repo, &slug, &mut counts).await?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(counts)
}

async fn open(url: &str) -> Result<DatabaseConnection, String> {
    let conn = Database::connect(docket_migration::postgres_url(url))
        .await
        .map_err(|e| format!("the database: {e}"))?;
    docket_migration::migrate(&conn)
        .await
        .map_err(|e| format!("migrating the database: {e}"))?;
    Ok(conn)
}

/// A JSON value bound to a `jsonb` column.
fn json_value(value: Json) -> Value {
    Value::Json(Some(Box::new(value)))
}

async fn exec<C: ConnectionTrait>(c: &C, text: &str, values: Vec<Value>) -> Result<(), String> {
    c.execute_raw(statement(text, values))
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

async fn scalar<C: ConnectionTrait>(c: &C, text: &str, values: Vec<Value>) -> Result<i64, String> {
    let row = c
        .query_one_raw(statement(text, values))
        .await
        .map_err(|e| e.to_string())?
        .ok_or("no row")?;
    row.try_get_by_index(0).map_err(|e| e.to_string())
}

async fn restore_project(
    tx: &DatabaseTransaction,
    repo: &Path,
    slug: &str,
    counts: &mut Restored,
) -> Result<(), String> {
    let text = tree::read(repo, &project_path(slug)).ok_or(format!("{slug}: no project.json"))?;
    let doc: Json = serde_json::from_str(&text).map_err(|e| format!("{slug}/project.json: {e}"))?;
    insert_project(tx, slug, &doc).await?;
    counts.projects += 1;
    let mut parsed = Vec::new();
    for id in tree::items(repo, slug)? {
        let item = read_item(repo, slug, &id)?;
        let rid = insert_item(tx, &item).await?;
        parsed.push((rid, item));
    }
    for (rid, item) in &parsed {
        restore_refs(tx, *rid, item).await?;
    }
    counts.items += parsed.len();
    counts.events += restore_events(tx, repo, slug).await?;
    Ok(())
}

/// The project row: its lists, its text columns and its skills.
async fn insert_project(tx: &DatabaseTransaction, slug: &str, doc: &Json) -> Result<(), String> {
    let mut values: Vec<Value> = vec![slug.into()];
    for k in LIST_COLUMNS {
        let list = doc
            .get(k)
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(json!([]));
        values.push(json_value(list));
    }
    for k in TEXT_COLUMNS {
        values.push(doc[k].as_str().map(str::to_string).into());
    }
    let skills = Some(&doc["skills"])
        .filter(|v| truthy(v))
        .cloned()
        .unwrap_or(json!({}));
    values.push(json_value(skills));
    exec(
        tx,
        "INSERT INTO projects (slug, keys, remotes, themes, cite_roots, repos, fleet_repo, \
         integration_ref, worktree_hint, test_hint, created_at, updated_at, skills) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        values,
    )
    .await
}

fn read_item(repo: &Path, slug: &str, id: &str) -> Result<ItemDump, String> {
    let path = item_path(slug, id);
    let text = tree::read(repo, &path).ok_or(format!("{path}: not readable"))?;
    let (fields, body) = parse_item(&text).map_err(|e| format!("{path}: {e}"))?;
    item_from(slug, fields, body).map_err(|e| format!("{path}: {e}"))
}

/// The first pass: every plain column, the references left for when every row is in.
async fn insert_item(tx: &DatabaseTransaction, i: &ItemDump) -> Result<i64, String> {
    let (key, num) = split_id(&i.id).map_err(|e| e.0)?;
    let conflict = i
        .body
        .lines()
        .any(|l| l.starts_with("<<<<<<< ") || l.starts_with(">>>>>>> "));
    let mut values: Vec<Value> = vec![
        i.project.clone().into(),
        key.into(),
        num.into(),
        i.title.clone().into(),
        i.state.clone().into(),
    ];
    for v in [
        &i.turn,
        &i.turn_note,
        &i.asked_at,
        &i.claim_branch,
        &i.claim_host,
        &i.claim_since,
        &i.claim_runner,
        &i.claim_job,
        &i.claim_on,
        &i.decision,
        &i.decided_at,
        &i.resolution,
        &i.scope,
        &i.complexity,
        &i.theme,
    ] {
        values.push(v.clone().into());
    }
    values.extend([
        i.rank.into(),
        i.opened_at.clone().into(),
        i.updated_at.clone().into(),
        i.group.clone().into(),
        i.body.clone().into(),
        json_value(json!(i.tags)),
        i64::from(conflict).into(),
    ]);
    scalar(
        tx,
        "INSERT INTO items (project, key, num, title, state, turn, turn_note, asked_at, claim_branch, \
         claim_host, claim_since, claim_runner, claim_job, claim_on, decision, decided_at, resolution, \
         scope, complexity, theme, rank, opened_at, updated_at, group_name, body, tags, conflict) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         RETURNING rid",
        values,
    )
    .await
}

/// The rid of an id the item names, which the checkout must hold.
async fn target(
    tx: &DatabaseTransaction,
    i: &ItemDump,
    id: &str,
    how: &str,
) -> Result<i64, String> {
    let missing = || {
        format!(
            "{} {how} {id}, which is not in the tree",
            item_path(&i.project, &i.id)
        )
    };
    let (key, num) = split_id(id).map_err(|_| missing())?;
    scalar(
        tx,
        "SELECT rid FROM items WHERE project=? AND key=? AND num=?",
        vec![i.project.clone().into(), key.into(), num.into()],
    )
    .await
    .map_err(|_| missing())
}

/// The second pass: the wait, the replacement, the item links and the search row.
async fn restore_refs(tx: &DatabaseTransaction, rid: i64, i: &ItemDump) -> Result<(), String> {
    let wait_item = match (i.wait_on.as_deref(), i.wait_ref.as_deref()) {
        (Some("item"), Some(r)) => Some(target(tx, i, r, "waits on").await?),
        (Some("item"), None) => return Err(format!("{} waits on no item", i.id)),
        _ => None,
    };
    let superseded = match i.superseded_by.as_deref() {
        Some(r) => Some(target(tx, i, r, "is superseded by").await?),
        None => None,
    };
    exec(
        tx,
        "UPDATE items SET wait_on=?, wait_item=?, wait_ref=?, wait_since=?, superseded_by=? WHERE rid=?",
        vec![
            i.wait_on.clone().into(),
            wait_item.into(),
            i.wait_ref.clone().into(),
            i.wait_since.clone().into(),
            superseded.into(),
            rid.into(),
        ],
    )
    .await?;
    for (kind, ids) in [("related", &i.related), ("opened", &i.opened)] {
        for id in ids {
            let to = target(tx, i, id, &format!("lists under {kind}")).await?;
            exec(
                tx,
                "INSERT INTO links (rid, kind, to_rid) VALUES (?, ?, ?) ON CONFLICT DO NOTHING",
                vec![rid.into(), kind.into(), to.into()],
            )
            .await?;
        }
    }
    index(tx, rid, i).await
}

/// The citation links and the search row, as a write builds them.
async fn index(tx: &DatabaseTransaction, rid: i64, i: &ItemDump) -> Result<(), String> {
    let cites = citations(&i.body);
    for c in &cites {
        exec(
            tx,
            "INSERT INTO links (rid, kind, to_path, to_line) VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING",
            vec![
                rid.into(),
                c.kind.into(),
                c.path.clone().into(),
                c.line.into(),
            ],
        )
        .await?;
    }
    let mut files: Vec<&str> = cites.iter().map(|c| c.path.as_str()).collect();
    files.sort_unstable();
    files.dedup();
    exec(
        tx,
        "INSERT INTO search (rid, id, title, body, files) VALUES (?, ?, ?, ?, ?)",
        vec![
            rid.into(),
            i.id.clone().into(),
            i.title.clone().into(),
            i.body.clone().into(),
            files.join(" ").into(),
        ],
    )
    .await
}

/// A project's event log, each line once by uid, its item found by id.
async fn restore_events(
    tx: &DatabaseTransaction,
    repo: &Path,
    slug: &str,
) -> Result<usize, String> {
    let path = events_path(slug);
    let text = tree::read(repo, &path).unwrap_or_default();
    let mut n = 0;
    for (k, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let ev: Json = serde_json::from_str(line).map_err(|e| format!("{path}:{}: {e}", k + 1))?;
        let rid = match ev["item"].as_str() {
            Some(id) => item_rid(tx, slug, id).await?,
            None => None,
        };
        let data = Some(&ev["data"])
            .filter(|d| truthy(d))
            .map_or(Value::Json(None), |d| json_value(d.clone()));
        let text_of = |k: &str| ev[k].as_str().map(str::to_string);
        exec(
            tx,
            "INSERT INTO events (uid, project, rid, at, host, branch, kind, note, data) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING",
            vec![
                text_of("uid").into(),
                slug.into(),
                rid.into(),
                text_of("at").into(),
                text_of("host").into(),
                text_of("branch").into(),
                text_of("kind").into(),
                text_of("note").into(),
                data,
            ],
        )
        .await?;
        n += 1;
    }
    Ok(n)
}

async fn item_rid(tx: &DatabaseTransaction, slug: &str, id: &str) -> Result<Option<i64>, String> {
    let Ok((key, num)) = split_id(id) else {
        return Ok(None);
    };
    let rid = scalar(
        tx,
        "SELECT COALESCE((SELECT rid FROM items WHERE project=? AND key=? AND num=?), -1)",
        vec![slug.into(), key.into(), num.into()],
    )
    .await?;
    Ok(Some(rid).filter(|r| *r >= 0))
}

/// Whether Python reads the value as true: not null, false, zero or empty.
fn truthy(v: &Json) -> bool {
    match v {
        Json::Null => false,
        Json::Bool(b) => *b,
        Json::Number(n) => n.as_f64() != Some(0.0),
        Json::String(s) => !s.is_empty(),
        Json::Array(a) => !a.is_empty(),
        Json::Object(o) => !o.is_empty(),
    }
}

#[cfg(test)]
#[path = "tests/restore.rs"]
mod tests;
