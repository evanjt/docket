//! What a verb reads around an item: its package, the audits over it, its neighbours.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use sea_orm::{ConnectionTrait, DbErr, QueryResult};
use serde_json::json;

use docket_core::item::Item;
use docket_core::member::{Edge, Tie, descendants};
use docket_core::metrics::{self, Progress, Subject};
use docket_core::release::Listed;
use docket_core::stall::{Target, Wait};
use docket_core::text::split_id;
use docket_core::touch::{declared_files, fold_paths, same_file};
use docket_core::word::ItemType;

use crate::store::{ASKED, ProjectRow, STATE_COLUMNS, Tx, column, items, sql};
use crate::verbs::Failure;

pub use crate::reads::public::member_count;

/// Every parent edge of the project, child to plan.
pub async fn parent_ties<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Vec<Tie>, DbErr> {
    let rows = c
        .query_all_raw(sql(
            "SELECT rid, parent_rid FROM items WHERE project=? AND parent_rid IS NOT NULL ORDER BY rid",
            vec![slug.into()],
        ))
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(Tie {
            rid: r.try_get_by_index(0)?,
            edge: Edge::Parent,
            to: r.try_get_by_index(1)?,
        });
    }
    Ok(out)
}

/// What a package holds: its children, the direct ones only.
pub async fn package_members<C: ConnectionTrait>(
    c: &C,
    prid: i64,
    open_only: bool,
) -> Result<Vec<Item>, DbErr> {
    let open = if open_only { "AND state='open' " } else { "" };
    items(
        c,
        &format!("SELECT * FROM items WHERE parent_rid=? {open}ORDER BY key, num"),
        vec![prid.into()],
    )
    .await
}

/// How far a package is, over its direct children, by `metrics::progress`.
pub async fn package_progress<C: ConnectionTrait>(c: &C, prid: i64) -> Result<Progress, DbErr> {
    let ms = package_members(c, prid, false).await?;
    Ok(member_progress(&ms))
}

/// `metrics::progress` over rows read from the store, where an open item has no word to read.
#[must_use]
fn member_progress(ms: &[Item]) -> Progress {
    let subjects: Vec<Subject> = ms
        .iter()
        .map(|m| Subject::member(&m.id, &m.state, m.claim().is_some()))
        .collect();
    metrics::progress(&subjects)
}

/// The open items under rid at any depth, by rid.
pub async fn open_under<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    rid: i64,
) -> Result<Vec<Item>, DbErr> {
    let under = descendants(&parent_ties(c, slug).await?, rid);
    if under.is_empty() {
        return Ok(Vec::new());
    }
    Ok(items(
        c,
        &format!(
            "SELECT {STATE_COLUMNS} FROM items WHERE project=? AND state='open' AND parent_rid IS NOT NULL \
             ORDER BY rid"
        ),
        vec![slug.into()],
    )
    .await?
    .into_iter()
    .filter(|i| under.contains(&i.rid))
    .collect())
}

/// How each item of a project stands as something depended on, its id, and which items are
/// conditions: on the owner's turn and not questions.
pub struct Standing {
    pub targets: BTreeMap<i64, Target>,
    pub ids: BTreeMap<i64, String>,
    pub conditions: BTreeSet<i64>,
}

/// The columns a standing reads of an item, in the order `Standing::add` takes them. An item is on
/// the owner's turn while an ask of theirs is open on it.
static STANDING: LazyLock<String> = LazyLock::new(|| {
    format!(
        "rid, id, state, decision IS NOT NULL, superseded_by, \
         COALESCE(state='open' AND type<>'question' AND {ASKED}, false)"
    )
});

impl Standing {
    fn empty() -> Self {
        Standing {
            targets: BTreeMap::new(),
            ids: BTreeMap::new(),
            conditions: BTreeSet::new(),
        }
    }

    fn add(&mut self, r: &QueryResult) -> Result<(), DbErr> {
        let rid: i64 = r.try_get_by_index(0)?;
        let state: String = r.try_get_by_index(2)?;
        let target = Target::of(&state, r.try_get_by_index(3)?, r.try_get_by_index(4)?);
        self.targets.insert(rid, target);
        self.ids.insert(rid, r.try_get_by_index(1)?);
        if r.try_get_by_index::<bool>(5)? {
            self.conditions.insert(rid);
        }
        Ok(())
    }

    pub fn id(&self, rid: i64) -> String {
        self.ids.get(&rid).cloned().unwrap_or_default()
    }

    /// The ids of a path, the first item repeated at its end.
    pub fn cycle(&self, path: &[i64]) -> String {
        let mut ids: Vec<String> = path.iter().map(|r| self.id(*r)).collect();
        ids.extend(path.first().map(|r| self.id(*r)));
        ids.join(" -> ")
    }

    /// What an item with these dependencies, `(on, created_at)` oldest first, waits on.
    pub fn wait(&self, deps: &[(i64, String)]) -> Option<Wait> {
        docket_core::stall::wait(deps, &self.targets, &self.ids, &self.conditions)
    }
}

pub async fn standing<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Standing, DbErr> {
    let rows = c
        .query_all_raw(sql(
            &format!("SELECT {} FROM items WHERE project=?", *STANDING),
            vec![slug.into()],
        ))
        .await?;
    let mut out = Standing::empty();
    for r in rows {
        out.add(&r)?;
    }
    Ok(out)
}

/// The order of one item's dependencies, `d`, on the items `o`: oldest first, and those made in the
/// same second by the id of what they depend on, which a restore keeps.
const OLDEST: &str = "d.created_at, o.key, o.num";

/// What rid depends on, oldest first.
pub async fn depends_on<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Vec<i64>, DbErr> {
    Ok(dependencies_of(c, rid)
        .await?
        .into_iter()
        .map(|(on, _)| on)
        .collect())
}

/// What rid depends on, each with when the dependency was made, oldest first.
pub async fn dependencies_of<C: ConnectionTrait>(
    c: &C,
    rid: i64,
) -> Result<Vec<(i64, String)>, DbErr> {
    let rows = c
        .query_all_raw(sql(
            &format!(
                "SELECT d.on_rid, d.created_at FROM dependencies d JOIN items o ON o.rid=d.on_rid \
                 WHERE d.rid=? ORDER BY {OLDEST}"
            ),
            vec![rid.into()],
        ))
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push((r.try_get_by_index(0)?, r.try_get_by_index(1)?));
    }
    Ok(out)
}

/// The waits of the open items whose dependencies match `filter`, a condition on `d`, the dependency,
/// and `i`, the item that depends: each read from its dependencies alone, and the items that hold
/// them followed through their successors. The one reader of what an item waits on.
pub async fn waits_where<C: ConnectionTrait>(
    c: &C,
    filter: &str,
    values: Vec<sea_orm::Value>,
) -> Result<BTreeMap<i64, Wait>, DbErr> {
    let rows = c
        .query_all_raw(sql(
            &format!(
                "SELECT d.rid, d.on_rid, d.created_at FROM dependencies d JOIN items i ON i.rid=d.rid \
                 JOIN items o ON o.rid=d.on_rid WHERE i.state='open' AND {filter} ORDER BY d.rid, {OLDEST}"
            ),
            values.clone(),
        ))
        .await?;
    let mut deps: BTreeMap<i64, Vec<(i64, String)>> = BTreeMap::new();
    for r in rows {
        deps.entry(r.try_get_by_index(0)?)
            .or_default()
            .push((r.try_get_by_index(1)?, r.try_get_by_index(2)?));
    }
    if deps.is_empty() {
        return Ok(BTreeMap::new());
    }
    let held = c
        .query_all_raw(sql(
            &format!(
                "WITH RECURSIVE chain(rid) AS ( \
                   SELECT d.on_rid FROM dependencies d JOIN items i ON i.rid=d.rid \
                   WHERE i.state='open' AND {filter} \
                   UNION SELECT s.superseded_by FROM items s JOIN chain ON s.rid=chain.rid \
                   WHERE s.state='dropped' AND s.superseded_by IS NOT NULL) \
                 SELECT {standing} FROM items JOIN chain USING (rid)",
                standing = *STANDING
            ),
            values,
        ))
        .await?;
    let mut st = Standing::empty();
    for r in held {
        st.add(&r)?;
    }
    Ok(deps
        .into_iter()
        .filter_map(|(rid, d)| st.wait(&d).map(|w| (rid, w)))
        .collect())
}

/// What each open item of a project waits on, by rid; an item not waiting is absent.
pub async fn waits<C: ConnectionTrait>(c: &C, slug: &str) -> Result<BTreeMap<i64, Wait>, DbErr> {
    waits_where(c, "i.project=?", vec![slug.into()]).await
}

/// What one item waits on, or none when it is closed or nothing holds it.
pub async fn wait_of<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Option<Wait>, DbErr> {
    Ok(waits_where(c, "d.rid=?", vec![rid.into()])
        .await?
        .remove(&rid))
}

/// Every open item's dependencies, as `(rid, on_rid)`, by item.
pub async fn open_dependencies<C: ConnectionTrait>(
    c: &C,
    slug: &str,
) -> Result<Vec<(i64, i64)>, DbErr> {
    let rows = c
        .query_all_raw(sql(
            &format!(
                "SELECT d.rid, d.on_rid FROM dependencies d JOIN items i ON i.rid=d.rid \
                 JOIN items o ON o.rid=d.on_rid WHERE i.project=? AND i.state='open' \
                 ORDER BY d.rid, {OLDEST}"
            ),
            vec![slug.into()],
        ))
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push((r.try_get_by_index(0)?, r.try_get_by_index(1)?));
    }
    Ok(out)
}

/// What holds each open item: each dependency not yet satisfied, by the item that still holds it,
/// and, for an open plan, each of its open children.
pub async fn holds_of<C: ConnectionTrait>(
    c: &C,
    p: &ProjectRow,
    st: &Standing,
) -> Result<BTreeMap<i64, Vec<i64>>, DbErr> {
    let mut out: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for (rid, on) in open_dependencies(c, &p.rules.slug).await? {
        if let Some(h) = docket_core::stall::holder(on, &st.targets) {
            out.entry(rid).or_default().push(h);
        }
    }
    let rows = c
        .query_all_raw(sql(
            "SELECT a.rid, m.rid FROM items m JOIN items a ON a.rid=m.parent_rid \
             WHERE a.project=? AND a.state='open' AND m.state='open' AND a.type=? \
             ORDER BY a.rid, m.rid",
            vec![p.rules.slug.clone().into(), ItemType::Plan.as_str().into()],
        ))
        .await?;
    for r in rows {
        let plan: i64 = r.try_get_by_index(0)?;
        out.entry(plan).or_default().push(r.try_get_by_index(1)?);
    }
    Ok(out)
}

/// How a change to what an item depends on moved it.
pub enum Rewait {
    Same,
    /// It waits now, and did not before.
    Began,
    /// It waits on another item than before.
    Switched,
    Resumed(Box<Item>),
}

/// What follows from an item's wait moving from `before` to what its dependencies hold it on now.
/// An item that begins to wait gives up its claim with the `waited` event its caller writes; one held
/// by another item than before records it; one that resumes goes to the agents, unless the owner was
/// asked something during the wait, which still stands.
pub async fn rewait(
    tx: &mut Tx,
    slug: &str,
    r: &Item,
    before: Option<&Wait>,
    why: &str,
) -> Result<Rewait, Failure> {
    if r.state != "open" {
        return Ok(Rewait::Same);
    }
    let now = wait_of(&tx.conn, r.rid).await?;
    match (before, now) {
        (Some(b), Some(n)) if b.item == n.item => Ok(Rewait::Same),
        (Some(_), Some(n)) => {
            let note = if why.is_empty() {
                format!("on {}", n.id)
            } else {
                format!("on {} ({why})", n.id)
            };
            tx.event(slug, Some(r.rid), "waited", Some(&note), None, None)
                .await?;
            Ok(Rewait::Switched)
        }
        (None, Some(_)) => Ok(Rewait::Began),
        (Some(b), None) => {
            let asked_since = r.ask().is_some_and(|a| a.since > b.since);
            let data = asked_since.then(|| json!({ "ask": docket_core::assignment::KEPT }));
            tx.update(r.rid, &[]).await?;
            tx.event(slug, Some(r.rid), "resumed", Some(why), None, data.as_ref())
                .await?;
            Ok(Rewait::Resumed(Box::new(tx.fresh(r.rid).await?)))
        }
        (None, None) => Ok(Rewait::Same),
    }
}

/// The open items with a dependency that reaches `target`, itself or through a successor, by rid.
async fn dependants_of<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    target: i64,
    st: &Standing,
) -> Result<Vec<i64>, DbErr> {
    let out: BTreeSet<i64> = open_dependencies(c, slug)
        .await?
        .into_iter()
        .filter(|(_, on)| docket_core::stall::passes(*on, target, &st.targets))
        .map(|(rid, _)| rid)
        .collect();
    Ok(out.into_iter().collect())
}

/// Refuse reopening `r`, already written open in `tx`, when what holds it, what it holds, or its
/// plan would close a cycle through it, naming the path.
pub async fn refuse_reopen_cycle<C: ConnectionTrait>(
    c: &C,
    p: &ProjectRow,
    r: &Item,
) -> Result<(), Failure> {
    let st = standing(c, &p.rules.slug).await?;
    let holds = holds_of(c, p, &st).await?;
    for h in holds.get(&r.rid).into_iter().flatten() {
        if let Some(path) = docket_core::stall::path(&holds, *h, r.rid) {
            let mut around = vec![r.rid];
            around.extend(&path[..path.len() - 1]);
            return Err(Failure::Refused(format!(
                "{} cannot be reopened: it would close the cycle {}.",
                r.id,
                st.cycle(&around)
            )));
        }
    }
    Ok(())
}

/// Refuse dropping `r` for `s` when every dependency on `r` passing to what holds `s` would put a
/// dependant in a later release than its new holder, or close a cycle, naming the path.
pub async fn refuse_successor(
    tx: &Tx,
    p: &ProjectRow,
    r: &Item,
    s: &Item,
    force: bool,
) -> Result<(), Failure> {
    let slug = &p.rules.slug;
    let st = standing(&tx.conn, slug).await?;
    let dependants = dependants_of(&tx.conn, slug, r.rid, &st).await?;
    let mut after = standing_before(&st, r);
    after
        .targets
        .insert(r.rid, Target::of("dropped", false, Some(s.rid)));
    let Some(h) = docket_core::stall::holder(s.rid, &after.targets) else {
        return Ok(());
    };
    let holder = tx.fresh(h).await?;
    let listed = release_list(&tx.conn, slug).await?;
    let mut holds = holds_of(&tx.conn, p, &st).await?;
    for v in holds.values_mut() {
        v.retain(|x| *x != r.rid);
    }
    for rid in dependants {
        let dependant = tx.fresh(rid).await?;
        refuse_later(&listed, &dependant, &holder, force)?;
        if let Some(path) = docket_core::stall::path(&holds, h, rid) {
            let mut around = vec![rid];
            around.extend(&path[..path.len() - 1]);
            return Err(Failure::Refused(format!(
                "{} cannot be superseded by {}: {} would be held by {}, which closes the cycle {}.",
                r.id,
                s.id,
                dependant.id,
                holder.id,
                st.cycle(&around)
            )));
        }
        holds.entry(rid).or_default().push(h);
    }
    Ok(())
}

/// The standing as it was before `was` became `target`: the same, with the target as it stood.
fn standing_before(st: &Standing, was: &Item) -> Standing {
    let mut targets = st.targets.clone();
    targets.insert(
        was.rid,
        Target::of(&was.state, was.decision.is_some(), was.superseded_by),
    );
    Standing {
        targets,
        ids: st.ids.clone(),
        conditions: st.conditions.clone(),
    }
}

/// Every item with a dependency on target is waited again from what still holds it, and resumes when
/// nothing does. `was` is target as it stood before, and `how` what happened to it: closed, decided or
/// dropped. A drop that names no successor leaves the dependency abandoned, so each such item goes to
/// the owner to ask whether it is still wanted.
pub async fn release_waiters(
    tx: &mut Tx,
    p: &ProjectRow,
    was: &Item,
    target: &Item,
    how: &str,
) -> Result<Vec<Item>, Failure> {
    let slug = p.rules.slug.clone();
    let st = standing(&tx.conn, &slug).await?;
    let before = standing_before(&st, was);
    let abandoned = how == "dropped" && target.superseded_by.is_none();
    let why = format!("{} {how}", target.id);
    let mut out = Vec::new();
    for rid in dependants_of(&tx.conn, &slug, target.rid, &st).await? {
        let w = tx.fresh(rid).await?;
        let held = before.wait(&dependencies_of(&tx.conn, rid).await?);
        match rewait(tx, &slug, &w, held.as_ref(), &why).await? {
            Rewait::Resumed(row) => out.push(*row),
            Rewait::Began => {
                let on = wait_of(&tx.conn, rid).await?.map(|n| n.id);
                let note = format!("on {} ({why})", on.unwrap_or_default());
                tx.event(&slug, Some(rid), "waited", Some(&note), None, None)
                    .await?;
            }
            Rewait::Same | Rewait::Switched => {}
        }
        if abandoned {
            let note = format!(
                "{}, which this waited on, was dropped. Still wanted?",
                target.id
            );
            tx.update(rid, &[]).await?;
            tx.event(&slug, Some(rid), "asked", Some(&note), None, None)
                .await?;
            let row = tx.fresh(rid).await?;
            if let Some(o) = out.iter_mut().find(|o| o.rid == rid) {
                *o = row;
            }
        }
    }
    Ok(out)
}

/// Every open, unclaimed item with a dependency on target waits on it again, once it is open again.
/// `was` is target as it stood before it reopened.
pub async fn hold_waiters(
    tx: &mut Tx,
    p: &ProjectRow,
    was: &Item,
    target: &Item,
) -> Result<(), Failure> {
    let slug = p.rules.slug.clone();
    let st = standing(&tx.conn, &slug).await?;
    let before = standing_before(&st, was);
    for rid in dependants_of(&tx.conn, &slug, target.rid, &st).await? {
        let w = tx.fresh(rid).await?;
        if w.claim().is_some() {
            continue;
        }
        let held = before.wait(&dependencies_of(&tx.conn, rid).await?);
        if let Rewait::Began = rewait(tx, &slug, &w, held.as_ref(), "reopened").await? {
            let note = format!("on {} (reopened)", target.id);
            tx.event(&slug, Some(rid), "waited", Some(&note), None, None)
                .await?;
        }
    }
    Ok(())
}

/// Every tie of the project: its items' parent edges, and each related and origin link with an end
/// in it.
pub async fn project_ties<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Vec<Tie>, DbErr> {
    let rows = c
        .query_all_raw(sql(
            "SELECT l.rid, l.kind, l.to_rid FROM links l \
             WHERE l.kind IN ('related', 'origin') AND l.to_rid IS NOT NULL \
               AND (l.rid IN (SELECT rid FROM items WHERE project=?) \
                 OR l.to_rid IN (SELECT rid FROM items WHERE project=?)) \
             UNION ALL SELECT rid, 'parent', parent_rid FROM items WHERE project=? AND parent_rid IS NOT NULL \
             ORDER BY 1, 2, 3",
            vec![slug.into(), slug.into(), slug.into()],
        ))
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let kind: String = r.try_get_by_index(1)?;
        if let Some(edge) = Edge::parse(&kind) {
            out.push(Tie {
                rid: r.try_get_by_index(0)?,
                edge,
                to: r.try_get_by_index(2)?,
            });
        }
    }
    Ok(out)
}

// ---- the files a ticket touches ----

/// The files a ticket will change: its Touches line when it has one, else the files it cites.
pub async fn work_paths<C: ConnectionTrait>(c: &C, r: &Item) -> Result<BTreeSet<String>, DbErr> {
    let declared = declared_files(&r.body);
    if !declared.is_empty() {
        return Ok(declared);
    }
    let cited: Vec<String> = column(
        c,
        "SELECT to_path FROM links WHERE rid=? AND kind='cites_file'",
        vec![r.rid.into()],
    )
    .await?;
    Ok(fold_paths(cited))
}

/// Every other claimed open item that touches a file r touches, with the paths shared.
pub async fn live_overlaps<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    r: &Item,
) -> Result<Vec<(Item, Vec<String>)>, DbErr> {
    let mine = work_paths(c, r).await?;
    if mine.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let others = crate::store::claimed(c, slug, None)
        .await?
        .into_iter()
        .filter(|o| o.rid != r.rid);
    for other in others {
        let theirs = work_paths(c, &other).await?;
        let shared: Vec<String> = mine
            .iter()
            .filter(|p| theirs.iter().any(|q| same_file(p, q)))
            .cloned()
            .collect();
        if !shared.is_empty() {
            out.push((other, shared));
        }
    }
    Ok(out)
}

// ---- the index, read for what is close to an item ----

const STOP: &str = "the a an and or of to in on for with is are was were be by at as it its this that \
                    from not no but if so than then when which who what into over under one two";

static TITLE_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z][A-Za-z0-9_']{3,}").unwrap());
static SYMBOL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([A-Za-z_][A-Za-z0-9_.:]*)`").unwrap());

async fn similar_terms<C: ConnectionTrait>(c: &C, r: &Item) -> Result<Vec<String>, DbErr> {
    let stop: HashSet<&str> = STOP.split_whitespace().collect();
    let mut terms: Vec<String> = Vec::new();
    for m in TITLE_WORD.find_iter(&r.title) {
        if !stop.contains(m.as_str().to_lowercase().as_str()) {
            terms.push(m.as_str().to_string());
        }
    }
    let cited: Vec<String> = column(
        c,
        "SELECT to_path FROM links WHERE rid=? AND kind IN ('cites_file', 'cites_test') \
         ORDER BY kind, to_path, to_line NULLS FIRST, id",
        vec![r.rid.into()],
    )
    .await?;
    for path in cited {
        terms.push(path.rsplit('/').next().unwrap_or("").to_string());
    }
    for m in SYMBOL.captures_iter(&r.body) {
        let sym = &m[1];
        if split_id(sym).is_ok() || sym.contains('/') {
            continue;
        }
        let last = sym.rsplit('.').next().unwrap_or(sym);
        terms.push(last.rsplit("::").next().unwrap_or(last).to_string());
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for t in terms {
        let k = t.to_lowercase();
        if k.len() > 2 && seen.insert(k) {
            out.push(t);
        }
    }
    out.truncate(30);
    Ok(out)
}

/// Rows the index ranks close to r, by title, files and symbols.
pub async fn similar_rows<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    r: &Item,
    n: i64,
    state: Option<&str>,
) -> Result<Vec<Item>, Failure> {
    let terms = similar_terms(c, r).await?;
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let q = docket_core::search::any_of(&terms);
    let narrow = crate::reads::search::Narrow {
        state: state.map(str::to_string),
        exclude: Some(r.rid),
        n,
        ..crate::reads::search::Narrow::default()
    };
    let (text, values) = crate::reads::search::ranked(slug, &q, &narrow, (String::new(), vec![]));
    Ok(items(c, &text, values).await?)
}

/// Questions already decided, open or done, closest to r: the prior decisions an agent derives from.
pub async fn decided_like<C: ConnectionTrait>(
    c: &C,
    p: &ProjectRow,
    r: &Item,
) -> Result<Vec<Item>, Failure> {
    if r.item_type != ItemType::Question || r.decision.is_some() {
        return Ok(Vec::new());
    }
    let rows = similar_rows(c, &p.rules.slug, r, 15, None).await?;
    Ok(rows
        .into_iter()
        .filter(|x| x.item_type == ItemType::Question && x.decision.is_some())
        .take(3)
        .collect())
}

/// The project's releases in the order they ship.
pub async fn release_list<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Listed, Failure> {
    Ok(crate::verbs::releases::listed(c, slug).await?)
}

fn release_name(listed: &Listed, release: Option<i64>) -> String {
    listed
        .name(release)
        .map_or_else(|| "the backlog".to_string(), str::to_string)
}

/// Refuse a hold of `held` by `holder` when the holder ships in a later release, naming both repairs.
pub fn refuse_later(
    listed: &Listed,
    held: &Item,
    holder: &Item,
    force: bool,
) -> Result<(), Failure> {
    let (held_in, holder_in) = (
        release_name(listed, held.release_id),
        release_name(listed, holder.release_id),
    );
    if force
        || !docket_core::stall::runs_later(
            &listed.open,
            listed.name(held.release_id),
            listed.name(holder.release_id),
        )
    {
        return Ok(());
    }
    Err(Failure::Refused(format!(
        "{} ({held_in}) would be held by {} ({holder_in}), which ships later. Move {} to {held_in}, or move {} to {holder_in}, or pass --force.",
        held.id, holder.id, holder.id, held.id
    )))
}

/// The open items' places in the releases and what holds each: a dependency holds its dependant, a
/// child holds its plan. The one owner of the rule that a held item ships no later than its holder.
pub struct Order {
    listed: Listed,
    needs: Vec<(i64, i64)>,
    rank: BTreeMap<i64, usize>,
    ids: BTreeMap<i64, String>,
}

impl Order {
    pub async fn load<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Self, Failure> {
        let listed = release_list(c, slug).await?;
        let mut rank = BTreeMap::new();
        let mut ids = BTreeMap::new();
        let mut needs = Vec::new();
        let rows = c
            .query_all_raw(sql(
                "SELECT rid, id, release_id, parent_rid FROM items WHERE project=? AND state='open'",
                vec![slug.into()],
            ))
            .await?;
        let mut parents = Vec::new();
        for r in rows {
            let rid: i64 = r.try_get_by_index(0)?;
            let release: Option<i64> = r.try_get_by_index(2)?;
            let place = docket_core::queue::release_rank(&listed.open, listed.name(release))
                .unwrap_or(usize::MAX);
            rank.insert(rid, place);
            ids.insert(rid, r.try_get_by_index(1)?);
            if let Some(plan) = r.try_get_by_index::<Option<i64>>(3)? {
                parents.push((plan, rid));
            }
        }
        needs.extend(
            parents
                .into_iter()
                .filter(|(plan, _)| rank.contains_key(plan)),
        );
        for (rid, on) in open_dependencies(c, slug).await? {
            if rank.contains_key(&on) {
                needs.push((rid, on));
            }
        }
        Ok(Order {
            listed,
            needs,
            rank,
            ids,
        })
    }

    /// The place a release id has among the releases not shipped; the backlog is the last.
    #[must_use]
    pub fn place(&self, release: Option<i64>) -> usize {
        docket_core::queue::release_rank(&self.listed.open, self.listed.name(release))
            .unwrap_or(usize::MAX)
    }

    fn release_at(&self, place: usize) -> Option<i64> {
        self.listed.open.get(place).and_then(|n| self.listed.id(n))
    }

    fn name_at(&self, place: usize) -> String {
        self.listed
            .open
            .get(place)
            .cloned()
            .unwrap_or_else(|| "the backlog".to_string())
    }

    fn id(&self, rid: i64) -> String {
        self.ids.get(&rid).cloned().unwrap_or_default()
    }

    fn at(&self, rid: i64, change: &BTreeMap<i64, usize>) -> usize {
        change
            .get(&rid)
            .or_else(|| self.rank.get(&rid))
            .copied()
            .unwrap_or(0)
    }

    /// Refuse a move that leaves items out of order that were in order, naming each.
    pub fn refuse(&self, change: &BTreeMap<i64, usize>) -> Result<(), Failure> {
        let out = docket_core::stall::order::introduced(&self.needs, &self.rank, change);
        self.refuse_pairs(&out, change)
    }

    /// Refuse the items out of order with `rid`, an item that has just become open and so had no
    /// place in the order before.
    pub fn refuse_around(&self, rid: i64) -> Result<(), Failure> {
        let none = BTreeMap::new();
        let out: Vec<(i64, i64)> =
            docket_core::stall::order::violations(&self.needs, &self.rank, &none)
                .into_iter()
                .filter(|(held, holder)| *held == rid || *holder == rid)
                .collect();
        self.refuse_pairs(&out, &none)
    }

    fn refuse_pairs(
        &self,
        out: &[(i64, i64)],
        change: &BTreeMap<i64, usize>,
    ) -> Result<(), Failure> {
        if out.is_empty() {
            return Ok(());
        }
        let lines: Vec<String> = out
            .iter()
            .map(|(held, holder)| {
                let (hr, or) = (
                    self.name_at(self.at(*held, change)),
                    self.name_at(self.at(*holder, change)),
                );
                let (h, o) = (self.id(*held), self.id(*holder));
                format!(
                    "{h} ({hr}) would be held by {o} ({or}), which ships later. Move {o} to {hr}, or move {h} to {or}, or pass --carry to move them together, or --force."
                )
            })
            .collect();
        Err(Failure::Refused(lines.join("\n")))
    }

    /// What moving `rid` to the release `to` takes with it, as `(rid, item id, release id)` for each
    /// item but `rid` itself.
    #[must_use]
    pub fn carried(&self, rid: i64, to: Option<i64>) -> Vec<(i64, String, Option<i64>)> {
        let moved = docket_core::stall::order::carry(&self.needs, &self.rank, rid, self.place(to));
        moved
            .into_iter()
            .filter(|(r, _)| *r != rid)
            .map(|(r, place)| (r, self.id(r), self.release_at(place)))
            .collect()
    }

    /// What moving every item of `rids` to the release `to` takes with it, as `(rid, item id,
    /// release id)` for each item outside `rids`.
    #[must_use]
    pub fn carried_all(&self, rids: &[i64], to: Option<i64>) -> Vec<(i64, String, Option<i64>)> {
        let mut rank = self.rank.clone();
        let mut moved = BTreeMap::new();
        for rid in rids {
            let step = docket_core::stall::order::carry(&self.needs, &rank, *rid, self.place(to));
            rank.extend(step.iter().map(|(r, p)| (*r, *p)));
            moved.extend(step);
        }
        moved
            .into_iter()
            .filter(|(r, _)| !rids.contains(r))
            .map(|(r, place)| (r, self.id(r), self.release_at(place)))
            .collect()
    }

    /// The change that moves every item of `rids` to the release `to`.
    #[must_use]
    pub fn moving_all(&self, rids: &[i64], to: Option<i64>) -> BTreeMap<i64, usize> {
        rids.iter().map(|r| (*r, self.place(to))).collect()
    }

    /// The place `rid` takes if moved to the release `to`, as a change.
    #[must_use]
    pub fn moving(&self, rid: i64, to: Option<i64>) -> BTreeMap<i64, usize> {
        BTreeMap::from([(rid, self.place(to))])
    }
}
