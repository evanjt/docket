//! `/metrics`: the numbers `docket-core` works out over a release or a plan, served as they come.

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Query, State};
use sea_orm::{ConnectionTrait, DatabaseConnection, FromQueryResult};
use serde::Deserialize;
use serde_json::{Value, json};

use docket_core::assignment::{Kind as Attempt, Outcome};
use docket_core::board::Board;
use docket_core::clock::now as clock_now;
use docket_core::metrics::{self, Span, Subject, Unshipped, WINDOW};
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
    let spans_of = attempts(&db, &q.project).await?;
    let counted = subjects(&db, &model, &board, &listed, &order, &spans_of).await?;
    let now = at(&clock_now());
    let days = q.days.unwrap_or(WINDOW).clamp(1, 366);
    if q.scope.as_deref() == Some("releases") {
        let items: Vec<Subject> = counted.iter().map(|(_, s)| s.clone()).collect();
        let held = held_later_ids(&db, &q.project, &board).await?;
        return Ok(Json(json!({
            "project": q.project,
            "scope": "releases",
            "releases": metrics::release_rows(&items, &unshipped(&order), &held, now, days, SEED),
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
    let items: Vec<Subject> = counted
        .iter()
        .filter(|(rid, _)| held.contains(rid))
        .map(|(_, s)| s.clone())
        .collect();
    let spans: Vec<Span> = spans_of
        .iter()
        .filter(|(rid, _)| held.contains(rid))
        .map(|(rid, s)| Span {
            item: board.by_rid(*rid).map(|i| i.id.clone()).unwrap_or_default(),
            settled: counted
                .iter()
                .find(|(r, _)| r == rid)
                .and_then(|(_, s)| s.closed_at),
            ..s.clone()
        })
        .collect();
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
            let everything: Vec<Subject> = counted.iter().map(|(_, s)| s.clone()).collect();
            let target = order[upto].target_date.as_deref();
            let forecast = metrics::forecast(&everything, upto, now, days, SEED, target);
            out["forecast"] = json!(forecast);
        }
    }
    Ok(Json(out))
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

/// Every item, by rid, as the metrics read it.
async fn subjects(
    db: &DatabaseConnection,
    model: &crate::entities::project::Model,
    board: &Board,
    listed: &Listed,
    order: &[Release],
    spans: &[(i64, Span)],
) -> Result<Vec<(i64, Subject)>, Failure> {
    let closes: HashMap<i64, i64> = Closed::find_by_statement(sql(
        "SELECT i.rid, MAX(e.at) AS at FROM events e JOIN items i ON i.rid=e.rid \
         WHERE e.project=? AND e.kind IN ('closed', 'dropped') GROUP BY i.rid",
        vec![model.slug.clone().into()],
    ))
    .all(db)
    .await?
    .into_iter()
    .map(|c| (c.rid, at(&c.at)))
    .collect();
    Ok(board
        .items
        .iter()
        .map(|i| {
            let closed_at = (i.state != "open").then(|| {
                closes
                    .get(&i.rid)
                    .copied()
                    .unwrap_or_else(|| at(&i.updated_at))
            });
            let started_at = spans
                .iter()
                .filter(|(rid, s)| *rid == i.rid && s.kind == Attempt::Claim)
                .map(|(_, s)| s.start)
                .min();
            let name = listed.name(i.release_id);
            let subject = Subject {
                id: i.id.clone(),
                word: board.word(i),
                release: order.iter().position(|r| Some(r.name.as_str()) == name),
                opened_at: at(&i.opened_at),
                closed_at,
                started_at,
                claimed: i.claim_branch.is_some(),
                area: i.area_id,
                holder: board.subject(i).holder,
            };
            (i.rid, subject)
        })
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
