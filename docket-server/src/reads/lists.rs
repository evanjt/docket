use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use sea_orm::{DatabaseConnection, FromQueryResult};
use serde::Deserialize;
use serde_json::{Value, json};

use std::collections::HashMap;

use docket_core::flow::{DERIVED, derived_parts};
use docket_core::queue::{OwnerFilter, OwnerRow, owner_queue};
use docket_core::word::Kind;
use docket_core::word::{PRIORITIES, priority};

use crate::reads::public::{
    Failure, Kinds, failure, internal, items_where, marks, one_of, open_members, project_of,
    public, public_rows, sql,
};
use crate::reads::queue::under_cond;
use crate::reads::search::{Narrow, similar_rows};

fn like(words: &str) -> sea_orm::Value {
    format!("%{words}%").into()
}

async fn listed(
    db: &DatabaseConnection,
    slug: &str,
    under: Option<&str>,
    tail: &str,
    values: Vec<sea_orm::Value>,
) -> Result<Json<Value>, Failure> {
    let project = project_of(db, slug).await?;
    let (cond, mut bound) = under_cond(db, slug, under, "rid").await?;
    bound.extend(values);
    let rows = items_where(db, slug, &format!("{cond} {tail}"), bound).await?;
    Ok(Json(Value::Array(public_rows(db, &project, rows).await?)))
}

#[derive(Deserialize)]
pub struct InProject {
    under: Option<String>,
    project: String,
}

#[derive(Deserialize)]
pub struct OwnerQuery {
    project: String,
    under: Option<String>,
    n: Option<usize>,
    key: Option<String>,
    theme: Option<String>,
    priority: Option<String>,
}

#[derive(FromQueryResult)]
struct OpenNeed {
    rid: i64,
    need: Option<String>,
}

/// The owner's queue as `docket-core` orders it, and the rows it names in that order, each with its group.
async fn owner_rows(
    db: &DatabaseConnection,
    q: &OwnerQuery,
    limit: Option<usize>,
) -> Result<(Vec<Value>, usize), Failure> {
    one_of("priority", q.priority.as_deref(), &PRIORITIES)?;
    let project = project_of(db, &q.project).await?;
    let kinds = Kinds::of(&project);
    let qkeys = kinds.keys(Kind::Decision);
    let mut values: Vec<sea_orm::Value> = qkeys.iter().map(|k| k.clone().into()).collect();
    values.push(format!("{DERIVED}%").into());
    let derived = if qkeys.is_empty() {
        String::new()
    } else {
        format!(
            " OR (key IN ({}) AND decision ILIKE ? ESCAPE '')",
            marks(qkeys.len())
        )
    };
    let (cond, mut bound) = under_cond(db, &q.project, q.under.as_deref(), "rid").await?;
    if !qkeys.is_empty() {
        bound.extend(values);
    }
    let items = items_where(
        db,
        &q.project,
        &format!("{cond} AND state='open' AND (turn='user'{derived})"),
        bound,
    )
    .await?;
    let needs: HashMap<i64, String> = OpenNeed::find_by_statement(sql(
        "SELECT a.rid, a.need FROM assignments a JOIN items i ON i.rid=a.rid \
         WHERE i.project=? AND a.ended_at IS NULL AND a.kind='ask' AND a.need IS NOT NULL",
        vec![q.project.clone().into()],
    ))
    .all(db)
    .await
    .map_err(|e| internal(&e))?
    .into_iter()
    .filter_map(|n| n.need.map(|need| (n.rid, need)))
    .collect();
    let tags: Vec<Vec<String>> = items
        .iter()
        .map(|r| serde_json::from_value(r.tags.clone()).unwrap_or_default())
        .collect();
    let listed = crate::verbs::releases::listed(db, &q.project)
        .await
        .map_err(|e| internal(&e))?;
    let rows: Vec<OwnerRow> = items
        .iter()
        .zip(&tags)
        .map(|(r, tags)| OwnerRow {
            rid: r.rid,
            key: &r.key,
            kind: kinds.kind(&r.key),
            waiting: r.wait_on.is_some(),
            derived: r
                .decision
                .as_deref()
                .is_some_and(|d| d.starts_with(DERIVED)),
            need: needs.get(&r.rid).map(String::as_str),
            theme: r.theme.as_deref(),
            release: listed.name(r.release_id),
            tier: PRIORITIES
                .iter()
                .position(|p| *p == priority(tags))
                .unwrap_or(2),
            asked_at: r.asked_at.as_deref().unwrap_or(&r.opened_at),
        })
        .collect();
    let key = q.key.as_deref().map(str::to_uppercase);
    let releases = listed.open.clone();
    let filter = OwnerFilter {
        priority: q
            .priority
            .as_deref()
            .and_then(|p| PRIORITIES.iter().position(|x| *x == p)),
        key: key.as_deref(),
        theme: q.theme.as_deref(),
        releases: &releases,
        limit,
    };
    let queue = owner_queue(&rows, &filter);
    let mut by_rid: HashMap<i64, _> = items.iter().map(|r| (r.rid, r)).collect();
    let counts = open_members(db, &q.project)
        .await
        .map_err(|e| internal(&e))?;
    let mut out = Vec::with_capacity(queue.rows.len());
    for (rid, group) in queue.rows {
        let Some(row) = by_rid.remove(&rid) else {
            continue;
        };
        let open = counts.get(&rid).copied().unwrap_or(0);
        let mut view = public(db, kinds.kind(&row.key), row.clone(), open, None)
            .await
            .map_err(|e| internal(&e))?;
        view.insert("owner_group".into(), json!(group));
        out.push(Value::Object(view));
    }
    Ok((out, queue.waiting))
}

/// `docket todo`: derived answers to confirm, then questions, then asks by the need they name, each
/// group by release, priority and age. Items waiting on something else are left out.
///
/// # Errors
/// 400 for a priority outside its tiers, 404 for an unknown project.
pub async fn todo(
    State(db): State<DatabaseConnection>,
    Query(q): Query<OwnerQuery>,
) -> Result<Json<Value>, Failure> {
    let (rows, _) = owner_rows(&db, &q, Some(q.n.unwrap_or(usize::MAX))).await?;
    Ok(Json(Value::Array(rows)))
}

/// `docket todo`'s count of items left out because they wait on something first.
///
/// # Errors
/// As `todo`.
pub async fn todo_waiting(
    State(db): State<DatabaseConnection>,
    Query(q): Query<OwnerQuery>,
) -> Result<Json<Value>, Failure> {
    let (_, waiting) = owner_rows(&db, &q, None).await?;
    Ok(Json(json!({ "waiting": waiting })))
}

#[derive(Deserialize)]
pub struct Under {
    under: Option<String>,
    project: String,
}

/// Every item a plan, story or concept holds, whatever its word, for the words no list covers.
///
/// # Errors
/// 400 without `under`, 404 for an unknown project or `under` id.
pub async fn under(
    State(db): State<DatabaseConnection>,
    Query(q): Query<Under>,
) -> Result<Json<Value>, Failure> {
    if q.under.is_none() {
        return Err(failure(
            StatusCode::BAD_REQUEST,
            "under needs a plan, story or concept.",
        ));
    }
    listed(&db, &q.project, q.under.as_deref(), "ORDER BY rid", vec![]).await
}

#[derive(Deserialize)]
pub struct ByHost {
    under: Option<String>,
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
    listed(&db, &q.project, q.under.as_deref(), &tail, values).await
}

#[derive(Deserialize)]
pub struct ByWait {
    under: Option<String>,
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
    let project = project_of(&db, &q.project).await?;
    let (cond, under) = under_cond(&db, &q.project, q.under.as_deref(), "rid").await?;
    tail.push_str(&cond);
    values.extend(under);
    tail.push_str(" ORDER BY wait_on, wait_ref NULLS FIRST, wait_since, rid");
    let rows = items_where(&db, &q.project, &tail, values).await?;
    let targets: Vec<i64> = rows
        .iter()
        .filter(|r| r.wait_on.as_deref() == Some("item"))
        .filter_map(|r| r.wait_item)
        .collect();
    let mut by_id = std::collections::HashMap::new();
    if !targets.is_empty() {
        let found = items_where(
            &db,
            &q.project,
            &format!("AND rid IN ({})", marks(targets.len())),
            targets.into_iter().map(Into::into).collect(),
        )
        .await?;
        for t in public_rows(&db, &project, found).await? {
            if let Some(id) = t["id"].as_str() {
                by_id.insert(
                    id.to_string(),
                    json!({ "word": t["word"], "title": t["title"] }),
                );
            }
        }
    }
    let waits: Vec<Option<String>> = rows.iter().map(|r| r.wait_ref.clone()).collect();
    let mut out = public_rows(&db, &project, rows).await?;
    for (row, wait) in out.iter_mut().zip(waits) {
        row["wait_target"] = wait
            .and_then(|id| by_id.get(&id).cloned())
            .unwrap_or(Value::Null);
    }
    Ok(Json(Value::Array(out)))
}

#[derive(Deserialize)]
pub struct ByTheme {
    under: Option<String>,
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
    let kinds = Kinds::of(&project);
    let qkeys = kinds.keys(Kind::Decision);
    let decided_keys = qkeys.clone();
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
    let (cond, under) = under_cond(&db, &q.project, q.under.as_deref(), "rid").await?;
    tail.push_str(&cond);
    values.extend(under);
    tail.push_str(" ORDER BY theme IS NULL, theme, rank IS NULL, rank, rid");
    let rows = items_where(&db, &q.project, &tail, values).await?;
    let counts = open_members(&db, &q.project)
        .await
        .map_err(|e| internal(&e))?;
    let mut twins = Vec::with_capacity(rows.len());
    for row in &rows {
        let narrow = Narrow {
            decided: decided_keys.clone(),
            n: 1,
            ..Narrow::default()
        };
        let near = similar_rows(&db, &q.project, row, &narrow).await?;
        twins.push(match near.into_iter().next() {
            Some((twin, _, _)) => {
                let open = counts.get(&twin.rid).copied().unwrap_or(0);
                Value::Object(
                    public(&db, kinds.kind(&twin.key), twin, open, None)
                        .await
                        .map_err(|e| internal(&e))?,
                )
            }
            None => Value::Null,
        });
    }
    let mut out = public_rows(&db, &project, rows).await?;
    for (row, twin) in out.iter_mut().zip(twins) {
        row["close_to"] = twin;
    }
    Ok(Json(Value::Array(out)))
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
        q.under.as_deref(),
        &tail,
        qkeys.into_iter().map(Into::into).collect(),
    )
    .await
}

#[derive(Deserialize)]
pub struct Recent {
    under: Option<String>,
    project: String,
    #[serde(default = "twenty")]
    n: i64,
    key: Option<String>,
    release: Option<String>,
}

fn twenty() -> i64 {
    20
}

/// The condition and values selecting the items of one release, shipped or not.
///
/// # Errors
/// 400 when `release` is not one of the project's releases.
async fn in_release(
    db: &DatabaseConnection,
    slug: &str,
    release: &str,
    column: &str,
) -> Result<(String, Vec<sea_orm::Value>), Failure> {
    let listed = crate::verbs::releases::listed(db, slug)
        .await
        .map_err(|e| internal(&e))?;
    let Some(id) = listed.id(release) else {
        let names: Vec<&str> = listed.rows.iter().map(|(_, r)| r.name.as_str()).collect();
        return Err(failure(
            StatusCode::BAD_REQUEST,
            &format!("release: choose from {}", names.join(", ")),
        ));
    };
    Ok((format!(" AND {column}=?"), vec![id.into()]))
}

async fn in_state(db: &DatabaseConnection, q: Recent, state: &str) -> Result<Json<Value>, Failure> {
    project_of(db, &q.project).await?;
    let (mut tail, mut values) = ("AND state=?".to_string(), vec![state.into()]);
    if let Some(release) = &q.release {
        let (cond, vals) = in_release(db, &q.project, release, "release_id").await?;
        tail.push_str(&cond);
        values.extend(vals);
    }
    if let Some(key) = q.key {
        tail.push_str(" AND key=?");
        values.push(key.to_uppercase().into());
    }
    let (cond, under) = under_cond(db, &q.project, q.under.as_deref(), "rid").await?;
    tail.push_str(&cond);
    values.extend(under);
    tail.push_str(" ORDER BY updated_at DESC, rid LIMIT ?");
    values.push(q.n.into());
    listed(db, &q.project, None, &tail, values).await
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
    listed(&db, &q.project, None, &tail, values).await
}

#[derive(Deserialize)]
pub struct Newest {
    under: Option<String>,
    project: String,
    #[serde(default = "forty")]
    n: i64,
    release: Option<String>,
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
    let (own, own_values) = match &q.release {
        Some(r) => in_release(&db, &q.project, r, "release_id").await?,
        None => (String::new(), vec![]),
    };
    let (joined, joined_values) = match &q.release {
        Some(r) => in_release(&db, &q.project, r, "i.release_id").await?,
        None => (String::new(), vec![]),
    };
    let (own_under, own_under_values) =
        under_cond(&db, &q.project, q.under.as_deref(), "rid").await?;
    let (joined_under, joined_under_values) =
        under_cond(&db, &q.project, q.under.as_deref(), "i.rid").await?;
    let mut values: Vec<sea_orm::Value> = vec![q.project.clone().into()];
    values.extend(qkeys.iter().map(|k| k.clone().into()));
    values.push(format!("{DERIVED}%").into());
    values.extend(own_values);
    values.extend(own_under_values);
    values.push(q.n.into());
    let questions = DerivedQuestion::find_by_statement(sql(
        &format!(
            "SELECT id, state, decided_at, title, decision FROM items WHERE project=? AND key IN ({}) \
             AND decision ILIKE ? ESCAPE ''{own}{own_under} ORDER BY decided_at DESC, rid LIMIT ?",
            marks(qkeys.len())
        ),
        values,
    ))
    .all(&db)
    .await
    .map_err(|e| crate::reads::public::internal(&e))?;
    let mut event_values: Vec<sea_orm::Value> = vec![q.project.clone().into()];
    event_values.extend(joined_values);
    event_values.extend(joined_under_values);
    event_values.push(q.n.into());
    let events = DerivedEvent::find_by_statement(sql(
        &format!(
            "SELECT e.at, e.note, e.data, i.id, i.state, i.title FROM events e JOIN items i ON i.rid=e.rid \
             WHERE e.project=? AND e.kind='decided' AND e.data::text ILIKE '%\"derived\"%' ESCAPE ''{joined}{joined_under} \
             ORDER BY e.at DESC, e.seq DESC LIMIT ?"
        ),
        event_values,
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
