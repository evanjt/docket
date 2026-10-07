//! `/metrics`: the numbers `docket-core` works out over a release or a plan, served as they come.

use std::collections::HashSet;

use axum::Json;
use axum::extract::{Query, State};
use sea_orm::{ConnectionTrait, DatabaseConnection, FromQueryResult};
use serde::Deserialize;
use serde_json::{Value, json};

use docket_core::assignment::{Kind as Attempt, Outcome};
use docket_core::board::Board;
use docket_core::clock::now as clock_now;
use docket_core::metrics::{self, Dates, Span, Subject, Unshipped, WINDOW};
use docket_core::pace::epoch;
use docket_core::release::{self, Listed, Release};

use crate::reads::board::{core_board, held_later_ids};
use crate::reads::public::sql;
use crate::reads::rows::project_model;
use crate::verbs::Failure;

/// The seed every forecast is drawn with, so a read repeats until the data moves.
const SEED: u64 = 0x646f_636b_6574;

#[derive(Deserialize)]
pub struct Params {
    project: String,
    scope: Option<String>,
    /// A release name or `current`; the current release when absent.
    release: Option<String>,
    /// The plan, for the plan scope.
    plan: Option<String>,
    days: Option<u32>,
}

#[derive(FromQueryResult)]
struct Closed {
    rid: i64,
    at: String,
}

#[derive(FromQueryResult)]
struct Attempted {
    rid: i64,
    kind: String,
    runner: Option<String>,
    started_at: String,
    ended_at: Option<String>,
    outcome: Option<String>,
    tokens_in: Option<i64>,
    tokens_out: Option<i64>,
    cost_reported: Option<f64>,
    model: Option<String>,
}

fn at(stamp: &str) -> i64 {
    epoch(stamp).unwrap_or(0)
}

/// Progress, throughput, cycle and lead time, time spent and cost over the items of a release or a
/// plan, and for a release the forecast of its clearing.
///
/// # Errors
/// 400 for a scope or release not offered, 404 for an unknown project or plan.
pub async fn metrics(
    State(db): State<DatabaseConnection>,
    Query(q): Query<Params>,
) -> Result<Json<Value>, Failure> {
    if !matches!(
        q.scope.as_deref(),
        None | Some("release" | "plan" | "releases" | "project")
    ) {
        return Err(Failure::Invalid(
            "scope: choose from release, plan, releases, project".into(),
        ));
    }
    let model = project_model(&db, &q.project).await?;
    let board = core_board(&db, &model).await?;
    let listed = crate::verbs::releases::listed(&db, &q.project).await?;
    let mut order = listed.all();
    order.sort_by_key(|r| r.position);
    let prices = model
        .skills
        .get("prices")
        .and_then(|v| v.as_str())
        .and_then(|v| docket_core::fact::prices_of(v).ok())
        .unwrap_or_default();
    let dates = Dates::read(
        &first_claims(&db, &q.project).await?,
        &closes(&db, &q.project).await?,
    );
    let counted = subjects(&board, &listed, &order, &dates);
    let now = at(&clock_now());
    let days = q.days.unwrap_or(WINDOW).clamp(1, 366);
    if q.scope.as_deref() == Some("releases") {
        let held = held_later_ids(&db, &q.project, &board).await?;
        return Ok(Json(json!({
            "project": q.project,
            "scope": "releases",
            "releases": metrics::release_rows(&counted, &unshipped(&order), &held, now, days, SEED),
        })));
    }
    let plan = q.scope.as_deref() == Some("plan");
    let whole = q.scope.as_deref() == Some("project");
    let given = q.release.as_deref().unwrap_or(release::CURRENT);
    let name = if whole || plan {
        String::new()
    } else {
        release::resolve(&listed.all(), given)?.unwrap_or_default()
    };
    let held = if whole {
        board.items.iter().map(|i| i.rid).collect()
    } else if plan {
        plan_rids(&board, q.plan.as_deref())?
    } else {
        let id = listed.id(&name);
        board
            .items
            .iter()
            .filter(|i| i.release_id == id)
            .map(|i| i.rid)
            .collect()
    };
    let items: Vec<Subject> = board
        .items
        .iter()
        .zip(&counted)
        .filter(|(i, _)| held.contains(&i.rid))
        .map(|(_, s)| s.clone())
        .collect();
    let spans = spans_held(attempts(&db, &q.project).await?, &held, &board, &dates);
    let mut out = json!({
        "project": q.project,
        "scope": q.scope.as_deref().unwrap_or("release"),
        "progress": metrics::progress(&items),
        "throughput": metrics::throughput(&items, now, days),
        "daily": metrics::daily(&items, now, days),
        "cycle_time": metrics::cycle_time(&items),
        "lead_time": metrics::lead_time(&items),
        "time_spent": metrics::time_spent(&spans, now),
        "cost": metrics::cost(&spans, now, &prices),
    });
    if whole {
        let areas = crate::verbs::areas::listed(&db, &q.project).await?;
        out["areas"] = json!(metrics::area_progress(&areas, &items));
    } else if plan {
        out["plan"] = json!(q.plan);
    } else {
        out["release"] = json!(name);
        if let Some(upto) = order.iter().position(|r| r.name == name) {
            let target = order[upto].target_date.as_deref();
            let forecast = metrics::forecast(&counted, upto, now, days, SEED, target);
            out["forecast"] = json!(forecast);
        }
    }
    Ok(Json(out))
}

/// The spans of the held items, each named by its item's id and settled with the item.
fn spans_held(
    spans_of: Vec<(i64, Span)>,
    held: &HashSet<i64>,
    board: &Board,
    dates: &Dates,
) -> Vec<Span> {
    spans_of
        .into_iter()
        .filter(|(rid, _)| held.contains(rid))
        .map(|(rid, s)| {
            let item = board.by_rid(rid);
            Span {
                item: item.map(|i| i.id.clone()).unwrap_or_default(),
                settled: item
                    .and_then(|i| dates.settled(rid, i.state == "open", at(&i.updated_at))),
                ..s
            }
        })
        .collect()
}

/// The rids a plan holds at any depth.
fn plan_rids(board: &Board, plan: Option<&str>) -> Result<HashSet<i64>, Failure> {
    let id = plan.ok_or_else(|| Failure::Invalid("plan: name the plan to read".into()))?;
    let item = board
        .get(id)
        .ok_or_else(|| Failure::NotFound(format!("no item {id}")))?;
    Ok(board.holds(item))
}

/// The releases not yet shipped, with their place in the whole order.
fn unshipped(order: &[Release]) -> Vec<Unshipped> {
    order
        .iter()
        .enumerate()
        .filter(|(_, r)| r.shipped_at.is_none())
        .map(|(position, r)| Unshipped {
            position,
            name: r.name.clone(),
            target: r.target_date.clone(),
        })
        .collect()
}

/// Every item as the metrics read it, in the board's order.
fn subjects(board: &Board, listed: &Listed, order: &[Release], dates: &Dates) -> Vec<Subject> {
    board
        .items
        .iter()
        .map(|i| {
            let name = listed.name(i.release_id);
            Subject {
                release: order.iter().position(|r| Some(r.name.as_str()) == name),
                opened_at: at(&i.opened_at),
                closed_at: dates.settled(i.rid, i.state == "open", at(&i.updated_at)),
                started_at: dates.started.get(&i.rid).copied(),
                ..board.subject(i)
            }
        })
        .collect()
}

/// The latest close or drop of each item of the project, by rid.
async fn closes<C: ConnectionTrait>(db: &C, slug: &str) -> Result<Vec<(i64, i64)>, Failure> {
    Ok(Closed::find_by_statement(sql(
        "SELECT rid, MAX(at) AS at FROM events \
         WHERE project=? AND kind IN ('closed', 'dropped') AND rid IS NOT NULL GROUP BY rid",
        vec![slug.into()],
    ))
    .all(db)
    .await?
    .into_iter()
    .map(|c| (c.rid, at(&c.at)))
    .collect())
}

/// The start of the earliest claim of each item of the project that was claimed, by rid.
///
/// # Errors
/// A database failure.
pub async fn first_claims<C: ConnectionTrait>(
    db: &C,
    slug: &str,
) -> Result<Vec<(i64, i64)>, Failure> {
    Ok(Closed::find_by_statement(sql(
        "SELECT a.rid, MIN(a.started_at) AS at FROM assignments a JOIN items i ON i.rid=a.rid \
         WHERE i.project=? AND a.kind='claim' GROUP BY a.rid ORDER BY a.rid",
        vec![slug.into()],
    ))
    .all(db)
    .await?
    .into_iter()
    .map(|c| (c.rid, at(&c.at)))
    .collect())
}

/// Every assignment of the project with its item's rid.
async fn attempts<C: ConnectionTrait>(db: &C, slug: &str) -> Result<Vec<(i64, Span)>, Failure> {
    Ok(Attempted::find_by_statement(sql(
        "SELECT a.rid, a.kind, a.runner, a.started_at, a.ended_at, a.outcome, a.tokens_in, \
         a.tokens_out, a.cost_reported, a.model FROM assignments a JOIN items i ON i.rid=a.rid \
         WHERE i.project=? ORDER BY a.started_at, a.id",
        vec![slug.into()],
    ))
    .all(db)
    .await?
    .into_iter()
    .map(|a| {
        (
            a.rid,
            Span {
                item: String::new(),
                kind: Attempt::parse(&a.kind).unwrap_or(Attempt::Claim),
                runner: a.runner,
                start: at(&a.started_at),
                end: a.ended_at.as_deref().map(at),
                outcome: a.outcome.as_deref().and_then(Outcome::parse),
                settled: None,
                tokens_in: a.tokens_in,
                tokens_out: a.tokens_out,
                cost_reported: a.cost_reported,
                model: a.model,
            },
        )
    })
    .collect())
}
