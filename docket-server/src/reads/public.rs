use std::collections::HashMap;

use axum::Json;
use axum::http::StatusCode;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait, FromQueryResult,
    QueryFilter, Statement,
};
use serde_json::{Map, Value, json};

use docket_core::assignment::{Held, HeldFields};
use docket_core::item::turn_of;
use docket_core::stall::Wait;
use docket_core::word::{ItemType, Kind, PRIORITIES, Standing, kind_of_type};

use crate::entities::{item, project};
use crate::store::held_of;
use crate::verbs::graph::wait_of;

pub type Failure = (StatusCode, Json<Value>);

pub fn failure(status: StatusCode, message: &str) -> Failure {
    (status, Json(json!({ "error": message })))
}

pub fn internal(e: &DbErr) -> Failure {
    failure(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
}

/// A raw statement, written with `?` marks, with its bound values.
pub fn sql(text: &str, values: Vec<sea_orm::Value>) -> Statement {
    docket_migration::statement(text, values)
}

/// `?,?,?` for a list of values, `NULL` when there are none, so `x IN (...)` matches nothing.
pub fn marks(n: usize) -> String {
    if n == 0 {
        return "NULL".to_string();
    }
    vec!["?"; n].join(",")
}

/// The keys holding questions: the one key questions are filed under.
pub fn decision_keys() -> Vec<String> {
    vec![ItemType::Question.key().to_string()]
}

/// A project by its slug.
///
/// # Errors
/// 404 when the slug is unknown.
pub async fn project_of(db: &DatabaseConnection, slug: &str) -> Result<project::Model, Failure> {
    project::Entity::find_by_id(slug)
        .one(db)
        .await
        .map_err(|e| internal(&e))?
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, &format!("no project {slug}")))
}

/// An item by its id within a project.
///
/// # Errors
/// 400 when the text is not an id, 404 when the project holds no such item.
pub async fn item_of(
    db: &DatabaseConnection,
    slug: &str,
    id: &str,
) -> Result<item::Model, Failure> {
    let id = id.trim().to_uppercase();
    if !docket_core::search::is_id(&id) {
        return Err(failure(
            StatusCode::BAD_REQUEST,
            &format!("{id:?} is not an id: a key of one to three capitals and a number, like B14"),
        ));
    }
    item::Entity::find()
        .filter(item::Column::Project.eq(slug))
        .filter(item::Column::Id.eq(&id))
        .one(db)
        .await
        .map_err(|e| internal(&e))?
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, &format!("no item {id} in {slug}")))
}

pub async fn id_of<C: ConnectionTrait>(db: &C, rid: i64) -> Result<Option<String>, DbErr> {
    Ok(item::Entity::find_by_id(rid).one(db).await?.map(|i| i.id))
}

pub use docket_core::member::Members;

#[derive(FromQueryResult)]
struct Placed {
    rid: i64,
    parent_rid: Option<i64>,
    state: String,
}

/// `{plan rid: its members, at any depth}` for every item that holds some. One flat read walked in
/// memory: a recursive query here was estimated at millions of rows and JIT-compiled on every call.
pub async fn member_counts<C: ConnectionTrait>(
    db: &C,
    slug: &str,
) -> Result<HashMap<i64, Members>, DbErr> {
    totals(db, "project=?", slug.into()).await
}

/// How many items under rid, at any depth, are open and how many are closed.
pub async fn member_count<C: ConnectionTrait>(db: &C, rid: i64) -> Result<Members, DbErr> {
    let mut all = totals(
        db,
        "project=(SELECT project FROM items WHERE rid=?)",
        rid.into(),
    )
    .await?;
    Ok(all.remove(&rid).unwrap_or_default())
}

async fn totals<C: ConnectionTrait>(
    db: &C,
    filter: &str,
    value: sea_orm::Value,
) -> Result<HashMap<i64, Members>, DbErr> {
    let stmt = sql(
        &format!("SELECT rid, parent_rid, state FROM items WHERE {filter}"),
        vec![value],
    );
    let rows: Vec<(i64, Option<i64>, bool)> = Placed::find_by_statement(stmt)
        .all(db)
        .await?
        .into_iter()
        .map(|p| (p.rid, p.parent_rid, p.state == "open"))
        .collect();
    Ok(docket_core::member::member_totals(&rows))
}

/// The status word of a row, the same on every surface: an open claim is its open assignment, an
/// open ask the owner's turn, and `waiting` whether its dependencies hold it.
pub fn word_of(
    state: &str,
    held: Option<&Held>,
    waiting: bool,
    kind: Kind,
    members: Members,
) -> &'static str {
    let claimed = matches!(held, Some(Held::Claim(_)));
    let asked = matches!(held, Some(Held::Ask(_)));
    Standing::of(state, claimed, waiting, turn_of(state, asked))
        .holding(kind, members.open, members.closed)
        .word()
}

/// A row as `--json` prints it: ids instead of rids, with its word, priority, labels and the group
/// they name. A plan's word reads its `members`; `tier` overrides the item's own priority.
pub async fn public<C: ConnectionTrait>(
    db: &C,
    kind: Kind,
    row: item::Model,
    members: Members,
    tier: Option<usize>,
) -> Result<Map<String, Value>, DbErr> {
    let superseded_by = match row.superseded_by {
        Some(rid) => id_of(db, rid).await?,
        None => None,
    };
    let release: Option<String> = match row.release_id {
        Some(id) => {
            crate::store::scalar(db, "SELECT name FROM releases WHERE id=?", vec![id.into()])
                .await?
        }
        None => None,
    };
    let area = crate::verbs::areas::name_of(db, Some(row.area_id)).await?;
    let wait = wait_of(db, row.rid).await?;
    let held = held_of(db, row.rid).await?;
    let mut out = Map::new();
    out.insert(
        "word".into(),
        json!(word_of(
            &row.state,
            held.as_ref(),
            wait.is_some(),
            kind,
            members
        )),
    );
    out.insert("release".into(), json!(release));
    out.insert("area".into(), json!(area));
    let labels: Vec<String> = crate::verbs::labels::carried(db, row.rid)
        .await?
        .into_iter()
        .map(|l| l.name)
        .collect();
    out.insert("group".into(), json!(docket_core::label::group_of(&labels)));
    let repos = crate::verbs::labels::repos(db, row.rid).await?;
    out.insert("repo".into(), json!(repos.first()));
    out.insert("repos".into(), json!(repos));
    out.insert("labels".into(), json!(labels));
    out.insert(
        "priority".into(),
        json!(tier.map_or(row.priority.as_str(), |t| PRIORITIES[t])),
    );
    out.insert("superseded_by".into(), json!(superseded_by));
    let fields = HeldFields::of(&row.state, held.as_ref());
    if let Value::Object(stored) =
        serde_json::to_value(StoredFields::of(row, fields, wait.as_ref())).unwrap_or_default()
    {
        out.extend(stored);
    }
    Ok(out)
}

/// Rows of one project in their given order, each in the `--json` shape.
///
/// # Errors
/// The database.
pub async fn public_rows<C: ConnectionTrait>(
    db: &C,
    project: &project::Model,
    rows: Vec<item::Model>,
) -> Result<Vec<Value>, Failure> {
    let counts = member_counts(db, &project.slug)
        .await
        .map_err(|e| internal(&e))?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let open = counts.get(&row.rid).copied().unwrap_or_default();
        let kind = kind_of_type(&row.item_type);
        out.push(Value::Object(
            public(db, kind, row, open, None)
                .await
                .map_err(|e| internal(&e))?,
        ));
    }
    Ok(out)
}

/// Items of a project by a raw `SELECT * FROM items WHERE project=? ...` tail, bound after the slug.
///
/// # Errors
/// The database.
pub async fn items_where<C: ConnectionTrait>(
    db: &C,
    slug: &str,
    tail: &str,
    values: Vec<sea_orm::Value>,
) -> Result<Vec<item::Model>, Failure> {
    let mut bound: Vec<sea_orm::Value> = vec![slug.into()];
    bound.extend(values);
    item::Entity::find()
        .from_raw_sql(sql(
            &format!("SELECT * FROM items WHERE project=? {tail}"),
            bound,
        ))
        .all(db)
        .await
        .map_err(|e| internal(&e))
}

/// Refuses a value outside its choices, as the command's parser does.
///
/// # Errors
/// 400 naming the choices.
pub fn one_of(name: &str, value: Option<&str>, choices: &[&str]) -> Result<(), Failure> {
    match value {
        Some(v) if !choices.contains(&v) => Err(failure(
            StatusCode::BAD_REQUEST,
            &format!("{name}: choose from {}", choices.join(", ")),
        )),
        _ => Ok(()),
    }
}

/// The stored columns a list row carries under their own names, and what it waits on as its
/// dependencies hold it. The body is left out: only `/show` returns it.
#[derive(serde::Serialize)]
struct StoredFields {
    project: String,
    key: String,
    num: i64,
    id: String,
    title: String,
    state: String,
    #[serde(flatten)]
    held: HeldFields,
    wait_on: Option<String>,
    wait_ref: Option<String>,
    wait_since: Option<String>,
    decision: Option<String>,
    decided_at: Option<String>,
    resolution: Option<String>,
    complexity: Option<String>,
    #[serde(rename = "type")]
    item_type: String,
    opened_at: String,
    updated_at: String,
}

impl StoredFields {
    fn of(r: item::Model, held: HeldFields, wait: Option<&Wait>) -> Self {
        Self {
            project: r.project,
            key: r.key,
            num: r.num,
            id: r.id,
            title: r.title,
            state: r.state,
            held,
            wait_on: wait.map(|w| w.on.to_string()),
            wait_ref: wait.map(|w| w.id.clone()),
            wait_since: wait.map(|w| w.since.clone()),
            decision: r.decision,
            decided_at: r.decided_at,
            resolution: r.resolution,
            complexity: r.complexity,
            item_type: r.item_type,
            opened_at: r.opened_at,
            updated_at: r.updated_at,
        }
    }
}
