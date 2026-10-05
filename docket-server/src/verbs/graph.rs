//! What a verb reads around an item: its package, the audits over it, its concepts, its neighbours.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use sea_orm::{ConnectionTrait, DbErr, EntityTrait, Value};

use docket_core::api::Progress;
use docket_core::item::{Field, Item};
use docket_core::rules::{GATE, kind_of};
use docket_core::stall::Target;
use docket_core::text::split_id;
use docket_core::touch::{declared_files, fold_paths, same_file};
use docket_core::word::Kind;

use crate::store::{ProjectRow, STATE_COLUMNS, Tx, column, items, sql, state_by_rid};
use crate::verbs::Failure;

use crate::reads::public::marks;

/// The keys of the project whose kind is one of those given.
pub fn keys_of(p: &ProjectRow, kinds: &[Kind]) -> Vec<String> {
    p.rules
        .keys
        .iter()
        .filter(|s| kinds.contains(&s.kind))
        .map(|s| s.key.clone())
        .collect()
}

pub fn is_standing(p: &ProjectRow, key: &str) -> bool {
    kind_of(&p.rules, key).is_ok_and(Kind::is_standing)
}

/// The open packages that opened rid, by number.
pub async fn packages_of<C: ConnectionTrait>(
    c: &C,
    p: &ProjectRow,
    rid: i64,
) -> Result<Vec<Item>, DbErr> {
    let pk = keys_of(p, &[Kind::Package]);
    if pk.is_empty() {
        return Ok(Vec::new());
    }
    let mut values: Vec<Value> = vec![rid.into()];
    values.extend(pk.iter().map(|k| k.clone().into()));
    items(
        c,
        &format!(
            "SELECT p.* FROM links l JOIN items p ON p.rid=l.to_rid WHERE l.rid=? AND l.kind='opened' \
             AND p.state='open' AND p.key IN ({}) ORDER BY p.num",
            marks(pk.len())
        ),
        values,
    )
    .await
}

/// The open package that opened rid, or none.
pub async fn package_of<C: ConnectionTrait>(
    c: &C,
    p: &ProjectRow,
    rid: i64,
) -> Result<Option<Item>, DbErr> {
    Ok(packages_of(c, p, rid).await?.into_iter().next())
}

/// What a package holds: the items it opened, the direct ones only.
pub async fn package_members<C: ConnectionTrait>(
    c: &C,
    prid: i64,
    open_only: bool,
) -> Result<Vec<Item>, DbErr> {
    let open = if open_only { "AND i.state='open' " } else { "" };
    items(
        c,
        &format!(
            "SELECT i.* FROM links l JOIN items i ON i.rid=l.rid WHERE l.to_rid=? AND l.kind='opened' \
             {open}ORDER BY i.key, i.num"
        ),
        vec![prid.into()],
    )
    .await
}

pub async fn open_member_count<C: ConnectionTrait>(c: &C, prid: i64) -> Result<u64, DbErr> {
    Ok(package_members(c, prid, true).await?.len() as u64)
}

/// A package's members closed or dropped, all of them, and those claimed now.
pub async fn package_progress<C: ConnectionTrait>(c: &C, prid: i64) -> Result<Progress, DbErr> {
    let ms = package_members(c, prid, false).await?;
    Ok(Progress {
        done: ms.iter().filter(|m| m.state != "open").count(),
        total: ms.len(),
        live: ms.iter().filter(|m| m.claim_branch.is_some()).count(),
    })
}

/// The open items under rid, by rid.
pub async fn open_under<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Vec<Item>, DbErr> {
    items(
        c,
        &format!(
            "WITH RECURSIVE below(rid) AS (\
                 SELECT rid FROM links WHERE kind='opened' AND to_rid=? \
                 UNION \
                 SELECT l.rid FROM links l JOIN below b ON l.to_rid=b.rid WHERE l.kind='opened') \
             SELECT {STATE_COLUMNS} FROM items \
             WHERE rid IN (SELECT rid FROM below) AND state='open' AND rid<>? ORDER BY rid"
        ),
        vec![rid.into(), rid.into()],
    )
    .await
}

/// The open plans among rids, and every one that opened one of them.
pub async fn audits_over<C: ConnectionTrait>(
    c: &C,
    p: &ProjectRow,
    rids: &[i64],
) -> Result<Vec<Item>, DbErr> {
    let audit_keys = keys_of(p, &[Kind::Audit]);
    let mut seen = HashSet::new();
    let mut todo: Vec<i64> = rids.to_vec();
    let mut out = Vec::new();
    while let Some(rid) = todo.pop() {
        if !seen.insert(rid) {
            continue;
        }
        if let Some(r) = state_by_rid(c, rid).await?
            && audit_keys.contains(&r.key)
            && r.state == "open"
        {
            out.push(r);
        }
        let parents: Vec<i64> = column(
            c,
            "SELECT to_rid FROM links WHERE kind='opened' AND rid=? AND to_rid IS NOT NULL ORDER BY to_rid",
            vec![rid.into()],
        )
        .await?;
        todo.extend(parents);
    }
    Ok(out)
}

/// How each item of a project stands as something depended on, and its id.
pub struct Standing {
    pub targets: BTreeMap<i64, Target>,
    pub ids: BTreeMap<i64, String>,
}

impl Standing {
    pub fn id(&self, rid: i64) -> String {
        self.ids.get(&rid).cloned().unwrap_or_default()
    }

    /// The ids of a path, the first item repeated at its end.
    pub fn cycle(&self, path: &[i64]) -> String {
        let mut ids: Vec<String> = path.iter().map(|r| self.id(*r)).collect();
        ids.extend(path.first().map(|r| self.id(*r)));
        ids.join(" -> ")
    }
}

pub async fn standing<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Standing, DbErr> {
    let rows = c
        .query_all_raw(sql(
            "SELECT rid, id, state, decision IS NOT NULL, superseded_by FROM items WHERE project=?",
            vec![slug.into()],
        ))
        .await?;
    let mut out = Standing {
        targets: BTreeMap::new(),
        ids: BTreeMap::new(),
    };
    for r in rows {
        let rid: i64 = r.try_get_by_index(0)?;
        let state: String = r.try_get_by_index(2)?;
        let target = Target::of(&state, r.try_get_by_index(3)?, r.try_get_by_index(4)?);
        out.targets.insert(rid, target);
        out.ids.insert(rid, r.try_get_by_index(1)?);
    }
    Ok(out)
}

/// What rid depends on, oldest first.
pub async fn depends_on<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Vec<i64>, DbErr> {
    column(
        c,
        "SELECT on_rid FROM dependencies WHERE rid=? ORDER BY created_at, on_rid",
        vec![rid.into()],
    )
    .await
}

/// Every open item's dependencies, as `(rid, on_rid)`, by item.
pub async fn open_dependencies<C: ConnectionTrait>(
    c: &C,
    slug: &str,
) -> Result<Vec<(i64, i64)>, DbErr> {
    let rows = c
        .query_all_raw(sql(
            "SELECT d.rid, d.on_rid FROM dependencies d JOIN items i ON i.rid=d.rid \
             WHERE i.project=? AND i.state='open' ORDER BY d.rid, d.created_at, d.on_rid",
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
/// and, for an open plan, each open item it opened.
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
    let plans = keys_of(p, &[Kind::Audit]);
    if !plans.is_empty() {
        let mut values: Vec<Value> = vec![p.rules.slug.clone().into()];
        values.extend(plans.iter().map(|k| k.clone().into()));
        let rows = c
            .query_all_raw(sql(
                &format!(
                    "SELECT l.to_rid, l.rid FROM links l JOIN items a ON a.rid=l.to_rid \
                     JOIN items m ON m.rid=l.rid WHERE a.project=? AND l.kind='opened' \
                     AND a.state='open' AND m.state='open' AND a.key IN ({}) ORDER BY l.to_rid, l.rid",
                    marks(plans.len())
                ),
                values,
            ))
            .await?;
        for r in rows {
            let plan: i64 = r.try_get_by_index(0)?;
            out.entry(plan).or_default().push(r.try_get_by_index(1)?);
        }
    }
    Ok(out)
}

/// The refusal for any cycle of holds in the project, naming one.
pub async fn held_wait<C: ConnectionTrait>(
    c: &C,
    p: &ProjectRow,
) -> Result<Option<Failure>, DbErr> {
    let st = standing(c, &p.rules.slug).await?;
    let holds = holds_of(c, p, &st).await?;
    let Some(&first) = docket_core::stall::cycles(&holds).iter().next() else {
        return Ok(None);
    };
    let around = holds[&first]
        .iter()
        .find_map(|n| docket_core::stall::path(&holds, *n, first))
        .map_or_else(
            || vec![first],
            |mut path| {
                path.pop();
                path.insert(0, first);
                path
            },
        );
    Ok(Some(Failure::Refused(format!(
        "{} is held in a cycle: {}, so {} would wait forever. Remove a dependency on it, or unlink it, first.",
        st.id(first),
        st.cycle(&around),
        st.id(first)
    ))))
}

/// How a rewait moved an item.
pub enum Rewait {
    Same,
    /// It waits now, and did not before.
    Began,
    /// It waits on another item than before.
    Switched,
    Resumed(Box<Item>),
}

/// An item's wait set from what still holds it: the item it waits on while that still holds it, else
/// the first of its dependencies not yet satisfied, or none. A wait on a condition, a plan's gate
/// among them, is left as it is. An item that begins to wait gives up its claim. One that resumes
/// goes to the agents, unless the owner was asked something during the wait, which still stands.
pub async fn rewait(
    tx: &mut Tx,
    slug: &str,
    r: &Item,
    st: &Standing,
    why: &str,
) -> Result<Rewait, Failure> {
    if r.state != "open" || r.wait_on.as_deref() == Some("condition") {
        return Ok(Rewait::Same);
    }
    let on = depends_on(&tx.conn, r.rid).await?;
    let holders = docket_core::stall::holders(&on, &st.targets);
    match holders.first() {
        Some(_) if r.wait_on.is_some() && r.wait_item.is_some_and(|w| holders.contains(&w)) => {
            Ok(Rewait::Same)
        }
        Some(&h) => {
            let mut cols = vec![
                Field::WaitOn(Some("item".into())),
                Field::WaitItem(Some(h)),
                Field::WaitRef(Some(st.id(h))),
                Field::WaitSince(Some(r.wait_since.clone().unwrap_or_else(|| tx.now.clone()))),
            ];
            if r.wait_on.is_some() {
                tx.update(r.rid, &cols).await?;
                let note = if why.is_empty() {
                    format!("on {}", st.id(h))
                } else {
                    format!("on {} ({why})", st.id(h))
                };
                tx.event(slug, Some(r.rid), "waited", Some(&note), None, None)
                    .await?;
                return Ok(Rewait::Switched);
            }
            cols.extend(docket_core::rules::unclaimed());
            tx.update(r.rid, &cols).await?;
            Ok(Rewait::Began)
        }
        None if r.wait_on.is_some() => {
            let mut cols = vec![
                Field::WaitOn(None),
                Field::WaitItem(None),
                Field::WaitRef(None),
                Field::WaitSince(None),
            ];
            let asked_since = r.turn.as_deref() == Some("user")
                && r.asked_at.as_deref() > r.wait_since.as_deref();
            if !asked_since {
                cols.push(Field::Turn(Some("agent".into())));
            }
            let row = tx.update(r.rid, &cols).await?;
            tx.event(slug, Some(r.rid), "resumed", Some(why), None, None)
                .await?;
            Ok(Rewait::Resumed(Box::new(row)))
        }
        None => Ok(Rewait::Same),
    }
}

/// The open items with a dependency that reaches `target`, itself or through a successor, by rid.
async fn dependants_of<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    target: i64,
    st: &Standing,
) -> Result<Vec<i64>, DbErr> {
    let mut out: BTreeSet<i64> = open_dependencies(c, slug)
        .await?
        .into_iter()
        .filter(|(_, on)| docket_core::stall::passes(*on, target, &st.targets))
        .map(|(rid, _)| rid)
        .collect();
    let mirrored: Vec<i64> = column(
        c,
        "SELECT rid FROM items WHERE wait_item=? AND state='open'",
        vec![target.into()],
    )
    .await?;
    out.extend(mirrored);
    Ok(out.into_iter().collect())
}

/// Hold each plan over rids while anything it opened is open; release it to its audit when nothing is.
pub async fn settle_audits(
    tx: &mut Tx,
    p: &ProjectRow,
    rids: &[i64],
) -> Result<Vec<Item>, Failure> {
    let mut released = Vec::new();
    for a in audits_over(&tx.conn, p, rids).await? {
        if a.claim_branch.is_some() {
            continue;
        }
        let pending = open_under(&tx.conn, a.rid).await?;
        let gated =
            a.wait_on.as_deref() == Some("condition") && a.wait_ref.as_deref() == Some(GATE);
        if !pending.is_empty() && a.wait_on.is_none() {
            tx.update(
                a.rid,
                &[
                    Field::WaitOn(Some("condition".into())),
                    Field::WaitRef(Some(GATE.into())),
                    Field::WaitItem(None),
                    Field::WaitSince(Some(tx.now.clone())),
                ],
            )
            .await?;
            let note = format!("until: {GATE} ({} open)", pending.len());
            tx.event(
                &p.rules.slug,
                Some(a.rid),
                "waited",
                Some(&note),
                None,
                None,
            )
            .await?;
        } else if pending.is_empty() && gated {
            let cols = [
                Field::WaitOn(None),
                Field::WaitItem(None),
                Field::WaitRef(None),
                Field::WaitSince(None),
                Field::Turn(Some("agent".into())),
            ];
            let row = tx.update(a.rid, &cols).await?;
            tx.event(
                &p.rules.slug,
                Some(a.rid),
                "resumed",
                Some(GATE),
                None,
                None,
            )
            .await?;
            let st = standing(&tx.conn, &p.rules.slug).await?;
            if let Rewait::Began = rewait(tx, &p.rules.slug, &row, &st, GATE).await? {
                let note = format!("on {}", tx.fresh(a.rid).await?.wait_ref.unwrap_or_default());
                tx.event(
                    &p.rules.slug,
                    Some(a.rid),
                    "waited",
                    Some(&note),
                    None,
                    None,
                )
                .await?;
            } else {
                released.push(tx.fresh(a.rid).await?);
            }
        }
    }
    Ok(released)
}

/// Whether a plan's audit came due: its last gate event is the resume.
pub async fn came_due<C: ConnectionTrait>(c: &C, rid: i64) -> Result<bool, DbErr> {
    let rows = c
        .query_all_raw(sql(
            "SELECT kind, note FROM events WHERE rid=? AND kind IN ('waited', 'resumed') ORDER BY seq",
            vec![rid.into()],
        ))
        .await?;
    let mut events = Vec::with_capacity(rows.len());
    for r in rows {
        let kind: String = r.try_get_by_index(0)?;
        let note: Option<String> = r.try_get_by_index(1)?;
        events.push((kind, note.unwrap_or_default()));
    }
    Ok(docket_core::rules::came_due(
        events.iter().map(|(k, n)| (k.as_str(), n.as_str())),
    ))
}

/// Every item with a dependency on target is waited again from what still holds it, and resumes when
/// nothing does. `how` is what happened to target: closed, decided or dropped. A drop that names no
/// successor leaves the dependency abandoned, so each such item goes to the owner to ask whether it
/// is still wanted.
pub async fn release_waiters(
    tx: &mut Tx,
    p: &ProjectRow,
    target: &Item,
    how: &str,
) -> Result<Vec<Item>, Failure> {
    let slug = p.rules.slug.clone();
    let st = standing(&tx.conn, &slug).await?;
    let abandoned = how == "dropped" && target.superseded_by.is_none();
    let why = format!("{} {how}", target.id);
    let mut out = Vec::new();
    for rid in dependants_of(&tx.conn, &slug, target.rid, &st).await? {
        let w = tx.fresh(rid).await?;
        if let Rewait::Resumed(row) = rewait(tx, &slug, &w, &st, &why).await? {
            out.push(*row);
        }
        if abandoned {
            let note = format!(
                "{}, which this waited on, was dropped. Still wanted?",
                target.id
            );
            let mut cols = vec![
                Field::Turn(Some("user".into())),
                Field::TurnNote(Some(note.clone())),
                Field::AskedAt(Some(tx.now.clone())),
            ];
            cols.extend(docket_core::rules::unclaimed());
            let row = tx.update(rid, &cols).await?;
            tx.event(&slug, Some(rid), "asked", Some(&note), None, None)
                .await?;
            if let Some(o) = out.iter_mut().find(|o| o.rid == rid) {
                *o = row;
            }
        }
    }
    let rids: Vec<i64> = out.iter().map(|r| r.rid).collect();
    settle_audits(tx, p, &rids).await?;
    let mut free = Vec::with_capacity(rids.len());
    for rid in rids {
        let row = tx.fresh(rid).await?;
        if row.wait_on.is_none() {
            free.push(row);
        }
    }
    Ok(free)
}

/// Every open, unclaimed item with a dependency on target waits on it again, once it is open again.
pub async fn hold_waiters(tx: &mut Tx, p: &ProjectRow, target: &Item) -> Result<(), Failure> {
    let slug = p.rules.slug.clone();
    let st = standing(&tx.conn, &slug).await?;
    for rid in dependants_of(&tx.conn, &slug, target.rid, &st).await? {
        let w = tx.fresh(rid).await?;
        if w.claim_branch.is_some() {
            continue;
        }
        if let Rewait::Began = rewait(tx, &slug, &w, &st, "reopened").await? {
            let note = format!("on {} (reopened)", target.id);
            tx.event(&slug, Some(rid), "waited", Some(&note), None, None)
                .await?;
        }
    }
    Ok(())
}

/// The concepts an item belongs to: linked to it either way, or to anything that opened it.
pub async fn concepts_of<C: ConnectionTrait>(
    c: &C,
    p: &ProjectRow,
    rid: i64,
) -> Result<Vec<String>, DbErr> {
    let con = keys_of(p, &[Kind::Concept]);
    if con.is_empty() {
        return Ok(Vec::new());
    }
    let mut seen = HashSet::new();
    let mut todo = vec![rid];
    let mut out: Vec<(String, i64, String)> = Vec::new();
    while let Some(x) = todo.pop() {
        if !seen.insert(x) {
            continue;
        }
        let rows = c
            .query_all_raw(sql(
                "SELECT rid, to_rid FROM links WHERE (rid=? OR to_rid=?) AND kind IN ('related', 'opened')",
                vec![x.into(), x.into()],
            ))
            .await?;
        for l in rows {
            let from: i64 = l.try_get_by_index(0)?;
            let to: Option<i64> = l.try_get_by_index(1)?;
            let other = if from == x { to } else { Some(from) };
            let Some(other) = other else { continue };
            if let Some(o) = state_by_rid(c, other).await?
                && con.contains(&o.key)
                && o.rid != rid
                && !out.iter().any(|(id, _, _)| *id == o.id)
            {
                out.push((o.id.clone(), o.num, o.key.clone()));
            }
        }
        let parents: Vec<i64> = column(
            c,
            "SELECT to_rid FROM links WHERE kind='opened' AND rid=? AND to_rid IS NOT NULL",
            vec![x.into()],
        )
        .await?;
        todo.extend(parents);
    }
    out.sort_by(|a, b| (&a.2, a.1).cmp(&(&b.2, b.1)));
    Ok(out.into_iter().map(|(id, _, _)| id).collect())
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
    let others = items(
        c,
        "SELECT * FROM items WHERE project=? AND state='open' AND claim_branch IS NOT NULL \
         AND rid<>? ORDER BY claim_since",
        vec![slug.into(), r.rid.into()],
    )
    .await?;
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
    let qkeys = keys_of(p, &[Kind::Decision]);
    if !qkeys.contains(&r.key) || r.decision.is_some() {
        return Ok(Vec::new());
    }
    let rows = similar_rows(c, &p.rules.slug, r, 15, None).await?;
    Ok(rows
        .into_iter()
        .filter(|x| qkeys.contains(&x.key) && x.decision.is_some())
        .take(3)
        .collect())
}

/// The project's releases in the order they ship.
pub async fn release_list<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Vec<String>, Failure> {
    let skills = crate::entities::project::Entity::find_by_id(slug)
        .one(c)
        .await?
        .map(|p| p.skills)
        .unwrap_or_default();
    Ok(docket_core::fact::releases(skills["releases"].as_str()))
}

fn release_name(releases: &[String], theme: Option<&str>) -> String {
    let at = docket_core::queue::release_rank(releases, theme);
    releases.get(at).cloned().unwrap_or_default()
}

/// Refuse a hold of `held` by `holder` when the holder ships in a later release, naming both repairs.
pub fn refuse_later(
    releases: &[String],
    held: &Item,
    holder: &Item,
    force: bool,
) -> Result<(), Failure> {
    let (held_in, holder_in) = (
        release_name(releases, held.theme.as_deref()),
        release_name(releases, holder.theme.as_deref()),
    );
    if force
        || !docket_core::stall::runs_later(releases, held.theme.as_deref(), holder.theme.as_deref())
    {
        return Ok(());
    }
    Err(Failure::Refused(format!(
        "{} ({held_in}) would be held by {} ({holder_in}), which ships later. Move {} to {held_in}, or move {} to {holder_in}, or pass --force.",
        held.id, holder.id, holder.id, held.id
    )))
}

/// Refuse giving `r` a new theme when that puts a hold it takes part in into a later release.
pub async fn refuse_later_after_theme(
    tx: &Tx,
    slug: &str,
    r: &Item,
    theme: Option<&str>,
) -> Result<(), Failure> {
    let releases = release_list(&tx.conn, slug).await?;
    if releases.len() < 2 {
        return Ok(());
    }
    let rows = tx
        .conn
        .query_all_raw(sql(
            "SELECT d.rid, d.on_rid FROM dependencies d JOIN items i ON i.rid=d.rid \
               JOIN items o ON o.rid=d.on_rid WHERE i.project=? AND i.state='open' \
               AND o.state='open' AND (d.rid=? OR d.on_rid=?) \
             UNION ALL SELECT c.rid, l.rid FROM items c JOIN links l ON l.to_rid=c.rid AND l.kind='opened' \
               JOIN items m ON m.rid=l.rid AND m.state='open' \
               WHERE c.project=? AND c.state='open' AND c.wait_on='condition' AND c.wait_ref=? \
                 AND (c.rid=? OR l.rid=?) ORDER BY 1, 2",
            vec![
                slug.into(),
                r.rid.into(),
                r.rid.into(),
                slug.into(),
                GATE.into(),
                r.rid.into(),
                r.rid.into(),
            ],
        ))
        .await?;
    let after = |mut it: Item| {
        if it.rid == r.rid {
            it.theme = theme.map(str::to_string);
        }
        it
    };
    for row in &rows {
        let (Some(held), Some(holder)) = (
            state_by_rid(&tx.conn, row.try_get_by_index::<i64>(0)?).await?,
            state_by_rid(&tx.conn, row.try_get_by_index::<i64>(1)?).await?,
        ) else {
            continue;
        };
        if refuse_later(&releases, &held, &holder, false).is_ok() {
            refuse_later(&releases, &after(held), &after(holder), false)?;
        }
    }
    Ok(())
}
