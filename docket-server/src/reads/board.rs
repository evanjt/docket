//! A project at a glance: the caller, every project's counts, the flow, the check, the graph, the claims.

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Extension, Query, State};
use sea_orm::{ConnectionTrait, DatabaseConnection, FromQueryResult, TransactionTrait};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use docket_core::board::{Board, GateProblem};
use docket_core::clock::stamp;
use docket_core::member::Tie;
use docket_core::pace::epoch;
use docket_core::rows::{ItemRow, KeySpec, ProjectRow};
use docket_core::rules::GATE;
use docket_core::word::{Kind, word};

use crate::auth::Caller;
use crate::reads::public::{Kinds, facts, open_members, sql};
use crate::reads::rows::{project_model, rows};
use crate::store::{self, column, to_item};
use crate::verbs::Failure;
use crate::verbs::graph::live_overlaps;

#[derive(Deserialize)]
pub struct InProject {
    project: String,
}

/// Who the presented key belongs to.
pub async fn whoami(Extension(caller): Extension<Caller>) -> Json<Value> {
    Json(json!({ "host": caller.host, "owner": caller.owner }))
}

#[derive(FromQueryResult)]
struct StateCount {
    state: String,
    n: i64,
}

/// `docket projects`: every project with its open, done and dropped counts and its last event.
///
/// # Errors
/// The database.
pub async fn counts(State(db): State<DatabaseConnection>) -> Result<Json<Value>, Failure> {
    let slugs: Vec<String> = column(&db, "SELECT slug FROM projects ORDER BY slug", vec![]).await?;
    let mut out = Vec::new();
    for slug in slugs {
        let by_state: HashMap<String, i64> = StateCount::find_by_statement(sql(
            "SELECT state, COUNT(*) AS n FROM items WHERE project=? GROUP BY state",
            vec![slug.clone().into()],
        ))
        .all(&db)
        .await?
        .into_iter()
        .map(|c| (c.state, c.n))
        .collect();
        let last: Option<String> = store::scalar(
            &db,
            "SELECT MAX(at) AS at FROM events WHERE project=?",
            vec![slug.clone().into()],
        )
        .await?
        .flatten();
        out.push(json!({
            "slug": slug,
            "open": by_state.get("open").copied().unwrap_or(0),
            "done": by_state.get("done").copied().unwrap_or(0),
            "dropped": by_state.get("dropped").copied().unwrap_or(0),
            "last_event": last,
        }));
    }
    Ok(Json(Value::Array(out)))
}

/// `(word, count)` pairs in the order each word is first seen.
fn bump(pairs: &mut Vec<(String, u64)>, word: &str) {
    match pairs.iter_mut().find(|(w, _)| w == word) {
        Some(p) => p.1 += 1,
        None => pairs.push((word.to_string(), 1)),
    }
}

/// The word of every item of a project, by state then rid.
async fn words<C: ConnectionTrait>(
    db: &C,
    slug: &str,
    kinds: &Kinds,
) -> Result<Vec<(crate::entities::item::Model, String)>, Failure> {
    let open = open_members(db, slug).await?;
    let found = rows(
        db,
        "SELECT * FROM items WHERE project=? ORDER BY state, rid",
        vec![slug.into()],
    )
    .await?;
    Ok(found
        .into_iter()
        .map(|r| {
            let kind = kinds.kind(&r.key);
            let n = if kind == Kind::Package && r.state == "open" {
                open.get(&r.rid).copied().unwrap_or(0)
            } else {
                0
            };
            let w = word(&facts(&r, kind), n).to_string();
            (r, w)
        })
        .collect())
}

/// `docket status --json`: the flow over the tickets and every key, each count in the order its word
/// is first met.
///
/// # Errors
/// 404 for an unknown project.
pub async fn flow(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    let model = project_model(&db, &q.project).await?;
    let kinds = Kinds::of(&model);
    let mut total: Vec<(String, u64)> = Vec::new();
    let mut by_key: Vec<(String, Vec<(String, u64)>)> = Vec::new();
    for (r, w) in words(&db, &q.project, &kinds).await? {
        if kinds.kind(&r.key) != Kind::Package {
            bump(&mut total, &w);
        }
        match by_key.iter_mut().find(|(k, _)| *k == r.key) {
            Some((_, pairs)) => bump(pairs, &w),
            None => by_key.push((r.key.clone(), vec![(w, 1)])),
        }
    }
    Ok(Json(json!({
        "project": q.project,
        "host": caller.host,
        "total": total.iter().map(|(_, n)| n).sum::<u64>(),
        "by_word": total,
        "by_key": by_key,
    })))
}

/// One integrity problem, as data the client words.
fn problem(kind: &str, fields: Value) -> Value {
    let mut p = Map::new();
    p.insert("kind".into(), json!(kind));
    if let Value::Object(f) = fields {
        p.extend(f);
    }
    Value::Object(p)
}

/// `docket check`: conflicts, undefined keys, the database's own checks, wait cycles, an item in two
/// packages, audits held or released wrongly, waits gone stale and bodies never written.
///
/// # Errors
/// 404 for an unknown project.
pub async fn check(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    project_model(&db, &q.project).await?;
    Ok(Json(Value::Array(problems(&db, &q.project).await?)))
}

/// Every problem `check` finds, in the order it reports them.
///
/// # Errors
/// The database.
pub async fn problems<C: ConnectionTrait>(db: &C, slug: &str) -> Result<Vec<Value>, Failure> {
    let project = store::project(db, slug).await?;
    let mut out = Vec::new();
    let conflicts: Vec<String> = column(
        db,
        "SELECT id FROM items WHERE project=? AND conflict=1 ORDER BY state, rid",
        vec![slug.into()],
    )
    .await?;
    out.extend(
        conflicts
            .into_iter()
            .map(|id| problem("conflict", json!({ "id": id }))),
    );
    out.extend(undefined_keys(db, slug, &project).await?);
    out.extend(database_checks(db).await?);
    out.extend(stalls(db, slug).await?);
    out.extend(audits_held(db, slug).await?);
    let cutoff = stamp(now_secs().saturating_sub(30 * 86_400));
    let waiting = rows(
        db,
        "SELECT * FROM items WHERE project=? AND state='open' AND wait_on='condition' AND wait_since < ? \
         AND wait_ref<>? ORDER BY rid",
        vec![slug.into(), cutoff.clone().into(), GATE.into()],
    )
    .await?;
    let last = last_forward(db, slug).await?;
    let limit = i64::try_from(now_secs().saturating_sub(30 * 86_400)).unwrap_or(0);
    let stale: Vec<_> = waiting
        .into_iter()
        .filter(|r| last.get(&r.rid).is_none_or(|t| *t < limit))
        .collect();
    out.extend(stale.into_iter().map(|r| {
        problem(
            "stale_wait",
            json!({ "id": r.id, "since": r.wait_since, "until": r.wait_ref }),
        )
    }));
    let bare: Vec<String> = column(
        db,
        "SELECT id FROM items WHERE project=? AND state='open' AND body='' AND opened_at < ? ORDER BY rid",
        vec![slug.into(), cutoff[..10].to_string().into()],
    )
    .await?;
    out.extend(
        bare.into_iter()
            .map(|id| problem("no_body", json!({ "id": id }))),
    );
    Ok(out)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[derive(FromQueryResult)]
struct KeyCount {
    key: String,
    n: i64,
}

async fn undefined_keys<C: ConnectionTrait>(
    db: &C,
    slug: &str,
    project: &store::ProjectRow,
) -> Result<Vec<Value>, Failure> {
    let known: Vec<sea_orm::Value> = project
        .keys
        .iter()
        .filter_map(|k| k["key"].as_str().map(|s| s.to_string().into()))
        .collect();
    let text = format!(
        "SELECT key, COUNT(*) AS n FROM items WHERE project=? AND key NOT IN ({}) GROUP BY key ORDER BY key",
        if known.is_empty() {
            "''".to_string()
        } else {
            vec!["?"; known.len()].join(", ")
        }
    );
    let mut values: Vec<sea_orm::Value> = vec![slug.into()];
    values.extend(known);
    let found = KeyCount::find_by_statement(sql(&text, values))
        .all(db)
        .await?;
    Ok(found
        .into_iter()
        .map(|r| problem("undefined_key", json!({ "key": r.key, "n": r.n })))
        .collect())
}

/// The indexes Postgres cannot use and the check constraints it has not validated, by name; then the
/// foreign keys it has not validated, which may hold rows that break them.
async fn database_checks<C: ConnectionTrait>(db: &C) -> Result<Vec<Value>, Failure> {
    let mut out = Vec::new();
    let broken: Vec<String> = column(db, BROKEN, vec![]).await?;
    if !broken.is_empty() {
        out.push(problem("integrity", json!({ "result": broken.join(", ") })));
    }
    let unchecked: i64 = store::scalar(
        db,
        "SELECT COUNT(*) FROM pg_constraint k JOIN pg_namespace n ON n.oid = k.connamespace \
         WHERE n.nspname = current_schema() AND k.contype = 'f' AND NOT k.convalidated",
        vec![],
    )
    .await?
    .unwrap_or(0);
    if unchecked > 0 {
        out.push(problem("foreign_keys", json!({ "n": unchecked })));
    }
    Ok(out)
}

/// Invalid or unready indexes and unvalidated check constraints in the server's schema.
const BROKEN: &str = "SELECT 'index ' || c.relname FROM pg_index x JOIN pg_class c ON c.oid = x.indexrelid \
     JOIN pg_namespace n ON n.oid = c.relnamespace \
     WHERE n.nspname = current_schema() AND NOT (x.indisvalid AND x.indisready) \
     UNION ALL SELECT 'constraint ' || k.conname FROM pg_constraint k \
     JOIN pg_namespace n ON n.oid = k.connamespace \
     WHERE n.nspname = current_schema() AND k.contype = 'c' AND NOT k.convalidated \
     ORDER BY 1";

#[derive(FromQueryResult)]
struct Hold {
    rid: i64,
    to_rid: i64,
}

/// What stalls the queue for good: open items holding each other in a cycle, through an item's wait
/// or a container's wait on its open members, and an item held by one in a later release.
async fn stalls<C: ConnectionTrait>(db: &C, slug: &str) -> Result<Vec<Value>, Failure> {
    let open = rows(
        db,
        "SELECT * FROM items WHERE project=? AND state='open' ORDER BY rid",
        vec![slug.into()],
    )
    .await?;
    let by_rid: HashMap<i64, &crate::entities::item::Model> =
        open.iter().map(|r| (r.rid, r)).collect();
    let holds = Hold::find_by_statement(sql(
        "SELECT i.rid, i.wait_item AS to_rid FROM items i WHERE i.project=? AND i.state='open' \
           AND i.wait_on='item' AND i.wait_item IS NOT NULL \
         UNION ALL SELECT c.rid, l.rid FROM items c JOIN links l ON l.to_rid=c.rid AND l.kind='opened' \
           WHERE c.project=? AND c.state='open' AND c.wait_on='condition' AND c.wait_ref=? \
         ORDER BY 1, 2",
        vec![slug.into(), slug.into(), GATE.into()],
    ))
    .all(db)
    .await?;
    let mut edges: std::collections::BTreeMap<i64, Vec<i64>> = std::collections::BTreeMap::new();
    for h in holds {
        if by_rid.contains_key(&h.rid) && by_rid.contains_key(&h.to_rid) {
            edges.entry(h.rid).or_default().push(h.to_rid);
        }
    }
    let mut out: Vec<Value> = docket_core::stall::cycles(&edges)
        .into_iter()
        .map(|rid| problem("cycle", json!({ "id": by_rid[&rid].id })))
        .collect();
    let model = project_model(db, slug).await?;
    let releases = docket_core::fact::releases(model.skills["releases"].as_str());
    if releases.len() > 1 {
        let release_of = |rid: i64| {
            let theme = by_rid[&rid].theme.as_deref();
            docket_core::queue::release_rank(&releases, theme)
        };
        let rank = by_rid.keys().map(|&rid| (rid, release_of(rid))).collect();
        for (held, by) in docket_core::stall::held_later(&edges, &rank) {
            out.push(problem(
                "held_later",
                json!({
                    "id": by_rid[&held].id,
                    "by": by_rid[&by].id,
                    "release": releases[release_of(held)],
                    "later": releases[release_of(by)],
                }),
            ));
        }
    }
    Ok(out)
}

async fn audits_held<C: ConnectionTrait>(db: &C, slug: &str) -> Result<Vec<Value>, Failure> {
    let model = project_model(db, slug).await?;
    let board = core_board(db, &model).await?;
    Ok(board
        .gate_problems()
        .into_iter()
        .map(|g| match g {
            GateProblem::HeldGate { id } => problem("held_gate", json!({ "id": id })),
            GateProblem::OpenAudit { id, n } => problem("open_audit", json!({ "id": id, "n": n })),
        })
        .collect())
}

#[derive(FromQueryResult)]
struct LinkRow {
    rid: i64,
    kind: String,
    to_rid: Option<i64>,
    to_path: Option<String>,
}

/// `docket graph`: every item with its rid, kind and word, and every link and cited file leaving one.
///
/// # Errors
/// 404 for an unknown project.
pub async fn graph(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    let model = project_model(&db, &q.project).await?;
    let kinds = Kinds::of(&model);
    let items = words(&db, &q.project, &kinds).await?;
    let board = core_board(&db, &model).await?;
    let ids: HashMap<i64, &str> = items.iter().map(|(r, _)| (r.rid, r.id.as_str())).collect();
    let nodes: Vec<Value> = items
        .iter()
        .map(|(r, w)| {
            let kind = kinds.kind(&r.key);
            let mut node = json!({ "id": r.id, "rid": r.rid, "key": r.key, "kind": kind.as_str(), "state": r.state,
                                   "word": w, "theme": r.theme, "title": r.title });
            if let Some(row) = board.by_rid(r.rid).filter(|_| holds(kind)) {
                node["progress"] = json!(board.progress(row));
                if kind == Kind::Audit {
                    node["due"] = json!(board.due(row));
                }
            }
            node
        })
        .collect();
    let links = LinkRow::find_by_statement(sql(
        "SELECT l.* FROM links l JOIN items i ON i.rid=l.rid WHERE i.project=? ORDER BY i.state, i.rid, \
         l.kind, l.to_rid NULLS FIRST, l.to_path NULLS FIRST, l.to_line NULLS FIRST, l.id",
        vec![q.project.clone().into()],
    ))
    .all(&db)
    .await?;
    let mut edges = Vec::new();
    for l in links {
        let Some(a) = ids.get(&l.rid) else { continue };
        match l.to_rid {
            Some(to) => {
                let Some(b) = ids.get(&to) else { continue };
                if l.kind == "related" && a > b {
                    continue;
                }
                edges.push(json!({ "from": a, "to": b, "kind": l.kind }));
            }
            None => edges.push(json!({ "from": a, "to": l.to_path, "kind": "cites" })),
        }
    }
    Ok(Json(
        json!({ "project": q.project, "nodes": nodes, "edges": edges }),
    ))
}

/// Kinds that hold other items, and so carry a progress: what a package, plan or story opened, what
/// belongs to a concept or idea.
fn holds(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Package | Kind::Audit | Kind::Story | Kind::Concept | Kind::Idea
    )
}

/// A project as docket-core's board reads it: every item, the `opened` ties leaving them and the
/// `related` ties reaching or leaving a standing item, as the TUI loads it.
///
/// # Errors
/// The database.
pub async fn core_board<C: ConnectionTrait>(
    db: &C,
    model: &crate::entities::project::Model,
) -> Result<Board, Failure> {
    let project = ProjectRow {
        slug: model.slug.clone(),
        keys: model
            .keys
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|spec| serde_json::from_value::<KeySpec>(spec.clone()).ok())
            .collect(),
        ..ProjectRow::default()
    };
    let items: Vec<ItemRow> = rows(
        db,
        "SELECT * FROM items WHERE project=? ORDER BY rid",
        vec![model.slug.clone().into()],
    )
    .await?
    .into_iter()
    .map(item_row)
    .collect();
    let standing: HashSet<i64> = items
        .iter()
        .filter(|i| project.kind(&i.key).is_standing())
        .map(|i| i.rid)
        .collect();
    let ties = TieRow::find_by_statement(sql(
        "SELECT l.rid, l.kind, l.to_rid FROM links l JOIN items i ON i.rid=l.rid \
         WHERE i.project=? AND l.to_rid IS NOT NULL AND l.kind IN ('opened', 'related') ORDER BY l.id",
        vec![model.slug.clone().into()],
    ))
    .all(db)
    .await?
    .into_iter()
    .filter(|t| t.kind == "opened" || standing.contains(&t.rid) || standing.contains(&t.to_rid))
    .map(|t| Tie {
        rid: t.rid,
        opened: t.kind == "opened",
        to: t.to_rid,
    })
    .collect();
    Ok(Board::new(project, items, ties))
}

#[derive(FromQueryResult)]
struct TieRow {
    rid: i64,
    kind: String,
    to_rid: i64,
}

fn item_row(m: crate::entities::item::Model) -> ItemRow {
    ItemRow {
        tags: serde_json::from_value(m.tags).unwrap_or_default(),
        rid: m.rid,
        project: m.project,
        key: m.key,
        num: m.num,
        id: m.id,
        title: m.title,
        state: m.state,
        turn: m.turn,
        turn_note: m.turn_note,
        claim_branch: m.claim_branch,
        claim_host: m.claim_host,
        claim_since: m.claim_since,
        claim_job: m.claim_job,
        claim_on: m.claim_on,
        wait_on: m.wait_on,
        wait_item: m.wait_item,
        wait_ref: m.wait_ref,
        decision: m.decision,
        resolution: m.resolution,
        scope: m.scope,
        group_name: m.group_name,
        theme: m.theme,
        rank: m.rank,
        opened_at: m.opened_at,
        updated_at: m.updated_at,
    }
}

#[derive(Deserialize)]
pub struct ByHost {
    project: String,
    host: Option<String>,
}

/// `docket wip` beside its rows: each claim's flag when it looks abandoned, and the files it shares
/// with other claims.
///
/// # Errors
/// 404 for an unknown project.
pub async fn shares(
    State(db): State<DatabaseConnection>,
    Query(q): Query<ByHost>,
) -> Result<Json<Value>, Failure> {
    let model = project_model(&db, &q.project).await?;
    let (mut text, mut values) = (
        "SELECT * FROM items WHERE project=? AND claim_branch IS NOT NULL".to_string(),
        vec![sea_orm::Value::from(q.project.clone())],
    );
    if let Some(host) = q.host {
        text.push_str(" AND claim_host=?");
        values.push(host.into());
    }
    text.push_str(" ORDER BY claim_since, rid");
    let claimed = rows(&db, &text, values).await?;
    let flags = idle_flags(&db, &q.project, &model, &claimed).await?;
    let mut out = Vec::new();
    for r in claimed {
        let item = to_item(r);
        let overlaps: Vec<Value> = live_overlaps(&db, &q.project, &item)
            .await?
            .into_iter()
            .map(|(h, paths)| {
                json!({ "holder": h.id, "branch": h.claim_branch, "host": h.claim_host, "paths": paths })
            })
            .collect();
        out.push(json!({ "id": item.id, "flag": flags.get(&item.rid), "shares": overlaps }));
    }
    Ok(Json(Value::Array(out)))
}

#[derive(FromQueryResult)]
struct Stamp {
    rid: i64,
    at: String,
    kind: String,
}

/// `{rid: epoch}` of the last event that moved each item forward.
async fn last_forward<C: ConnectionTrait>(
    db: &C,
    slug: &str,
) -> Result<HashMap<i64, i64>, Failure> {
    let stamps = Stamp::find_by_statement(sql(
        "SELECT rid, at, kind FROM events WHERE project=? AND rid IS NOT NULL",
        vec![slug.into()],
    ))
    .all(db)
    .await?;
    let mut by_item: HashMap<i64, Vec<(String, i64)>> = HashMap::new();
    for e in stamps {
        if let Some(t) = epoch(&e.at) {
            by_item.entry(e.rid).or_default().push((e.kind, t));
        }
    }
    Ok(by_item
        .into_iter()
        .filter_map(|(rid, events)| {
            let events: Vec<(&str, i64)> = events.iter().map(|(k, t)| (k.as_str(), *t)).collect();
            docket_core::stall::idle_since(&events).map(|t| (rid, t))
        })
        .collect())
}

/// `{rid: flag}` for the claims with no forward event for longer than the project's `stale_claim` minutes.
async fn idle_flags<C: ConnectionTrait>(
    db: &C,
    slug: &str,
    model: &crate::entities::project::Model,
    claimed: &[crate::entities::item::Model],
) -> Result<HashMap<i64, String>, Failure> {
    let limit = fact_int(db, model, "stale_claim", 120).await?;
    let last = last_forward(db, slug).await?;
    let now = i64::try_from(now_secs()).unwrap_or(0);
    let mut out = HashMap::new();
    for r in claimed {
        let Some(t) = last.get(&r.rid) else { continue };
        let idle = (now - t).div_euclid(60);
        if idle >= limit {
            let flag = if idle >= 60 {
                format!("no event for {}h", idle.div_euclid(60))
            } else {
                format!("no event for {idle}m")
            };
            out.insert(r.rid, flag);
        }
    }
    Ok(out)
}

/// A numeric agent setting: the project's, else the owner-level one, else the default, also when
/// unreadable.
async fn fact_int<C: ConnectionTrait>(
    db: &C,
    model: &crate::entities::project::Model,
    key: &str,
    default: i64,
) -> Result<i64, Failure> {
    let project: std::collections::BTreeMap<String, String> = model
        .skills
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let owner = crate::facts::owner(db).await?;
    Ok(docket_core::fact::layered(&project, &owner, key)
        .and_then(|(v, _)| v.parse().ok())
        .unwrap_or(default))
}

/// What the status text reads beside the flow and the queue: the pace, the claims, the plans under
/// way, the plans due for audit and the check.
///
/// # Errors
/// 404 for an unknown project.
pub async fn summary(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    let db = db
        .begin_with_config(None, Some(sea_orm::AccessMode::ReadOnly))
        .await?;
    let model = project_model(&db, &q.project).await?;
    let project = store::project(&db, &q.project).await?;
    let board = core_board(&db, &model).await?;
    let now = i64::try_from(now_secs()).unwrap_or(0);
    let stored: std::collections::BTreeMap<String, String> = model
        .skills
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    Ok(Json(json!({
        "skills": docket_core::fact::known(&stored),
        "pace": pace(&db, &q.project, &project, now).await?,
        "net": net(&db, &model, now - 3600).await?,
        "claims": claims(&db, &q.project, &model).await?,
        "plans": plans(&board),
        "due": due(&board),
        "problems": problems(&db, &q.project).await?,
    })))
}

#[derive(FromQueryResult)]
struct MoveRow {
    kind: String,
    note: Option<String>,
    at: String,
    id: String,
    key: String,
}

/// The closes and working seconds over the last twenty closes.
async fn pace<C: ConnectionTrait>(
    db: &C,
    slug: &str,
    project: &store::ProjectRow,
    now: i64,
) -> Result<Value, Failure> {
    use docket_core::pace::{Logged, moves, pace as pace_of};
    let found = MoveRow::find_by_statement(sql(
        "SELECT e.kind, e.note, e.at, i.id, i.key FROM events e JOIN items i ON i.rid = e.rid \
         WHERE e.project=? ORDER BY e.at, e.seq",
        vec![slug.into()],
    ))
    .all(db)
    .await?;
    let standing = crate::verbs::graph::keys_of(project, &[Kind::Concept, Kind::Idea]);
    let events: Vec<Logged> = found
        .iter()
        .filter(|r| !standing.contains(&r.key))
        .map(|r| Logged {
            at: epoch(&r.at).unwrap_or(i64::MIN),
            id: &r.id,
            kind: &r.kind,
            note: r.note.as_deref(),
        })
        .collect();
    // An event with no readable time still marks a legacy close as logged, and moves nothing itself.
    let timed: Vec<_> = moves(&events)
        .into_iter()
        .filter(|m| m.at != i64::MIN)
        .collect();
    let p = pace_of(&timed, 20, now);
    Ok(json!({ "closed": p.closed, "working": p.working }))
}

#[derive(FromQueryResult)]
struct NetRow {
    kind: String,
    at: String,
    key: String,
    theme: Option<String>,
    opened: i64,
}

/// The current release's tickets closed and opened since `since`, and the research closes that opened
/// nothing.
async fn net<C: ConnectionTrait>(
    db: &C,
    model: &crate::entities::project::Model,
    since: i64,
) -> Result<docket_core::pace::Net, Failure> {
    use docket_core::pace::{Class, Counted, net as net_of};
    let found = NetRow::find_by_statement(sql(
        "SELECT e.kind, e.at, i.key, i.theme, \
           (SELECT COUNT(*) FROM links l WHERE l.to_rid=i.rid AND l.kind='opened') AS opened \
         FROM events e JOIN items i ON i.rid = e.rid WHERE e.project=? AND e.at >= ? ORDER BY e.at, e.seq",
        vec![
            model.slug.clone().into(),
            stamp(u64::try_from(since).unwrap_or(0)).into(),
        ],
    ))
    .all(db)
    .await?;
    let kinds = Kinds::of(model);
    let releases = docket_core::fact::releases(model.skills["releases"].as_str());
    let counted: Vec<Counted> = found
        .iter()
        .map(|r| Counted {
            at: epoch(&r.at).unwrap_or(i64::MIN),
            kind: &r.kind,
            class: match kinds.kind(&r.key) {
                Kind::Work => Class::Code {
                    current: docket_core::queue::release_rank(&releases, r.theme.as_deref()) == 0,
                },
                Kind::Decision | Kind::Research => Class::Research {
                    opened: r.opened > 0,
                },
                _ => Class::Other,
            },
        })
        .collect();
    Ok(net_of(&counted, since))
}

/// Every claim in the project, oldest first: its branch, the host it was claimed on, since when, and
/// a flag when nothing has moved on it for longer than `stale_claim`.
async fn claims<C: ConnectionTrait>(
    db: &C,
    slug: &str,
    model: &crate::entities::project::Model,
) -> Result<Value, Failure> {
    let text = "SELECT * FROM items WHERE project=? AND state='open' AND claim_branch IS NOT NULL \
                ORDER BY claim_since, rid";
    let claimed = rows(db, text, vec![slug.into()]).await?;
    let flags = idle_flags(db, slug, model, &claimed).await?;
    Ok(json!(
        claimed
            .iter()
            .map(|c| {
                let host = c.claim_host.as_deref().unwrap_or_default();
                json!({
                    "id": c.id, "title": c.title, "branch": c.claim_branch,
                    "host": host.split('.').next().unwrap_or_default(),
                    "since": c.claim_since.as_deref().and_then(epoch),
                    "flag": flags.get(&c.rid),
                })
            })
            .collect::<Vec<_>>()
    ))
}

/// Open plans that opened anything: done, total and claimed of all they opened at any depth, those
/// being worked first, then the nearest done.
/// The open plans started, as the core counts them: those being worked first, then the nearest done.
fn plans(board: &Board) -> Value {
    json!(
        board
            .plans_under_way()
            .into_iter()
            .map(|(p, g)| json!({
                "id": p.id, "title": p.title, "done": g.done, "total": g.total, "live": g.live
            }))
            .collect::<Vec<_>>()
    )
}

/// The plans due for their audit, as the core decides it.
fn due(board: &Board) -> Value {
    json!(
        board
            .due_audits()
            .into_iter()
            .map(|p| json!({ "id": p.id, "title": p.title }))
            .collect::<Vec<_>>()
    )
}

#[cfg(test)]
#[path = "../tests/board.rs"]
mod tests;
