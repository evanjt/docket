use axum::Json;
use axum::extract::{Query, State};
use sea_orm::{DatabaseConnection, FromQueryResult};
use serde::Deserialize;
use serde_json::{Value, json};

use docket_core::flow::{DERIVED, derived_parts};
use docket_core::word::Kind;

use crate::reads::public::{
    Failure, Kinds, items_where, marks, one_of, project_of, public_rows, sql,
};

fn like(words: &str) -> sea_orm::Value {
    format!("%{words}%").into()
}

async fn listed(
    db: &DatabaseConnection,
    slug: &str,
    tail: &str,
    values: Vec<sea_orm::Value>,
) -> Result<Json<Value>, Failure> {
    let project = project_of(db, slug).await?;
    let rows = items_where(db, slug, tail, values).await?;
    Ok(Json(Value::Array(public_rows(db, &project, rows).await?)))
}

#[derive(Deserialize)]
pub struct InProject {
    project: String,
}

/// `docket todo`: open items on the owner's turn, questions first, by theme and rank.
///
/// # Errors
/// 404 for an unknown project.
pub async fn todo(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    listed(
        &db,
        &q.project,
        "AND state='open' AND turn='user' \
         ORDER BY key='Q', theme IS NULL, theme, rank IS NULL, rank, asked_at NULLS FIRST, rid",
        vec![],
    )
    .await
}

#[derive(Deserialize)]
pub struct ByHost {
    project: String,
    host: Option<String>,
}

/// `docket wip`: claimed items, oldest claim first.
///
/// # Errors
/// 404 for an unknown project.
pub async fn wip(
    State(db): State<DatabaseConnection>,
    Query(q): Query<ByHost>,
) -> Result<Json<Value>, Failure> {
    let (mut tail, mut values) = ("AND claim_branch IS NOT NULL".to_string(), vec![]);
    if let Some(host) = q.host {
        tail.push_str(" AND claim_host=?");
        values.push(host.into());
    }
    tail.push_str(" ORDER BY claim_since, rid");
    listed(&db, &q.project, &tail, values).await
}

#[derive(Deserialize)]
pub struct ByWait {
    project: String,
    on: Option<String>,
}

/// `docket waiting`: open items nobody can move yet, by what they wait on.
///
/// # Errors
/// 400 when `on` is neither `item` nor `condition`, 404 for an unknown project.
pub async fn waiting(
    State(db): State<DatabaseConnection>,
    Query(q): Query<ByWait>,
) -> Result<Json<Value>, Failure> {
    one_of("on", q.on.as_deref(), &["item", "condition"])?;
    let (mut tail, mut values) = (
        "AND state='open' AND wait_on IS NOT NULL".to_string(),
        vec![],
    );
    if let Some(on) = q.on {
        tail.push_str(" AND wait_on=?");
        values.push(on.into());
    }
    tail.push_str(" ORDER BY wait_on, wait_ref NULLS FIRST, wait_since, rid");
    listed(&db, &q.project, &tail, values).await
}

#[derive(Deserialize)]
pub struct ByTheme {
    project: String,
    theme: Option<String>,
}

/// `docket questions`: open decisions still undecided, by theme. Empty where no key holds decisions.
///
/// # Errors
/// 404 for an unknown project.
pub async fn questions(
    State(db): State<DatabaseConnection>,
    Query(q): Query<ByTheme>,
) -> Result<Json<Value>, Failure> {
    let project = project_of(&db, &q.project).await?;
    let qkeys = Kinds::of(&project).keys(Kind::Decision);
    if qkeys.is_empty() {
        return Ok(Json(json!([])));
    }
    let mut tail = format!(
        "AND key IN ({}) AND state='open' AND decision IS NULL",
        marks(qkeys.len())
    );
    let mut values: Vec<sea_orm::Value> = qkeys.into_iter().map(Into::into).collect();
    if let Some(theme) = q.theme {
        tail.push_str(" AND theme ILIKE ? ESCAPE ''");
        values.push(like(&theme));
    }
    tail.push_str(" ORDER BY theme IS NULL, theme, rank IS NULL, rank, rid");
    listed(&db, &q.project, &tail, values).await
}

/// `docket research`: decided questions still open, which owe work items.
///
/// # Errors
/// 404 for an unknown project.
pub async fn research(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    let project = project_of(&db, &q.project).await?;
    let qkeys = Kinds::of(&project).keys(Kind::Decision);
    let tail = format!(
        "AND key IN ({}) AND state='open' AND decision IS NOT NULL ORDER BY decided_at, rid",
        marks(qkeys.len())
    );
    listed(
        &db,
        &q.project,
        &tail,
        qkeys.into_iter().map(Into::into).collect(),
    )
    .await
}

#[derive(Deserialize)]
pub struct Recent {
    project: String,
    #[serde(default = "twenty")]
    n: i64,
    key: Option<String>,
}

fn twenty() -> i64 {
    20
}

async fn in_state(db: &DatabaseConnection, q: Recent, state: &str) -> Result<Json<Value>, Failure> {
    let (mut tail, mut values) = ("AND state=?".to_string(), vec![state.into()]);
    if let Some(key) = q.key {
        tail.push_str(" AND key=?");
        values.push(key.to_uppercase().into());
    }
    tail.push_str(" ORDER BY updated_at DESC, rid LIMIT ?");
    values.push(q.n.into());
    listed(db, &q.project, &tail, values).await
}

/// `docket done`: recently done, newest first.
///
/// # Errors
/// 404 for an unknown project.
pub async fn done(
    State(db): State<DatabaseConnection>,
    Query(q): Query<Recent>,
) -> Result<Json<Value>, Failure> {
    in_state(&db, q, "done").await
}

/// `docket dropped`: recently dropped, newest first.
///
/// # Errors
/// 404 for an unknown project.
pub async fn dropped(
    State(db): State<DatabaseConnection>,
    Query(q): Query<Recent>,
) -> Result<Json<Value>, Failure> {
    in_state(&db, q, "dropped").await
}

#[derive(Deserialize)]
pub struct ByGroup {
    project: String,
    name: Option<String>,
}

/// `docket groups`: items meant to be taken together, by group, rank and number.
///
/// # Errors
/// 404 for an unknown project.
pub async fn groups(
    State(db): State<DatabaseConnection>,
    Query(q): Query<ByGroup>,
) -> Result<Json<Value>, Failure> {
    let (mut tail, mut values) = ("AND group_name IS NOT NULL".to_string(), vec![]);
    if let Some(name) = q.name {
        tail.push_str(" AND group_name=?");
        values.push(name.into());
    }
    tail.push_str(" ORDER BY group_name, rank IS NULL, rank, num, state, rid");
    listed(&db, &q.project, &tail, values).await
}

#[derive(Deserialize)]
pub struct Newest {
    project: String,
    #[serde(default = "forty")]
    n: i64,
}

fn forty() -> i64 {
    40
}

#[derive(FromQueryResult)]
struct DerivedQuestion {
    id: String,
    state: String,
    decided_at: Option<String>,
    title: String,
    decision: String,
}

#[derive(FromQueryResult)]
struct DerivedEvent {
    at: String,
    note: Option<String>,
    data: Option<Value>,
    id: String,
    state: String,
    title: String,
}

/// `docket derived`: the decisions agents derived, on questions and on tickets, newest first.
///
/// # Errors
/// 404 for an unknown project.
pub async fn derived(
    State(db): State<DatabaseConnection>,
    Query(q): Query<Newest>,
) -> Result<Json<Value>, Failure> {
    let project = project_of(&db, &q.project).await?;
    let qkeys = Kinds::of(&project).keys(Kind::Decision);
    let mut values: Vec<sea_orm::Value> = vec![q.project.clone().into()];
    values.extend(qkeys.iter().map(|k| k.clone().into()));
    values.push(format!("{DERIVED}%").into());
    values.push(q.n.into());
    let questions = DerivedQuestion::find_by_statement(sql(
        &format!(
            "SELECT id, state, decided_at, title, decision FROM items WHERE project=? AND key IN ({}) \
             AND decision ILIKE ? ESCAPE '' ORDER BY decided_at DESC, rid LIMIT ?",
            marks(qkeys.len())
        ),
        values,
    ))
    .all(&db)
    .await
    .map_err(|e| crate::reads::public::internal(&e))?;
    let events = DerivedEvent::find_by_statement(sql(
        "SELECT e.at, e.note, e.data, i.id, i.state, i.title FROM events e JOIN items i ON i.rid=e.rid \
         WHERE e.project=? AND e.kind='decided' AND e.data::text ILIKE '%\"derived\"%' ESCAPE '' \
         ORDER BY e.at DESC, e.seq DESC LIMIT ?",
        vec![q.project.into(), q.n.into()],
    ))
    .all(&db)
    .await
    .map_err(|e| crate::reads::public::internal(&e))?;

    let mut out: Vec<(String, Value)> = Vec::new();
    for r in questions {
        let (basis, chose) = derived_parts(&r.decision);
        let at = r.decided_at.unwrap_or_default();
        out.push((
            at.clone(),
            json!({ "id": r.id, "state": r.state, "at": at, "title": r.title, "chose": chose, "basis": basis }),
        ));
    }
    for e in events {
        let data = e.data.unwrap_or_default();
        let basis = data["derived"].as_str().unwrap_or_default();
        out.push((
            e.at.clone(),
            json!({ "id": e.id, "state": e.state, "at": e.at, "title": e.title, "chose": e.note, "basis": basis }),
        ));
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.truncate(usize::try_from(q.n).unwrap_or(0));
    Ok(Json(Value::Array(
        out.into_iter().map(|(_, v)| v).collect(),
    )))
}
