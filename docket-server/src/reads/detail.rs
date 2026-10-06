//! One item's surroundings: its events, its ties, what the text of `docket show` reads beside it.

use axum::Json;
use axum::extract::{Path, Query, State};
use sea_orm::{DatabaseConnection, FromQueryResult};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::reads::public::sql;
use crate::reads::rows::{Extra, project_model, rows_with, shaped};
use crate::reads::search::{Narrow, search_rows};
use crate::store::{self};
use crate::verbs::Failure;

#[derive(Deserialize)]
pub struct InProject {
    project: String,
}

#[derive(FromQueryResult)]
struct EventRow {
    seq: i64,
    uid: String,
    project: String,
    rid: Option<i64>,
    at: String,
    host: String,
    branch: Option<String>,
    kind: String,
    note: Option<String>,
    data: Option<Value>,
}

/// `docket log`: one item's events, oldest first, each with its data read.
///
/// # Errors
/// 404 for an unknown project or item.
pub async fn log(
    State(db): State<DatabaseConnection>,
    Path(id): Path<String>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    store::project(&db, &q.project).await?;
    let r = store::item_of(&db, &q.project, &id).await?;
    let events = EventRow::find_by_statement(sql(
        "SELECT * FROM events WHERE rid=? ORDER BY at, seq",
        vec![r.rid.into()],
    ))
    .all(&db)
    .await?;
    let out: Vec<Value> = events
        .into_iter()
        .map(|e| {
            let data = e
                .data
                .filter(|d| d.as_object().is_some_and(|m| !m.is_empty()));
            json!({
                "seq": e.seq, "uid": e.uid, "project": e.project, "rid": e.rid, "at": e.at,
                "host": e.host, "branch": e.branch, "kind": e.kind, "note": e.note,
                "data": data,
            })
        })
        .collect();
    Ok(Json(Value::Array(out)))
}

/// The queries `docket deps` reads, by the tie each names, in the order it prints them. Each list
/// comes in a fixed order: by rid, by link, or by the link's target.
const TIES: [(&str, &str); 9] = [
    (
        "holds",
        "SELECT i.* FROM dependencies d JOIN items i ON i.rid=d.rid WHERE d.on_rid=? AND i.state='open' \
         ORDER BY i.rid",
    ),
    (
        "related",
        "SELECT i.* FROM links l JOIN items i ON i.rid=l.to_rid WHERE l.rid=? AND l.kind='related' \
         UNION SELECT i.* FROM links l JOIN items i ON i.rid=l.rid WHERE l.to_rid=? AND l.kind='related' \
         ORDER BY rid",
    ),
    (
        "parent",
        "SELECT p.* FROM items i JOIN items p ON p.rid=i.parent_rid WHERE i.rid=?",
    ),
    (
        "children",
        "SELECT * FROM items WHERE parent_rid=? ORDER BY key, num",
    ),
    (
        "origin",
        "SELECT i.* FROM links l JOIN items i ON i.rid=l.to_rid WHERE l.rid=? AND l.kind='origin' \
         ORDER BY l.to_rid",
    ),
    (
        "spawned",
        "SELECT i.* FROM links l JOIN items i ON i.rid=l.rid WHERE l.to_rid=? AND l.kind='origin' \
         ORDER BY l.id",
    ),
    (
        "supersedes",
        "SELECT * FROM items WHERE superseded_by=? ORDER BY rid",
    ),
    (
        "same_files",
        "SELECT i.*, COUNT(DISTINCT l2.to_path) AS shared FROM links l1 \
         JOIN links l2 ON l2.to_path=l1.to_path AND l2.rid<>l1.rid \
         JOIN items i ON i.rid=l2.rid WHERE l1.rid=? AND l1.kind IN ('cites_file','cites_test') \
         GROUP BY i.rid ORDER BY shared DESC, i.rid DESC LIMIT 15",
    ),
    (
        "waits_on",
        "SELECT i.* FROM dependencies d JOIN items i ON i.rid=d.on_rid WHERE d.rid=? \
         ORDER BY d.created_at, d.on_rid",
    ),
];

/// `docket deps`: everything an item is tied to, both ways, by the tie.
///
/// # Errors
/// 404 for an unknown project or item.
pub async fn deps(
    State(db): State<DatabaseConnection>,
    Path(id): Path<String>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    let project = project_model(&db, &q.project).await?;
    let r = store::item_of(&db, &q.project, &id).await?;
    let mut out = Map::new();
    for (tie, text) in TIES {
        let values: Vec<sea_orm::Value> = match tie {
            "related" => vec![r.rid.into(), r.rid.into()],
            _ => vec![r.rid.into()],
        };
        let extras: &[&str] = if tie == "same_files" {
            &["shared"]
        } else {
            &[]
        };
        let found = rows_with(&db, text, values, extras).await?;
        if tie == "waits_on" && found.is_empty() {
            continue;
        }
        out.insert(tie.into(), json!(shaped(&db, &project, found).await?));
    }
    let others = crate::verbs::labels::grouped_with(&db, &q.project, r.rid).await?;
    if !others.is_empty() {
        let found = rows_with(
            &db,
            "SELECT * FROM items WHERE rid = ANY(?) ORDER BY state, rid",
            vec![others.into()],
            &[],
        )
        .await?;
        out.insert("group".into(), json!(shaped(&db, &project, found).await?));
    }
    if let Some(s) = r.superseded_by {
        let found = rows_with(&db, "SELECT * FROM items WHERE rid=?", vec![s.into()], &[]).await?;
        out.insert(
            "superseded_by".into(),
            json!(shaped(&db, &project, found).await?),
        );
    }
    let narrow = Narrow {
        exclude: Some(r.rid),
        n: 15,
        ..Narrow::default()
    };
    let mentions = search_rows(&db, &q.project, &format!("\"{}\"", r.id), &narrow)
        .await
        .unwrap_or_default();
    let mentions: Vec<Extra> = mentions
        .into_iter()
        .map(|(m, score, snip)| {
            let mut extra = Map::new();
            extra.insert("score".into(), json!(score));
            extra.insert("snip".into(), json!(snip));
            (m, extra)
        })
        .collect();
    out.insert(
        "mentions".into(),
        json!(shaped(&db, &project, mentions).await?),
    );
    Ok(Json(Value::Object(out)))
}

/// What `docket show` prints beside the row: the priority tier, what waits on it, its area.
///
/// # Errors
/// 404 for an unknown project or item.
pub async fn context(
    State(db): State<DatabaseConnection>,
    Path(id): Path<String>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    store::project(&db, &q.project).await?;
    let r = store::item_of(&db, &q.project, &id).await?;
    let tier = r.priority.as_str();
    let holds: Vec<String> = store::column(
        &db,
        "SELECT i.id FROM dependencies d JOIN items i ON i.rid=d.rid WHERE d.on_rid=? AND i.state='open' \
         ORDER BY i.rid",
        vec![r.rid.into()],
    )
    .await?;
    Ok(Json(json!({
        "priority": tier,
        "holds": holds,
        "area": crate::verbs::areas::name_of(&db, r.area_id).await?,
    })))
}

#[derive(Deserialize)]
pub struct FilesQuery {
    project: String,
    #[serde(default)]
    prefix: String,
    state: Option<String>,
}

/// `docket files`: items citing a path that starts with the prefix, by path, line and number.
///
/// # Errors
/// 404 for an unknown project.
pub async fn files(
    State(db): State<DatabaseConnection>,
    Query(q): Query<FilesQuery>,
) -> Result<Json<Value>, Failure> {
    let project = project_model(&db, &q.project).await?;
    let mut text = "SELECT i.*, l.to_path AS path, l.to_line AS line FROM links l JOIN items i ON i.rid=l.rid \
                    WHERE i.project=? AND l.kind IN ('cites_file','cites_test') AND l.to_path ILIKE ? ESCAPE ''"
        .to_string();
    let mut values: Vec<sea_orm::Value> =
        vec![q.project.clone().into(), format!("{}%", q.prefix).into()];
    if let Some(state) = q.state.filter(|s| s != "any") {
        text.push_str(" AND i.state=?");
        values.push(state.into());
    }
    text.push_str(" ORDER BY l.to_path, l.to_line NULLS FIRST, i.num, i.state, i.rid");
    let found = rows_with(&db, &text, values, &["path", "line"]).await?;
    Ok(Json(Value::Array(shaped(&db, &project, found).await?)))
}

#[derive(Deserialize)]
pub struct CitationsQuery {
    project: String,
    #[serde(default)]
    open_only: bool,
}

#[derive(FromQueryResult)]
struct Citation {
    id: String,
    state: String,
    to_path: String,
    to_line: Option<i64>,
    kind: String,
}

/// Every path an item of the project cites with its line and link kind, once per item, for `docket stale` to resolve on its host.
///
/// # Errors
/// 404 for an unknown project.
pub async fn citations(
    State(db): State<DatabaseConnection>,
    Query(q): Query<CitationsQuery>,
) -> Result<Json<Value>, Failure> {
    project_model(&db, &q.project).await?;
    let states = if q.open_only {
        "'open'"
    } else {
        "'open','done','dropped'"
    };
    let found = Citation::find_by_statement(sql(
        &format!(
            "SELECT DISTINCT i.id, i.state, l.to_path, l.to_line, l.kind FROM links l JOIN items i ON i.rid=l.rid \
             WHERE i.project=? AND l.kind IN ('cites_file','cites_test') AND i.state IN ({states}) \
             ORDER BY i.id, l.to_path, l.to_line NULLS FIRST, l.kind"
        ),
        vec![q.project.into()],
    ))
    .all(&db)
    .await?;
    Ok(Json(json!(
        found
            .into_iter()
            .map(|c| json!({ "id": c.id, "state": c.state, "path": c.to_path, "line": c.to_line, "kind": c.kind }))
            .collect::<Vec<_>>()
    )))
}

#[derive(Deserialize)]
pub struct ProjectQuery {
    project: String,
}

#[derive(FromQueryResult)]
struct OpenBody {
    id: String,
    body: String,
}

/// The id and body of every open item of the project, for `docket stale` to find the branches they name.
///
/// # Errors
/// 404 for an unknown project.
pub async fn open_bodies(
    State(db): State<DatabaseConnection>,
    Query(q): Query<ProjectQuery>,
) -> Result<Json<Value>, Failure> {
    project_model(&db, &q.project).await?;
    let found = OpenBody::find_by_statement(sql(
        "SELECT id, body FROM items WHERE project=? AND state='open' ORDER BY rid",
        vec![q.project.into()],
    ))
    .all(&db)
    .await?;
    Ok(Json(json!(
        found
            .into_iter()
            .map(|b| json!({ "id": b.id, "body": b.body }))
            .collect::<Vec<_>>()
    )))
}
