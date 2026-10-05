//! What a verb reads around an item: its package, the audits over it, its concepts, its neighbours.

use std::collections::{BTreeSet, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use sea_orm::{ConnectionTrait, DbErr, EntityTrait, Value};

use docket_core::api::Progress;
use docket_core::item::{Field, Item};
use docket_core::rules::{GATE, kind_of};
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

/// The refusal for a waiter held by the plan it waits on: that plan cannot close while the waiter is
/// open, so the wait would never end. None when target is not an open plan over the waiter.
pub async fn wait_cycle<C: ConnectionTrait>(
    c: &C,
    p: &ProjectRow,
    waiter: &Item,
    target: &Item,
) -> Result<Option<Failure>, DbErr> {
    if target.rid == waiter.rid || target.state != "open" {
        return Ok(None);
    }
    let held = audits_over(c, p, &[waiter.rid])
        .await?
        .iter()
        .any(|a| a.rid == target.rid);
    Ok(held.then(|| {
        Failure::Refused(format!(
            "{t} holds {w} and cannot close while it is open, so {w} would wait forever. Wait on what blocks it, or unlink it from {t} first; resume the wait of {w} on {t} before linking it under {t}.",
            t = target.id,
            w = waiter.id
        ))
    }))
}

/// The first open item in the project that waits on a plan holding it, as its refusal.
pub async fn held_wait<C: ConnectionTrait>(
    c: &C,
    p: &ProjectRow,
) -> Result<Option<Failure>, DbErr> {
    let waiters = items(
        c,
        &format!(
            "SELECT {STATE_COLUMNS} FROM items WHERE project=? AND state='open' AND wait_on='item' AND wait_item IS NOT NULL ORDER BY key, num"
        ),
        vec![p.rules.slug.clone().into()],
    )
    .await?;
    for w in waiters {
        let Some(target) = state_by_rid(c, w.wait_item.unwrap_or_default()).await? else {
            continue;
        };
        if let Some(refused) = wait_cycle(c, p, &w, &target).await? {
            return Ok(Some(refused));
        }
    }
    Ok(None)
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
            tx.update(a.rid, &cols).await?;
            tx.event(
                &p.rules.slug,
                Some(a.rid),
                "resumed",
                Some(GATE),
                None,
                None,
            )
            .await?;
            released.push(tx.fresh(a.rid).await?);
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

/// Every item waiting on target goes back to the agent's queue, and says why.
pub async fn release_waiters(
    tx: &mut Tx,
    slug: &str,
    target: &Item,
    note: &str,
) -> Result<Vec<Item>, Failure> {
    let mut out = Vec::new();
    let waiting = tx
        .items(
            "SELECT * FROM items WHERE wait_item=? AND state=? ORDER BY rid",
            vec![target.rid.into(), "open".into()],
        )
        .await?;
    for w in waiting {
        let r = tx
            .update(
                w.rid,
                &[
                    Field::WaitOn(None),
                    Field::WaitItem(None),
                    Field::WaitRef(None),
                    Field::WaitSince(None),
                    Field::Turn(Some("agent".into())),
                ],
            )
            .await?;
        let text = format!("{} {note}", target.id);
        tx.event(slug, Some(w.rid), "resumed", Some(&text), None, None)
            .await?;
        out.push(r);
    }
    Ok(out)
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
    let (text, values) = crate::reads::search::ranked(slug, &q, &narrow);
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
            "SELECT rid, wait_item FROM items WHERE project=? AND state='open' \
               AND wait_on='item' AND wait_item IS NOT NULL AND (rid=? OR wait_item=?) \
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
