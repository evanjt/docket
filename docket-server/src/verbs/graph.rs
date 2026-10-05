//! What a verb reads around an item: its package, the audits over it, its neighbours.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use sea_orm::{ConnectionTrait, DbErr, FromQueryResult};

use docket_core::api::Progress;
use docket_core::item::{Field, Item};
use docket_core::member::{Edge, Tie, descendants};
use docket_core::release::Listed;
use docket_core::stall::Target;
use docket_core::text::split_id;
use docket_core::touch::{declared_files, fold_paths, same_file};
use docket_core::word::ItemType;

use crate::store::{ProjectRow, STATE_COLUMNS, Tx, column, items, sql};
use crate::verbs::Failure;

use crate::reads::public::Members;

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

/// How many items under rid, at any depth, are open and how many are closed.
pub async fn member_count<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Members, DbErr> {
    let stmt = sql(
        "WITH RECURSIVE under(rid) AS ( \
           SELECT rid FROM items WHERE parent_rid=? \
           UNION SELECT i.rid FROM items i JOIN under u ON i.parent_rid=u.rid) \
         SELECT COUNT(*) FILTER (WHERE state='open') AS open, \
                COUNT(*) FILTER (WHERE state<>'open') AS closed \
         FROM items WHERE rid IN (SELECT rid FROM under)",
        vec![rid.into()],
    );
    Ok(Counted::find_by_statement(stmt)
        .one(c)
        .await?
        .map_or_else(Members::default, |n| Members {
            open: n.open.unsigned_abs(),
            closed: n.closed.unsigned_abs(),
        }))
}

#[derive(FromQueryResult)]
struct Counted {
    open: i64,
    closed: i64,
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

    /// The place `rid` takes if moved to the release `to`, as a change.
    #[must_use]
    pub fn moving(&self, rid: i64, to: Option<i64>) -> BTreeMap<i64, usize> {
        BTreeMap::from([(rid, self.place(to))])
    }
}
