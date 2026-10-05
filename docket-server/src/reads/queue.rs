use std::collections::{BTreeMap, HashMap, HashSet};

use axum::extract::{Extension, Query, State};
use axum::{Json, http::StatusCode};
use sea_orm::{DatabaseConnection, FromQueryResult};
use serde::Deserialize;
use serde_json::{Value, json};

use docket_core::assignment::Outcome;
use docket_core::flow::tally;
use docket_core::member::{Edge, Tie, descendants};
use docket_core::queue::{Candidate, Filter, Ranked, Role, next as queue};
use docket_core::word::{Kind, PRIORITIES};

use crate::auth::Caller;
use crate::reads::public::{
    Failure, failure, internal, item_of, items_where, marks, member_counts, one_of, project_of,
    public, sql, word_of,
};
use crate::verbs::graph::open_dependencies;
use docket_core::word::kind_of_type;

/// The columns the queue and the flow read, for every item of a project, in rid order.
#[derive(FromQueryResult)]
struct Slim {
    rid: i64,
    key: String,
    item_type: String,
    state: String,
    turn: Option<String>,
    claim_branch: Option<String>,
    wait_on: Option<String>,
    conflict: i64,
    decided: bool,
    complexity: Option<String>,
    theme: Option<String>,
    release: Option<String>,
    priority: String,
    opened_at: String,
    area_priority: Option<String>,
    area: Option<String>,
}

struct Board {
    items: Vec<Slim>,
    ties: Vec<Tie>,
}

#[derive(FromQueryResult)]
struct TieRow {
    rid: i64,
    kind: String,
    to_rid: i64,
}

/// Every item of a project and every item-to-item link leaving one.
async fn board(db: &DatabaseConnection, slug: &str) -> Result<Board, Failure> {
    let items = Slim::find_by_statement(sql(
        "SELECT rid, key, type AS item_type, state, turn, claim_branch, wait_on, conflict, decision IS NOT NULL AS decided, complexity, theme, \
         (SELECT r.name FROM releases r WHERE r.id=items.release_id) AS release, priority, opened_at, \
         (SELECT a.priority FROM areas a WHERE a.id=items.area_id) AS area_priority, \
         (SELECT a.name FROM areas a WHERE a.id=items.area_id) AS area \
         FROM items WHERE project=? ORDER BY rid",
        vec![slug.into()],
    ))
    .all(db)
    .await
    .map_err(|e| internal(&e))?;
    let ties = TieRow::find_by_statement(sql(
        "SELECT l.rid, l.kind, l.to_rid FROM links l JOIN items i ON i.rid=l.rid \
         WHERE i.project=? AND l.kind IN ('related', 'origin') AND l.to_rid IS NOT NULL \
         UNION ALL SELECT rid, 'parent', parent_rid FROM items WHERE project=? AND parent_rid IS NOT NULL \
         ORDER BY 1, 2, 3",
        vec![slug.into(), slug.into()],
    ))
    .all(db)
    .await
    .map_err(|e| internal(&e))?
    .into_iter()
    .filter_map(|t| {
        Some(Tie {
            rid: t.rid,
            edge: Edge::parse(&t.kind)?,
            to: t.to_rid,
        })
    })
    .collect();
    Ok(Board { items, ties })
}

fn candidate(row: &Slim) -> Candidate<'_> {
    Candidate {
        rid: row.rid,
        key: &row.key,
        kind: kind_of_type(&row.item_type),
        open: row.state == "open",
        turn: row.turn.as_deref(),
        claimed: row.claim_branch.is_some(),
        waiting: row.wait_on.is_some(),
        conflict: row.conflict != 0,
        decided: row.decided,
        complexity: row.complexity.as_deref(),
        theme: row.theme.as_deref(),
        release: row.release.as_deref(),
        tier: PRIORITIES
            .iter()
            .position(|p| *p == row.priority)
            .unwrap_or(2),
        area_tier: PRIORITIES
            .iter()
            .position(|p| Some(*p) == row.area_priority.as_deref())
            .unwrap_or(2),
        area: row.area.as_deref(),
        opened_at: &row.opened_at,
    }
}

#[derive(Deserialize)]
pub struct NextQuery {
    project: String,
    #[serde(default = "ten")]
    n: usize,
    complexity: Option<String>,
    key: Option<String>,
    /// One item's id, to read its row and role when it is ready.
    id: Option<String>,
    theme: Option<String>,
    area: Option<String>,
    /// A release name or `current`.
    release: Option<String>,
    under: Option<String>,
    role: Option<String>,
    priority: Option<String>,
    #[serde(default)]
    current_release: bool,
}

fn ten() -> usize {
    10
}

/// The rids under a plan or story at any depth, .
async fn rids_under(
    db: &DatabaseConnection,
    board: &Board,
    slug: &str,
    id: &str,
) -> Result<HashSet<i64>, Failure> {
    let target = item_of(db, slug, id).await?;
    Ok(descendants(&board.ties, target.rid))
}

/// The condition and values keeping the rows of `column` that lie under the plan, story or concept `id`,
/// by the rule `next` applies; empty when no `id` is given.
///
/// # Errors
/// 404 for an unknown `id`.
pub(crate) async fn under_cond(
    db: &DatabaseConnection,
    slug: &str,
    id: Option<&str>,
    column: &str,
) -> Result<(String, Vec<sea_orm::Value>), Failure> {
    let Some(id) = id else {
        return Ok((String::new(), vec![]));
    };
    let board = board(db, slug).await?;
    let mut rids: Vec<i64> = rids_under(db, &board, slug, id)
        .await?
        .into_iter()
        .collect();
    rids.sort_unstable();
    if rids.is_empty() {
        return Ok((" AND FALSE".to_string(), vec![]));
    }
    Ok((
        format!(" AND {column} IN ({})", marks(rids.len())),
        rids.into_iter().map(Into::into).collect(),
    ))
}

/// `docket next`: the queue an agent takes from, most urgent first, narrowed to the roles asked, comma-separated,
/// which rank in the order given within a release.
///
/// # Errors
/// 400 for a choice outside its options, 404 for an unknown project or `under` id.
pub async fn next(
    State(db): State<DatabaseConnection>,
    Query(q): Query<NextQuery>,
) -> Result<Json<Value>, Failure> {
    one_of(
        "complexity",
        q.complexity.as_deref(),
        &["high", "medium", "low"],
    )?;
    let mut roles = Vec::new();
    for name in q.role.as_deref().into_iter().flat_map(|r| r.split(',')) {
        one_of("role", Some(name), &Role::NAMES)?;
        roles.extend(Role::parse(name));
    }
    one_of("priority", q.priority.as_deref(), &PRIORITIES)?;
    project_of(&db, &q.project).await?;
    let board = board(&db, &q.project).await?;
    let under = match &q.under {
        Some(id) => Some(rids_under(&db, &board, &q.project, id).await?),
        None => None,
    };
    let key = q.key.as_deref().map(str::to_uppercase);
    let rid = match &q.id {
        Some(id) => Some(item_of(&db, &q.project, id).await?.rid),
        None => None,
    };
    let dependencies = open_dependencies(&db, &q.project)
        .await
        .map_err(|e| internal(&e))?;
    let listed = crate::verbs::releases::listed(&db, &q.project)
        .await
        .map_err(|e| internal(&e))?;
    let release = match q.release.as_deref() {
        Some(given) => docket_core::release::resolve(&listed.all(), given)
            .map_err(|e| failure(StatusCode::BAD_REQUEST, &e.0))?,
        None => None,
    };
    let releases = listed.open;
    let filter = Filter {
        roles: &roles,
        priority: q
            .priority
            .as_deref()
            .and_then(|p| PRIORITIES.iter().position(|x| *x == p)),
        key: key.as_deref(),
        rid,
        under: under.as_ref(),
        complexity: q.complexity.as_deref(),
        theme: q.theme.as_deref(),
        area: q.area.as_deref(),
        release: release.as_deref(),
        releases: &releases,
        current_release_only: q.current_release,
        dependencies: &dependencies,
    };
    let candidates: Vec<Candidate> = board.items.iter().map(|r| candidate(r)).collect();
    let picked = queue(&candidates, &board.ties, &filter, q.n);
    Ok(Json(Value::Array(
        shaped(&db, &q.project, &board, picked).await?,
    )))
}

/// The picked rows in queue order, each carrying its effective tier as its priority, the role it is
/// taken in and how many items it unblocks. An audit also carries the runners already used under
/// its plan, with their counts, which the audit's runner is chosen from.
async fn shaped(
    db: &DatabaseConnection,
    slug: &str,
    board: &Board,
    picked: Vec<Ranked>,
) -> Result<Vec<Value>, Failure> {
    let rids: Vec<sea_orm::Value> = picked.iter().map(|r| r.rid.into()).collect();
    let tail = format!("AND rid IN ({})", marks(rids.len()));
    let rows = items_where(db, slug, &tail, rids).await?;
    let by_rid: HashMap<i64, _> = rows.into_iter().map(|r| (r.rid, r)).collect();
    let counts = member_counts(db, slug).await.map_err(|e| internal(&e))?;
    let mut out = Vec::with_capacity(picked.len());
    for ranked in picked {
        let Some(row) = by_rid.get(&ranked.rid) else {
            return Err(failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("rid {} vanished", ranked.rid),
            ));
        };
        let open = counts.get(&ranked.rid).copied().unwrap_or_default();
        let mut row = public(
            db,
            kind_of_type(&row.item_type),
            row.clone(),
            open,
            Some(ranked.tier),
        )
        .await
        .map_err(|e| internal(&e))?;
        row.insert("role".into(), json!(ranked.role.job()));
        row.insert("unblocks".into(), json!(ranked.unblocks));
        if ranked.role == Role::Audit {
            let runs = runner_counts(db, board, ranked.rid).await?;
            row.insert("audit_runners".into(), json!(runs));
        }
        out.push(Value::Object(row));
    }
    Ok(out)
}

/// The assignments under a plan, itself included, by the runner that took them.
async fn runner_counts(
    db: &DatabaseConnection,
    board: &Board,
    plan: i64,
) -> Result<BTreeMap<String, usize>, Failure> {
    let mut rids: Vec<i64> = descendants(&board.ties, plan).into_iter().collect();
    rids.push(plan);
    let marks_in = marks(rids.len());
    let rows = RunnerCount::find_by_statement(sql(
        &format!(
            "SELECT runner, COUNT(*) AS n FROM assignments WHERE runner IS NOT NULL \
             AND rid IN ({marks_in}) GROUP BY runner"
        ),
        rids.into_iter().map(Into::into).collect(),
    ))
    .all(db)
    .await
    .map_err(|e| internal(&e))?;
    Ok(rows
        .into_iter()
        .map(|r| (r.runner, usize::try_from(r.n).unwrap_or(0)))
        .collect())
}

#[derive(FromQueryResult)]
struct RunnerCount {
    runner: String,
    n: i64,
}

#[derive(Deserialize)]
pub struct InProject {
    project: String,
}

/// `GET /next/halt`: why the queue should stop being worked, when the project's last ended attempts
/// all failed, else null.
///
/// # Errors
/// 404 for an unknown project.
pub async fn halt(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    project_of(&db, &q.project).await?;
    let rows = Ended::find_by_statement(sql(
        "SELECT a.outcome FROM assignments a JOIN items i ON i.rid=a.rid \
         WHERE i.project=? AND a.kind='claim' AND a.ended_at IS NOT NULL AND a.outcome IS NOT NULL \
         ORDER BY a.ended_at DESC, a.id DESC LIMIT 3",
        vec![q.project.clone().into()],
    ))
    .all(&db)
    .await
    .map_err(|e| internal(&e))?;
    let outcomes: Vec<Outcome> = rows
        .iter()
        .filter_map(|r| Outcome::parse(&r.outcome))
        .collect();
    Ok(Json(
        json!({ "halt": docket_core::assignment::halt(&outcomes) }),
    ))
}

#[derive(FromQueryResult)]
struct Ended {
    outcome: String,
}

/// `docket status --json`: the flow counts over the tickets and over every item by key. The host is
/// the caller's.
///
/// # Errors
/// 404 for an unknown project.
pub async fn status(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    project_of(&db, &q.project).await?;
    let board = board(&db, &q.project).await?;
    let open = member_counts(&db, &q.project)
        .await
        .map_err(|e| internal(&e))?;
    let words: Vec<(&str, Kind, &str)> = board
        .items
        .iter()
        .map(|r| {
            let kind = kind_of_type(&r.item_type);
            let word = word_of(
                &r.state,
                r.claim_branch.as_deref(),
                r.wait_on.as_deref(),
                r.turn.as_deref(),
                kind,
                open.get(&r.rid).copied().unwrap_or_default(),
            );
            (r.key.as_str(), kind, word)
        })
        .collect();
    let (total, by_key) = tally(words);
    Ok(Json(json!({
        "project": q.project,
        "host": caller.host,
        "total": total.values().sum::<u64>(),
        "by_word": total,
        "by_key": by_key,
    })))
}
