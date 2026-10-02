//! A project at a glance: the caller, every project's counts, the flow, the check, the graph, the claims.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Extension, Query, State};
use sea_orm::{DatabaseConnection, FromQueryResult};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use docket_core::clock::stamp;
use docket_core::pace::epoch;
use docket_core::rules::GATE;
use docket_core::word::{Kind, word};

use crate::auth::Caller;
use crate::reads::public::{Kinds, facts, open_members, sql};
use crate::reads::rows::{marks, project_model, rows};
use crate::store::{self, column, to_item};
use crate::verbs::Failure;
use crate::verbs::graph::{audits_over, live_overlaps, open_under};

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
async fn words(
    db: &DatabaseConnection,
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
pub async fn problems(db: &DatabaseConnection, slug: &str) -> Result<Vec<Value>, Failure> {
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
    out.extend(cycles(db, slug).await?);
    out.extend(audits_held(db, slug, &project).await?);
    let cutoff = stamp(now_secs().saturating_sub(30 * 86_400));
    let stale = rows(
        db,
        "SELECT * FROM items WHERE project=? AND state='open' AND wait_on='condition' AND wait_since < ? \
         AND wait_ref<>? ORDER BY rid",
        vec![slug.into(), cutoff.clone().into(), GATE.into()],
    )
    .await?;
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

async fn undefined_keys(
    db: &DatabaseConnection,
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
async fn database_checks(db: &DatabaseConnection) -> Result<Vec<Value>, Failure> {
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

async fn cycles(db: &DatabaseConnection, slug: &str) -> Result<Vec<Value>, Failure> {
    let waiting = rows(
        db,
        "SELECT * FROM items WHERE project=? AND state='open' AND wait_on='item' ORDER BY rid",
        vec![slug.into()],
    )
    .await?;
    let mut out = Vec::new();
    for r in waiting {
        let mut seen = std::collections::HashSet::new();
        let mut cur = Some(r.rid);
        while let Some(c) = cur {
            if !seen.insert(c) {
                out.push(problem("cycle", json!({ "id": r.id })));
                break;
            }
            cur = store::by_rid(db, c)
                .await?
                .filter(|t| t.state == "open")
                .and_then(|t| t.wait_item);
        }
    }
    Ok(out)
}

async fn audits_held(
    db: &DatabaseConnection,
    slug: &str,
    project: &store::ProjectRow,
) -> Result<Vec<Value>, Failure> {
    let open: Vec<i64> = column(
        db,
        "SELECT rid FROM items WHERE project=? AND state='open' ORDER BY rid",
        vec![slug.into()],
    )
    .await?;
    let mut out = Vec::new();
    for a in audits_over(db, project, &open).await? {
        let pending = open_under(db, a.rid).await?;
        let gated =
            a.wait_on.as_deref() == Some("condition") && a.wait_ref.as_deref() == Some(GATE);
        if gated && pending.is_empty() {
            out.push(problem("held_gate", json!({ "id": a.id })));
        } else if !pending.is_empty() && a.wait_on.is_none() && a.claim_branch.is_none() {
            out.push(problem(
                "open_audit",
                json!({ "id": a.id, "n": pending.len() }),
            ));
        }
    }
    Ok(out)
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
    let ids: HashMap<i64, &str> = items.iter().map(|(r, _)| (r.rid, r.id.as_str())).collect();
    let nodes: Vec<Value> = items
        .iter()
        .map(|(r, w)| {
            json!({ "id": r.id, "rid": r.rid, "key": r.key, "kind": kinds.kind(&r.key).as_str(), "state": r.state,
                    "word": w, "theme": r.theme, "title": r.title })
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
}

/// `{rid: flag}` for the claims with no event for longer than the project's `stale_claim` minutes.
async fn idle_flags(
    db: &DatabaseConnection,
    slug: &str,
    model: &crate::entities::project::Model,
    claimed: &[crate::entities::item::Model],
) -> Result<HashMap<i64, String>, Failure> {
    let limit = fact_int(model, "stale_claim", 120);
    let mut last: HashMap<i64, i64> = HashMap::new();
    let stamps = Stamp::find_by_statement(sql(
        "SELECT rid, at FROM events WHERE project=? AND rid IS NOT NULL",
        vec![slug.into()],
    ))
    .all(db)
    .await?;
    for e in stamps {
        if let Some(t) = epoch(&e.at) {
            let best = last.entry(e.rid).or_insert(t);
            *best = (*best).max(t);
        }
    }
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

/// A numeric fact of the project, its default when unset or unreadable.
fn fact_int(model: &crate::entities::project::Model, key: &str, default: i64) -> i64 {
    model.skills[key]
        .as_str()
        .filter(|v| !v.is_empty())
        .map_or(Some(default), |v| v.parse().ok())
        .unwrap_or(default)
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
    let model = project_model(&db, &q.project).await?;
    let project = store::project(&db, &q.project).await?;
    let now = i64::try_from(now_secs()).unwrap_or(0);
    Ok(Json(json!({
        "skills": model.skills,
        "pace": pace(&db, &q.project, &project, now).await?,
        "claims": claims(&db, &q.project, &model).await?,
        "plans": plans(&db, &q.project, &project).await?,
        "due": due(&db, &q.project, &project).await?,
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
async fn pace(
    db: &DatabaseConnection,
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

/// Every claim in the project, oldest first: its branch, the host it was claimed on, since when, and
/// a flag when nothing has moved on it for longer than `stale_claim`.
async fn claims(
    db: &DatabaseConnection,
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
async fn plans(
    db: &DatabaseConnection,
    slug: &str,
    project: &store::ProjectRow,
) -> Result<Value, Failure> {
    let mut out: Vec<(String, String, usize, usize, usize)> = Vec::new();
    for p in open_plans(db, slug, project).await? {
        let mut g = (0, 0, 0);
        for rid in crate::verbs::graph::opened_under(db, p.rid).await? {
            if let Some(m) = store::by_rid(db, rid).await? {
                g.0 += usize::from(m.state != "open");
                g.1 += 1;
                g.2 += usize::from(m.claim_branch.is_some());
            }
        }
        if g.1 > 0 {
            out.push((p.id, p.title, g.0, g.1, g.2));
        }
    }
    out.sort_by(|a, b| {
        let near = |x: &(String, String, usize, usize, usize)| x.2 * 1000 / x.3;
        (b.4, near(b), a.3 - a.2, &a.0).cmp(&(a.4, near(a), b.3 - b.2, &b.0))
    });
    Ok(json!(
        out.into_iter()
            .map(|(id, title, done, total, live)| json!({
                "id": id, "title": title, "done": done, "total": total, "live": live
            }))
            .collect::<Vec<_>>()
    ))
}

/// Plans due for audit: open, nobody's claim, waiting on nothing, and everything they opened, at any
/// depth, closed.
async fn due(
    db: &DatabaseConnection,
    slug: &str,
    project: &store::ProjectRow,
) -> Result<Value, Failure> {
    let mut out = Vec::new();
    for p in open_plans(db, slug, project).await? {
        if p.claim_branch.is_some() || p.wait_on.is_some() || p.turn.as_deref() != Some("agent") {
            continue;
        }
        let opened = crate::verbs::graph::opened_under(db, p.rid).await?;
        if !opened.is_empty() && open_under(db, p.rid).await?.is_empty() {
            out.push(json!({ "id": p.id, "title": p.title }));
        }
    }
    Ok(json!(out))
}

async fn open_plans(
    db: &DatabaseConnection,
    slug: &str,
    project: &store::ProjectRow,
) -> Result<Vec<docket_core::item::Item>, Failure> {
    let mut keys = crate::verbs::graph::keys_of(project, &[Kind::Audit]);
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    keys.sort();
    let mut values: Vec<sea_orm::Value> = vec![slug.into()];
    values.extend(keys.iter().map(|k| k.clone().into()));
    Ok(store::items(
        db,
        &format!(
            "SELECT * FROM items WHERE project=? AND state='open' AND key IN ({}) ORDER BY key, num",
            marks(keys.len())
        ),
        values,
    )
    .await?)
}

#[cfg(test)]
#[path = "../tests/board.rs"]
mod tests;
