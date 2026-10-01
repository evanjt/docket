use std::collections::HashMap;

use axum::Json;
use axum::http::StatusCode;
use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, FromQueryResult, QueryFilter, Statement,
};
use serde_json::{Map, Value, json};

use docket_core::word::{Facts, Kind, PRIORITIES, priority, word};

use crate::entities::{item, project};

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

/// What each key of a project holds.
pub struct Kinds(Vec<(String, Kind)>);

impl Kinds {
    pub fn of(project: &project::Model) -> Self {
        let specs = project.keys.as_array().into_iter().flatten();
        Self(
            specs
                .filter_map(|spec| {
                    let key = spec["key"].as_str()?.to_string();
                    let kind = serde_json::from_value(spec["kind"].clone()).ok()?;
                    Some((key, kind))
                })
                .collect(),
        )
    }

    pub fn kind(&self, key: &str) -> Kind {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map_or(Kind::Work, |(_, kind)| *kind)
    }

    /// The keys holding one kind, in the project's order.
    pub fn keys(&self, kind: Kind) -> Vec<String> {
        self.0
            .iter()
            .filter(|(_, k)| *k == kind)
            .map(|(key, _)| key.clone())
            .collect()
    }
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

pub async fn id_of(db: &DatabaseConnection, rid: i64) -> Result<Option<String>, DbErr> {
    Ok(item::Entity::find_by_id(rid).one(db).await?.map(|i| i.id))
}

#[derive(FromQueryResult)]
struct Count {
    prid: i64,
    n: i64,
}

/// `{package rid: open members}` for every package of a project, in one query.
pub async fn open_members(db: &DatabaseConnection, slug: &str) -> Result<HashMap<i64, u64>, DbErr> {
    let stmt = sql(
        "SELECT l.to_rid AS prid, COUNT(*) AS n FROM links l JOIN items i ON i.rid=l.rid \
         JOIN items p ON p.rid=l.to_rid WHERE l.kind='opened' AND i.state='open' AND p.project=? \
         GROUP BY l.to_rid",
        vec![slug.into()],
    );
    Ok(Count::find_by_statement(stmt)
        .all(db)
        .await?
        .into_iter()
        .map(|c| (c.prid, c.n.unsigned_abs()))
        .collect())
}

pub fn facts(row: &item::Model, kind: Kind) -> Facts<'_> {
    Facts {
        state: &row.state,
        kind,
        claimed: row.claim_branch.is_some(),
        scope: row.scope.as_deref(),
        waiting: row.wait_on.is_some(),
        turn: row.turn.as_deref(),
    }
}

/// A row as `--json` prints it: ids instead of rids, `group` not `group_name`, with its word and
/// priority. Only a package's word reads `open_members`; `tier` overrides the priority its own tags name.
pub async fn public(
    db: &DatabaseConnection,
    kind: Kind,
    row: item::Model,
    open_members: u64,
    tier: Option<usize>,
) -> Result<Map<String, Value>, DbErr> {
    let tags: Vec<String> = serde_json::from_value(row.tags.clone()).unwrap_or_default();
    let superseded_by = match row.superseded_by {
        Some(rid) => id_of(db, rid).await?,
        None => None,
    };
    let open = if kind == Kind::Package {
        open_members
    } else {
        0
    };
    let mut out = Map::new();
    out.insert("word".into(), json!(word(&facts(&row, kind), open)));
    out.insert(
        "priority".into(),
        json!(tier.map_or_else(|| priority(&tags), |t| PRIORITIES[t])),
    );
    out.insert("group".into(), json!(row.group_name));
    out.insert("superseded_by".into(), json!(superseded_by));
    if let Value::Object(stored) = serde_json::to_value(StoredFields::from(row)).unwrap_or_default()
    {
        out.extend(stored);
    }
    Ok(out)
}

/// Rows of one project in their given order, each in the `--json` shape.
///
/// # Errors
/// The database.
pub async fn public_rows(
    db: &DatabaseConnection,
    project: &project::Model,
    rows: Vec<item::Model>,
) -> Result<Vec<Value>, Failure> {
    let kinds = Kinds::of(project);
    let counts = open_members(db, &project.slug)
        .await
        .map_err(|e| internal(&e))?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let open = counts.get(&row.rid).copied().unwrap_or(0);
        let kind = kinds.kind(&row.key);
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
pub async fn items_where(
    db: &DatabaseConnection,
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

/// The stored columns `--json` carries under their own names.
#[derive(serde::Serialize)]
struct StoredFields {
    project: String,
    key: String,
    num: i64,
    id: String,
    title: String,
    state: String,
    turn: Option<String>,
    turn_note: Option<String>,
    asked_at: Option<String>,
    claim_branch: Option<String>,
    claim_host: Option<String>,
    claim_since: Option<String>,
    claim_runner: Option<String>,
    claim_job: Option<String>,
    claim_on: Option<String>,
    wait_on: Option<String>,
    wait_ref: Option<String>,
    wait_since: Option<String>,
    decision: Option<String>,
    decided_at: Option<String>,
    resolution: Option<String>,
    scope: Option<String>,
    complexity: Option<String>,
    theme: Option<String>,
    rank: Option<i64>,
    tags: Value,
    body: String,
    conflict: i64,
    opened_at: String,
    updated_at: String,
}

impl From<item::Model> for StoredFields {
    fn from(r: item::Model) -> Self {
        Self {
            project: r.project,
            key: r.key,
            num: r.num,
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
            scope: r.scope,
            complexity: r.complexity,
            theme: r.theme,
            rank: r.rank,
            tags: r.tags,
            body: r.body,
            conflict: r.conflict,
            opened_at: r.opened_at,
            updated_at: r.updated_at,
        }
    }
}
