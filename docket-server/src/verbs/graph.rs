//! What a verb reads around an item: its package, the audits over it, its concepts, its neighbours.

use std::collections::{BTreeSet, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use sea_orm::{ConnectionTrait, DbErr, Value};

use docket_core::api::Progress;
use docket_core::item::{Field, Item};
use docket_core::rules::{GATE, kind_of};
use docket_core::text::split_id;
use docket_core::word::Kind;

use crate::store::{ProjectRow, Tx, by_rid, column, items, sql};
use crate::verbs::Failure;

fn marks(n: usize) -> String {
    if n == 0 {
        "''".to_string()
    } else {
        vec!["?"; n].join(",")
    }
}

/// The keys of the project whose kind is one of those given.
pub fn keys_of(p: &ProjectRow, kinds: &[Kind]) -> Vec<String> {
    p.rules
        .keys
        .iter()
        .filter(|s| kinds.contains(&s.kind))
        .map(|s| s.key.clone())
        .collect()
}

pub fn is_package(p: &ProjectRow, key: &str) -> bool {
    kind_of(&p.rules, key) == Ok(Kind::Package)
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

/// Every item rid opened, and everything those opened, to any depth.
pub async fn opened_under<C: ConnectionTrait>(c: &C, rid: i64) -> Result<BTreeSet<i64>, DbErr> {
    let mut seen = BTreeSet::new();
    let mut todo = vec![rid];
    while let Some(cur) = todo.pop() {
        let children: Vec<i64> = column(
            c,
            "SELECT rid FROM links WHERE kind='opened' AND to_rid=?",
            vec![cur.into()],
        )
        .await?;
        for child in children {
            if child != rid && seen.insert(child) {
                todo.push(child);
            }
        }
    }
    Ok(seen)
}

/// The open items under rid, by rid.
pub async fn open_under<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Vec<Item>, DbErr> {
    let mut out = Vec::new();
    for x in opened_under(c, rid).await? {
        if let Some(r) = by_rid(c, x).await?
            && r.state == "open"
        {
            out.push(r);
        }
    }
    Ok(out)
}

/// The open gated items (audits, stories) among rids, and every one that opened one of them.
pub async fn audits_over<C: ConnectionTrait>(
    c: &C,
    p: &ProjectRow,
    rids: &[i64],
) -> Result<Vec<Item>, DbErr> {
    let audit_keys = keys_of(p, &[Kind::Audit, Kind::Story]);
    let mut seen = HashSet::new();
    let mut todo: Vec<i64> = rids.to_vec();
    let mut out = Vec::new();
    while let Some(rid) = todo.pop() {
        if !seen.insert(rid) {
            continue;
        }
        if let Some(r) = by_rid(c, rid).await?
            && audit_keys.contains(&r.key)
            && r.state == "open"
        {
            out.push(r);
        }
        let parents: Vec<i64> = column(
            c,
            "SELECT to_rid FROM links WHERE kind='opened' AND rid=? AND to_rid IS NOT NULL",
            vec![rid.into()],
        )
        .await?;
        todo.extend(parents);
    }
    Ok(out)
}

/// Hold each audit or story over rids while anything it opened is open; release it when nothing is.
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
            let turn = kind_of(&p.rules, &a.key)
                .ok()
                .and_then(Kind::gated_turn)
                .unwrap_or("agent");
            let mut cols = vec![
                Field::WaitOn(None),
                Field::WaitItem(None),
                Field::WaitRef(None),
                Field::WaitSince(None),
                Field::Turn(Some(turn.into())),
            ];
            if turn == "user" {
                cols.push(Field::AskedAt(Some(tx.now.clone())));
                cols.push(Field::TurnNote(Some(
                    "everything it opened is closed: accept it, or reopen what fails".into(),
                )));
            }
            tx.update(a.rid, &cols).await?;
            tx.event(
                &p.rules.slug,
                Some(a.rid),
                "resumed",
                Some("everything it opened is closed"),
                None,
                None,
            )
            .await?;
            released.push(tx.fresh(a.rid).await?);
        }
    }
    Ok(released)
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
            "SELECT * FROM items WHERE wait_item=? AND state=?",
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
            if let Some(o) = by_rid(c, other).await?
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

/// Whether an open work item, question or investigation belongs to no concept.
pub async fn belongs_to_none<C: ConnectionTrait>(
    c: &C,
    p: &ProjectRow,
    r: &Item,
) -> Result<bool, DbErr> {
    if r.state != "open" || keys_of(p, &[Kind::Concept]).is_empty() {
        return Ok(false);
    }
    if !matches!(
        kind_of(&p.rules, &r.key),
        Ok(Kind::Work | Kind::Decision | Kind::Research)
    ) {
        return Ok(false);
    }
    Ok(concepts_of(c, p, r.rid).await?.is_empty())
}

// ---- the files a ticket touches ----

static DECLARED_FILES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\*\*Touches\.\*\*\s*(.*)$").unwrap());
static DECLARED_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r":\d+(?:-\d+)?$").unwrap());
static FILE_EXTENSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.[A-Za-z][A-Za-z0-9_+-]*$").unwrap());

/// One entry per file: a citation by bare name or short path folds into the longer path it ends.
pub fn fold_paths(paths: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    let paths: BTreeSet<String> = paths.into_iter().collect();
    paths
        .iter()
        .filter(|p| {
            !paths
                .iter()
                .any(|q| q != *p && q.ends_with(&format!("/{p}")))
        })
        .cloned()
        .collect()
}

/// Whether two citations name one file. A shorter one counts only when it carries a directory.
pub fn same_file(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short.contains('/') && long.ends_with(&format!("/{short}"))
}

/// The paths a body's Touches line declares, folded and stripped of line numbers.
pub fn declared_files(r: &Item) -> BTreeSet<String> {
    let mut found = Vec::new();
    for line in r.body.lines() {
        let Some(m) = DECLARED_FILES.captures(line) else {
            continue;
        };
        for part in m[1].split(',') {
            let path = part.trim().trim_start_matches('`');
            let path = path.trim_end_matches(['`', '.', ';', ',']);
            let path = DECLARED_LINE.replace(path, "").to_string();
            let lower = path.to_lowercase();
            if lower == "none" || lower == "n/a" || path.chars().any(char::is_whitespace) {
                continue;
            }
            if path.contains('/') || FILE_EXTENSION.is_match(&path) {
                found.push(path);
            }
        }
    }
    fold_paths(found)
}

/// The files a ticket will change: its Touches line when it has one, else the files it cites.
pub async fn work_paths<C: ConnectionTrait>(c: &C, r: &Item) -> Result<BTreeSet<String>, DbErr> {
    let declared = declared_files(r);
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
        "SELECT to_path FROM links WHERE rid=? AND kind IN ('cites_file', 'cites_test')",
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
    let q = terms
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ");
    let mut text = "SELECT i.* FROM items_fts JOIN items i ON i.rid = items_fts.rowid \
                    WHERE items_fts MATCH ? AND i.project=?"
        .to_string();
    let mut values: Vec<Value> = vec![q.clone().into(), slug.into()];
    if let Some(state) = state {
        text.push_str(" AND i.state=?");
        values.push(state.into());
    }
    text.push_str(" AND i.rid<>? ORDER BY bm25(items_fts, 4.0, 8.0, 1.0, 2.0) LIMIT ?");
    values.push(r.rid.into());
    values.push(n.into());
    items(c, &text, values).await.map_err(|e| {
        Failure::Refused(format!(
            "search could not parse {}: {e}",
            docket_core::text::py_repr(&q)
        ))
    })
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
