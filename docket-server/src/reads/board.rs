//! A project at a glance: the caller, every project's counts, the flow, the check, the graph, the claims.

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Extension, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use futures_util::StreamExt;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, FromQueryResult, TransactionTrait};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use docket_core::board::Board;
use docket_core::clock::stamp;
use docket_core::item::Item;
use docket_core::member::{Edge, Tie};
use docket_core::pace::epoch;
use docket_core::rows::{HeldRow, ItemRow, Progress, ProjectRow};
use docket_core::stall::Wait;
use docket_core::word::Kind;

use crate::auth::Caller;
use crate::reads::public::{member_counts, sql, word_of};
use crate::reads::rows::{project_model, rows};
use crate::store::{self, STATE_COLUMNS, column};
use crate::verbs::Failure;
use crate::verbs::graph::{live_overlaps, open_dependencies, standing, waits};
use docket_core::word::kind_of_type;

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
            "open": by_state.get("open").copied().unwrap_or_default(),
            "done": by_state.get("done").copied().unwrap_or_default(),
            "dropped": by_state.get("dropped").copied().unwrap_or_default(),
            "last_event": last,
        }));
    }
    Ok(Json(Value::Array(out)))
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

/// `docket check`: the database's own checks, wait cycles, an item in two
/// packages, audits held or released wrongly, waits gone stale and bodies never written.
///
/// # Errors
/// 404 for an unknown project.
pub async fn check(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    let model = project_model(&db, &q.project).await?;
    let board = core_board(&db, &model).await?;
    Ok(Json(Value::Array(problems(&db, &q.project, &board).await?)))
}

/// Every problem `check` finds, in the order it reports them.
///
/// # Errors
/// The database.
pub async fn problems<C: ConnectionTrait>(
    db: &C,
    slug: &str,
    board: &Board,
) -> Result<Vec<Value>, Failure> {
    let mut out = Vec::new();
    out.extend(database_checks(db).await?);
    out.extend(stalls(db, slug, board).await?);
    let cutoff = stamp(now_secs().saturating_sub(30 * 86_400));
    let waiting: Vec<(i64, Wait)> = waits(db, slug)
        .await?
        .into_iter()
        .filter(|(_, w)| w.is_condition() && w.since < cutoff)
        .collect();
    let rids: Vec<i64> = waiting.iter().map(|(rid, _)| *rid).collect();
    let last = last_forward(db, slug, &rids).await?;
    let limit = i64::try_from(now_secs().saturating_sub(30 * 86_400)).unwrap_or(0);
    out.extend(
        waiting
            .into_iter()
            .filter(|(rid, _)| last.get(rid).is_none_or(|t| *t < limit))
            .map(|(rid, w)| {
                let id = board.by_rid(rid).map(|r| r.id.as_str());
                let until = board.by_rid(w.item).map(|r| r.title.as_str());
                problem(
                    "stale_wait",
                    json!({ "id": id, "since": w.since, "until": until }),
                )
            }),
    );
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

/// The ids of the open items an item of a later release holds up.
///
/// # Errors
/// The database cannot be read.
pub async fn held_later_ids<C: ConnectionTrait>(
    db: &C,
    slug: &str,
    board: &Board,
) -> Result<HashSet<String>, Failure> {
    Ok(stalls(db, slug, board)
        .await?
        .iter()
        .filter(|p| p["kind"] == "held_later")
        .filter_map(|p| p["id"].as_str().map(str::to_string))
        .collect())
}

/// What stalls the queue for good: open items holding each other in a cycle, through an item's wait
/// or a container's wait on its open members, and an item held by one in a later release.
async fn stalls<C: ConnectionTrait>(
    db: &C,
    slug: &str,
    board: &Board,
) -> Result<Vec<Value>, Failure> {
    let open = rows(
        db,
        &format!("SELECT {STATE_COLUMNS} FROM items WHERE project=? AND state='open' ORDER BY rid"),
        vec![slug.into()],
    )
    .await?;
    let by_rid: HashMap<i64, &crate::entities::item::Model> =
        open.iter().map(|r| (r.rid, r)).collect();
    let holds = Hold::find_by_statement(sql(
        "SELECT d.rid, d.on_rid AS to_rid FROM dependencies d JOIN items i ON i.rid=d.rid \
           WHERE i.project=? AND i.state='open' ORDER BY 1, 2",
        vec![slug.into()],
    ))
    .all(db)
    .await?;
    let mut edges: std::collections::BTreeMap<i64, Vec<i64>> = std::collections::BTreeMap::new();
    for h in holds {
        if by_rid.contains_key(&h.rid) && by_rid.contains_key(&h.to_rid) {
            edges.entry(h.rid).or_default().push(h.to_rid);
        }
    }
    let plans: Vec<i64> = open
        .iter()
        .filter(|r| kind_of_type(&r.item_type) == Kind::Audit)
        .map(|r| r.rid)
        .collect();
    let open_rids: HashSet<i64> = by_rid.keys().copied().collect();
    for (plan, member) in docket_core::stall::plan_edges(&board.ties, &plans, &open_rids) {
        edges.entry(plan).or_default().push(member);
    }
    let mut out: Vec<Value> = docket_core::stall::cycles(&edges)
        .into_iter()
        .map(|rid| problem("cycle", json!({ "id": by_rid[&rid].id })))
        .collect();
    let listed = crate::verbs::releases::listed(db, slug).await?;
    if !listed.open.is_empty() {
        let name = |rid: i64| listed.name(by_rid[&rid].release_id);
        let rank = by_rid
            .keys()
            .map(|&rid| {
                let at = docket_core::queue::release_rank(&listed.open, name(rid));
                (rid, at.unwrap_or(usize::MAX))
            })
            .collect();
        for (held, by) in docket_core::stall::held_later(&edges, &rank) {
            out.push(problem(
                "held_later",
                json!({
                    "id": by_rid[&held].id,
                    "by": by_rid[&by].id,
                    "release": name(held).unwrap_or("the backlog"),
                    "later": name(by).unwrap_or("the backlog"),
                }),
            ));
        }
    }
    let st = standing(db, slug).await?;
    for (rid, on) in open_dependencies(db, slug).await? {
        if docket_core::stall::abandoned(on, &st.targets) {
            out.push(problem(
                "abandoned",
                json!({ "id": st.id(rid), "on": st.id(on) }),
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

#[derive(Serialize)]
struct Node<'a> {
    id: &'a str,
    rid: i64,
    key: &'a str,
    kind: &'static str,
    state: &'a str,
    word: &'a str,
    release: Option<&'a str>,
    area: Option<&'a str>,
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<Progress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    due: Option<bool>,
}

#[derive(Serialize)]
struct GraphEdge<'a> {
    from: &'a str,
    to: Option<&'a str>,
    kind: &'a str,
}

/// A comma before every element of an array but its first.
fn separate(text: &mut Vec<u8>, written: usize) {
    if written > 0 {
        text.push(b',');
    }
}

fn to_writer(text: &mut Vec<u8>, value: &impl Serialize) -> Result<(), DbErr> {
    serde_json::to_writer(text, value).map_err(|e| DbErr::Custom(e.to_string()))
}

/// `docket graph`: every item with its rid, kind and word, its parent, and every link and cited file
/// leaving one.
///
/// The answer is written node by node and edge by edge from typed structs that borrow the board's
/// rows, and the links are read as a stream, so a request holds the board and the response text, not a
/// second tree of values or every link row beside them.
///
/// # Errors
/// 404 for an unknown project.
pub async fn graph(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<impl IntoResponse, Failure> {
    let model = project_model(&db, &q.project).await?;
    let board = core_board(&db, &model).await?;
    let open = member_counts(&db, &q.project).await?;
    let held_by = crate::store::held_in(&db, &q.project).await?;
    let mut items: Vec<&ItemRow> = board.items.iter().collect();
    items.sort_by(|a, b| (&a.state, a.rid).cmp(&(&b.state, b.rid)));
    let ids: HashMap<i64, &str> = items.iter().map(|r| (r.rid, r.id.as_str())).collect();
    let listed = crate::verbs::releases::listed(&db, &q.project).await?;
    let areas = crate::verbs::areas::listed(&db, &q.project).await?;
    let mut text = Vec::new();
    text.extend_from_slice(b"{\"project\":");
    to_writer(&mut text, &q.project)?;
    text.extend_from_slice(b",\"nodes\":[");
    for (i, r) in items.iter().enumerate() {
        let kind = kind_of_type(&r.item_type);
        let held = holds(kind);
        let node = Node {
            id: &r.id,
            rid: r.rid,
            key: &r.key,
            kind: kind.as_str(),
            state: &r.state,
            word: word_of(
                &r.state,
                held_by.get(&r.rid),
                r.wait_on.is_some(),
                kind,
                open.get(&r.rid).copied().unwrap_or_default(),
            ),
            release: listed.name(r.release_id),
            area: areas.name(r.area_id),
            title: &r.title,
            progress: held.then(|| board.held_progress(r)),
            due: (held && kind == Kind::Audit).then(|| board.due(r)),
        };
        separate(&mut text, i);
        to_writer(&mut text, &node)?;
    }
    text.extend_from_slice(b"],\"edges\":[");
    let mut written = 0;
    for r in &items {
        let Some(plan) = r.parent_rid.and_then(|p| ids.get(&p)) else {
            continue;
        };
        separate(&mut text, written);
        written += 1;
        let edge = GraphEdge {
            from: &r.id,
            to: Some(plan),
            kind: "parent",
        };
        to_writer(&mut text, &edge)?;
    }
    let mut links = LinkRow::find_by_statement(sql(
        "SELECT l.rid, l.kind, l.to_rid, l.to_path FROM links l JOIN items i ON i.rid=l.rid \
         WHERE i.project=? ORDER BY i.state, i.rid, l.kind, l.to_rid NULLS FIRST, l.to_path NULLS FIRST, \
         l.to_line NULLS FIRST, l.id",
        vec![q.project.clone().into()],
    ))
    .stream(&db)
    .await?;
    while let Some(l) = links.next().await {
        let l = l?;
        let Some(a) = ids.get(&l.rid) else { continue };
        let edge = match l.to_rid {
            Some(to) => {
                let Some(b) = ids.get(&to) else { continue };
                if l.kind == "related" && a > b {
                    continue;
                }
                GraphEdge {
                    from: a,
                    to: Some(b),
                    kind: &l.kind,
                }
            }
            None => GraphEdge {
                from: a,
                to: l.to_path.as_deref(),
                kind: "cites",
            },
        };
        separate(&mut text, written);
        written += 1;
        to_writer(&mut text, &edge)?;
    }
    text.extend_from_slice(b"]}");
    Ok(([(header::CONTENT_TYPE, "application/json")], text))
}

/// Kinds that hold other items, and so carry a progress: what a package, plan or story opened, what
/// belongs to a concept or idea.
fn holds(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Package | Kind::Audit | Kind::Story | Kind::Concept | Kind::Idea
    )
}

/// A project as docket-core's board reads it: every item with its parent, every `origin` tie, as the TUI loads it.
///
/// # Errors
/// The database.
pub async fn core_board<C>(
    db: &C,
    model: &crate::entities::project::Model,
) -> Result<Board, Failure>
where
    C: ConnectionTrait + sea_orm::StreamTrait,
{
    let project = ProjectRow {
        slug: model.slug.clone(),
        ..ProjectRow::default()
    };
    let waiting = waits(db, &model.slug).await?;
    let mut items: Vec<ItemRow> = Vec::new();
    let mut stream = crate::entities::item::Model::find_by_statement(sql(
        &format!("SELECT {STATE_COLUMNS} FROM items WHERE project=? ORDER BY rid"),
        vec![model.slug.clone().into()],
    ))
    .stream(db)
    .await?;
    while let Some(m) = stream.next().await {
        let m = m?;
        let wait = waiting.get(&m.rid);
        items.push(item_row(m, wait));
    }
    drop(stream);
    let held = crate::store::held_in(db, &model.slug).await?;
    for i in &mut items {
        i.hold(held.get(&i.rid));
    }
    let ties = TieRow::find_by_statement(sql(
        "SELECT l.rid, l.kind, l.to_rid FROM links l JOIN items i ON i.rid=l.rid \
         WHERE i.project=? AND l.to_rid IS NOT NULL AND l.kind='origin' ORDER BY l.id",
        vec![model.slug.clone().into()],
    ))
    .all(db)
    .await?
    .into_iter()
    .filter_map(|t| {
        Some(Tie {
            rid: t.rid,
            edge: Edge::parse(&t.kind)?,
            to: t.to_rid,
        })
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

/// A stored row as the board reads it, with what its dependencies hold it on.
fn item_row(m: crate::entities::item::Model, wait: Option<&Wait>) -> ItemRow {
    ItemRow {
        item_type: m.item_type,
        rid: m.rid,
        project: m.project,
        key: m.key,
        num: m.num,
        id: m.id,
        title: m.title,
        state: m.state,
        wait_on: wait.map(|w| w.on.to_string()),
        wait_ref: wait.map(|w| w.id.clone()),
        parent_rid: m.parent_rid,
        decision: m.decision,
        resolution: m.resolution,
        release_id: m.release_id,
        area_id: Some(m.area_id),
        opened_at: m.opened_at,
        updated_at: m.updated_at,
        ..ItemRow::default()
    }
}

#[derive(Deserialize)]
pub struct ByHost {
    project: String,
    host: Option<String>,
}

/// `/held`: each item of the project an open assignment holds, with its claim or the owner's ask.
///
/// # Errors
/// 404 for an unknown project.
pub async fn held(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<Json<Vec<HeldRow>>, Failure> {
    project_model(&db, &q.project).await?;
    let mut rows: Vec<HeldRow> = store::held_in(&db, &q.project)
        .await?
        .into_iter()
        .map(|(rid, held)| HeldRow { rid, held })
        .collect();
    rows.sort_by_key(|r| r.rid);
    Ok(Json(rows))
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
    let claimed = crate::store::claimed(&db, &q.project, q.host.as_deref()).await?;
    let flags = idle_flags(&db, &q.project, &model, &claimed).await?;
    let mut out = Vec::new();
    for item in claimed {
        let overlaps: Vec<Value> = live_overlaps(&db, &q.project, &item)
            .await?
            .into_iter()
            .map(|(h, paths)| {
                let claim = h.claim();
                json!({
                    "holder": h.id, "branch": claim.map(|c| &c.branch),
                    "host": claim.map(|c| &c.host), "paths": paths,
                })
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

/// `{rid: epoch}` of the last event that moved each of `rids` forward. Nothing is read for no rids.
async fn last_forward<C: ConnectionTrait>(
    db: &C,
    slug: &str,
    rids: &[i64],
) -> Result<HashMap<i64, i64>, Failure> {
    if rids.is_empty() {
        return Ok(HashMap::new());
    }
    let stamps = Stamp::find_by_statement(sql(
        "SELECT rid, at, kind FROM events WHERE project=? AND rid = ANY(?)",
        vec![slug.into(), rids.to_vec().into()],
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
    claimed: &[Item],
) -> Result<HashMap<i64, String>, Failure> {
    let limit = fact_int(db, model, "stale_claim", 120).await?;
    let rids: Vec<i64> = claimed.iter().map(|r| r.rid).collect();
    let last = last_forward(db, slug, &rids).await?;
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

/// What the status text reads beside the flow and the queue: the claims, the plans under
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
    let board = core_board(&db, &model).await?;
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
        "claims": claims(&db, &q.project, &model).await?,
        "plans": plans(&board),
        "plan_count": plan_count(&board),
        "due": due(&board),
        "problems": problems(&db, &q.project, &board).await?,
    })))
}

/// Every claim in the project, oldest first: its branch, the host it was claimed on, since when, and
/// a flag when nothing has moved on it for longer than `stale_claim`.
async fn claims<C: ConnectionTrait>(
    db: &C,
    slug: &str,
    model: &crate::entities::project::Model,
) -> Result<Value, Failure> {
    let claimed = crate::store::claimed(db, slug, None).await?;
    let flags = idle_flags(db, slug, model, &claimed).await?;
    Ok(json!(
        claimed
            .iter()
            .filter_map(|i| Some((i, i.claim()?)))
            .map(|(i, c)| {
                json!({
                    "id": i.id, "title": i.title, "branch": c.branch,
                    "host": c.host.split('.').next().unwrap_or_default(),
                    "since": epoch(&c.since),
                    "flag": flags.get(&i.rid),
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
                "id": p.id, "title": p.title, "done": g.done, "total": g.counted, "live": g.live
            }))
            .collect::<Vec<_>>()
    )
}

/// The plans under way and those due for their audit, as the word counts them.
fn plan_count(board: &Board) -> Value {
    let c = board.plan_count();
    json!({ "open": c.open, "audit_due": c.audit_due })
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
