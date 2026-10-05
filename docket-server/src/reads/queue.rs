use std::collections::{HashMap, HashSet};

use axum::extract::{Extension, Query, State};
use axum::{Json, http::StatusCode};
use sea_orm::{DatabaseConnection, FromQueryResult};
use serde::Deserialize;
use serde_json::{Value, json};

use docket_core::flow::tally;
use docket_core::member::{Edge, Tie, descendants, members_of};
use docket_core::queue::{Candidate, Filter, Role, next as queue};
use docket_core::word::{Facts, Kind, PRIORITIES, priority, word};

use crate::auth::Caller;
use crate::reads::public::{
    Failure, Kinds, failure, internal, item_of, items_where, marks, one_of, open_members,
    project_of, public, sql,
};

/// The columns the queue and the flow read, for every item of a project, in rid order.
#[derive(FromQueryResult)]
struct Slim {
    rid: i64,
    key: String,
    state: String,
    turn: Option<String>,
    claim_branch: Option<String>,
    wait_on: Option<String>,
    conflict: i64,
    complexity: Option<String>,
    theme: Option<String>,
    release: Option<String>,
    tags: Value,
    opened_at: String,
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
        "SELECT rid, key, state, turn, claim_branch, wait_on, conflict, complexity, theme, \
         (SELECT r.name FROM releases r WHERE r.id=items.release_id) AS release, tags, opened_at \
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

fn candidate<'a>(row: &'a Slim, kinds: &Kinds) -> Candidate<'a> {
    let tags: Vec<String> = serde_json::from_value(row.tags.clone()).unwrap_or_default();
    let own = priority(&tags);
    Candidate {
        rid: row.rid,
        key: &row.key,
        kind: kinds.kind(&row.key),
        open: row.state == "open",
        turn: row.turn.as_deref(),
        claimed: row.claim_branch.is_some(),
        waiting: row.wait_on.is_some(),
        conflict: row.conflict != 0,
        complexity: row.complexity.as_deref(),
        theme: row.theme.as_deref(),
        release: row.release.as_deref(),
        tier: PRIORITIES.iter().position(|p| *p == own).unwrap_or(2),
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
    theme: Option<String>,
    under: Option<String>,
    role: Option<String>,
    priority: Option<String>,
    #[serde(default)]
    current_release: bool,
}

fn ten() -> usize {
    10
}

/// The rids under a plan or story at any depth, or a concept's or central idea's members.
async fn rids_under(
    db: &DatabaseConnection,
    board: &Board,
    kinds: &Kinds,
    slug: &str,
    id: &str,
) -> Result<HashSet<i64>, Failure> {
    let target = item_of(db, slug, id).await?;
    if !kinds.kind(&target.key).is_standing() {
        return Ok(descendants(&board.ties, target.rid));
    }
    let standing: HashSet<i64> = board
        .items
        .iter()
        .filter(|i| kinds.kind(&i.key).is_standing())
        .map(|i| i.rid)
        .collect();
    Ok(members_of(&board.ties, &standing, target.rid))
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
    let project = project_of(db, slug).await?;
    let kinds = Kinds::of(&project);
    let board = board(db, slug).await?;
    let mut rids: Vec<i64> = rids_under(db, &board, &kinds, slug, id)
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
    let project = project_of(&db, &q.project).await?;
    let kinds = Kinds::of(&project);
    let board = board(&db, &q.project).await?;
    let under = match &q.under {
        Some(id) => Some(rids_under(&db, &board, &kinds, &q.project, id).await?),
        None => None,
    };
    let key = q.key.as_deref().map(str::to_uppercase);
    let releases = crate::verbs::releases::listed(&db, &q.project)
        .await
        .map_err(|e| internal(&e))?
        .open;
    let filter = Filter {
        roles: &roles,
        priority: q
            .priority
            .as_deref()
            .and_then(|p| PRIORITIES.iter().position(|x| *x == p)),
        key: key.as_deref(),
        under: under.as_ref(),
        complexity: q.complexity.as_deref(),
        theme: q.theme.as_deref(),
        releases: &releases,
        current_release_only: q.current_release,
    };
    let candidates: Vec<Candidate> = board.items.iter().map(|r| candidate(r, &kinds)).collect();
    let picked = queue(&candidates, &board.ties, &filter, q.n);
    Ok(Json(Value::Array(
        shaped(&db, &q.project, &kinds, picked).await?,
    )))
}

/// The picked rows in queue order, each carrying its effective tier as its priority.
async fn shaped(
    db: &DatabaseConnection,
    slug: &str,
    kinds: &Kinds,
    picked: Vec<(i64, usize)>,
) -> Result<Vec<Value>, Failure> {
    let rids: Vec<sea_orm::Value> = picked.iter().map(|(rid, _)| (*rid).into()).collect();
    let tail = format!("AND rid IN ({})", marks(rids.len()));
    let rows = items_where(db, slug, &tail, rids).await?;
    let by_rid: HashMap<i64, _> = rows.into_iter().map(|r| (r.rid, r)).collect();
    let counts = open_members(db, slug).await.map_err(|e| internal(&e))?;
    let mut out = Vec::with_capacity(picked.len());
    for (rid, tier) in picked {
        let Some(row) = by_rid.get(&rid) else {
            return Err(failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("rid {rid} vanished"),
            ));
        };
        let open = counts.get(&rid).copied().unwrap_or(0);
        let row = public(db, kinds.kind(&row.key), row.clone(), open, Some(tier))
            .await
            .map_err(|e| internal(&e))?;
        out.push(Value::Object(row));
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct InProject {
    project: String,
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
    let project = project_of(&db, &q.project).await?;
    let kinds = Kinds::of(&project);
    let board = board(&db, &q.project).await?;
    let open = open_members(&db, &q.project)
        .await
        .map_err(|e| internal(&e))?;
    let words: Vec<(&str, Kind, &str)> = board
        .items
        .iter()
        .map(|r| {
            let kind = kinds.kind(&r.key);
            let facts = Facts {
                state: &r.state,
                kind,
                claimed: r.claim_branch.is_some(),
                waiting: r.wait_on.is_some(),
                turn: r.turn.as_deref(),
            };
            let n = if kind == Kind::Package {
                open.get(&r.rid).copied().unwrap_or(0)
            } else {
                0
            };
            (r.key.as_str(), kind, word(&facts, n))
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
