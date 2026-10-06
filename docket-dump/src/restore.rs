//! A Postgres database rebuilt from a dump checkout, for disaster recovery: into an empty one only.
//! Each item's release, area, priority, labels, parent, origins, dependencies and assignment rows are
//! read from its file. A checkout written before the dump took this shape is refused, and is rewritten
//! by a full dump pass first.

use std::path::Path;

use sea_orm::{
    ConnectionTrait, Database, DatabaseConnection, DatabaseTransaction, TransactionTrait, Value,
};
use serde_json::{Value as Json, json};

use docket_core::area::Area;
use docket_core::dump::{ItemDump, events_path, item_from, item_path, parse_item, project_path};
use docket_core::label::Label;
use docket_core::publication::Publication;
use docket_core::release::Release;
use docket_core::text::{citations, split_id};
use docket_core::word::ItemType;
use docket_migration::statement;

use crate::tree;

const LIST_COLUMNS: [&str; 3] = ["remotes", "cite_roots", "repos"];
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

/// Every project, item, link, assignment and event of the checkout written into the Postgres
/// database at `url`, in one transaction. The schema is migrated first.
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
    restore_publications(tx, slug, &doc).await
}

/// A project's publications, oldest first so the newest keeps the highest id, each covering the plans
/// it names, which are in once the items are.
async fn restore_publications(
    tx: &DatabaseTransaction,
    slug: &str,
    doc: &Json,
) -> Result<(), String> {
    let publications: Vec<Publication> = doc
        .get("publications")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    for p in publications.iter().rev() {
        let id = scalar(
            tx,
            "INSERT INTO publications (project, published_sha, work_sha, created_at) \
             VALUES (?, ?, ?, ?) RETURNING id",
            vec![
                slug.into(),
                p.published.clone().into(),
                p.work.clone().into(),
                p.created_at.clone().into(),
            ],
        )
        .await?;
        for plan in &p.plans {
            let (key, num) = split_id(plan).map_err(|e| e.0)?;
            let rid = scalar(
                tx,
                "SELECT COALESCE((SELECT rid FROM items WHERE project=? AND key=? AND num=?), -1)",
                vec![slug.into(), key.into(), num.into()],
            )
            .await?;
            if rid < 0 {
                return Err(format!(
                    "{} lists publication {} as covering {plan}, which is not in the tree",
                    project_path(slug),
                    p.published
                ));
            }
            exec(
                tx,
                "INSERT INTO publication_plans (publication, rid) VALUES (?, ?) ON CONFLICT DO NOTHING",
                vec![id.into(), rid.into()],
            )
            .await?;
        }
    }
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
        "INSERT INTO projects (slug, remotes, cite_roots, repos, fleet_repo, integration_ref, \
         worktree_hint, test_hint, created_at, updated_at, skills) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        values,
    )
    .await?;
    let releases: Vec<Release> = doc
        .get("releases")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    for r in releases {
        exec(
            tx,
            "INSERT INTO releases (project, name, position, target_date, shipped_at, note) \
             VALUES (?, ?, ?, ?, ?, ?)",
            vec![
                slug.into(),
                r.name.into(),
                r.position.into(),
                r.target_date.into(),
                r.shipped_at.into(),
                r.note.into(),
            ],
        )
        .await?;
    }
    let areas: Vec<Area> = doc
        .get("areas")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    for a in areas {
        exec(
            tx,
            "INSERT INTO areas (project, name, description, position, priority) \
             VALUES (?, ?, ?, ?, ?)",
            vec![
                slug.into(),
                a.name.into(),
                a.description.into(),
                a.position.into(),
                a.priority.into(),
            ],
        )
        .await?;
    }
    let labels: Vec<Label> = doc
        .get("labels")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    for l in labels {
        exec(
            tx,
            "INSERT INTO labels (project, name, description) VALUES (?, ?, ?)",
            vec![slug.into(), l.name.into(), l.description.into()],
        )
        .await?;
    }
    Ok(())
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
    let item_type = ItemType::parse(&i.item_type).ok_or_else(|| {
        format!(
            "{}: type {:?} is none of docket's types",
            item_path(&i.project, &i.id),
            i.item_type
        )
    })?;
    scalar(
        tx,
        "INSERT INTO items (project, key, num, title, state, decision, decided_at, resolution, \
         complexity, type, priority, opened_at, updated_at, body, release_id, area_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
         (SELECT id FROM releases WHERE project=? AND name=?), \
         (SELECT id FROM areas WHERE project=? AND name=?)) \
         RETURNING rid",
        vec![
            i.project.clone().into(),
            key.into(),
            num.into(),
            i.title.clone().into(),
            i.state.clone().into(),
            i.decision.clone().into(),
            i.decided_at.clone().into(),
            i.resolution.clone().into(),
            i.complexity.clone().into(),
            item_type.as_str().into(),
            i.priority.clone().into(),
            i.opened_at.clone().into(),
            i.updated_at.clone().into(),
            i.body.clone().into(),
            i.project.clone().into(),
            i.release.clone().into(),
            i.project.clone().into(),
            i.area.clone().into(),
        ],
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

/// The second pass: the dependencies, the replacement, the parent, the labels, the item links, the
/// assignment rows and the search row.
async fn restore_refs(tx: &DatabaseTransaction, rid: i64, i: &ItemDump) -> Result<(), String> {
    let superseded = match i.superseded_by.as_deref() {
        Some(r) => Some(target(tx, i, r, "is superseded by").await?),
        None => None,
    };
    let parent = match i.parent.as_deref() {
        Some(r) => Some(target(tx, i, r, "is under").await?),
        None => None,
    };
    exec(
        tx,
        "UPDATE items SET superseded_by=?, parent_rid=? WHERE rid=?",
        vec![superseded.into(), parent.into(), rid.into()],
    )
    .await?;
    for d in i.depends.iter().flatten() {
        let to = target(tx, i, &d.on, "depends on").await?;
        exec(
            tx,
            "INSERT INTO dependencies (rid, on_rid, created_at) VALUES (?, ?, ?) ON CONFLICT DO NOTHING",
            vec![rid.into(), to.into(), d.created_at.clone().into()],
        )
        .await?;
    }
    for name in i.labels.iter().flatten() {
        let id = scalar(
            tx,
            "INSERT INTO labels (project, name) VALUES (?, ?) \
             ON CONFLICT (project, lower(name)) DO UPDATE SET name=labels.name RETURNING id",
            vec![i.project.clone().into(), name.clone().into()],
        )
        .await?;
        exec(
            tx,
            "INSERT INTO item_labels (rid, label_id) VALUES (?, ?) ON CONFLICT DO NOTHING",
            vec![rid.into(), id.into()],
        )
        .await?;
    }
    let origin = i.origin.clone().unwrap_or_default();
    for (kind, ids) in [("related", &i.related), ("origin", &origin)] {
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
    for a in i.assignments.iter().flatten() {
        docket_migration::assignments::insert(tx, rid, a)
            .await
            .map_err(|e| format!("{}: {e}", item_path(&i.project, &i.id)))?;
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
