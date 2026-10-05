use std::collections::HashSet;

use crate::member::{Tie, descendants};
use crate::word::Kind;

/// The stored facts the ready queue filters and orders by; `tier` is the item's own, 0 for critical.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Candidate<'a> {
    pub rid: i64,
    pub key: &'a str,
    pub kind: Kind,
    pub open: bool,
    pub turn: Option<&'a str>,
    pub claimed: bool,
    pub waiting: bool,
    pub conflict: bool,
    pub complexity: Option<&'a str>,
    pub theme: Option<&'a str>,
    /// The release by name; none is the backlog.
    pub release: Option<&'a str>,
    pub tier: usize,
    pub opened_at: &'a str,
}

/// The three roles a session takes from the queue with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// A goal into tickets: a plan with nothing under it yet, an investigation, a decided question.
    Plan,
    /// The tickets.
    Work,
    /// A plan whose tickets are all closed.
    Audit,
}

impl Role {
    pub const NAMES: [&str; 3] = ["plan", "work", "audit"];

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "plan" => Some(Role::Plan),
            "work" => Some(Role::Work),
            "audit" => Some(Role::Audit),
            _ => None,
        }
    }
}

/// What `docket next` narrows the queue by.
#[derive(Clone, Debug, Default)]
pub struct Filter<'a> {
    /// The roles to take, in the order they rank within a release; empty takes any, unranked.
    pub roles: &'a [Role],
    pub priority: Option<usize>,
    pub key: Option<&'a str>,
    pub under: Option<&'a HashSet<i64>>,
    pub complexity: Option<&'a str>,
    pub theme: Option<&'a str>,
    /// The releases not yet shipped, in the order they ship.
    pub releases: &'a [String],
    /// Keep only items of the current release, the first of `releases`.
    pub current_release_only: bool,
}

/// Where an item's release falls among the releases not yet shipped: its position, and the backlog
/// after them all. `None` for a release they do not list, which no queue offers.
#[must_use]
pub fn release_rank(releases: &[String], release: Option<&str>) -> Option<usize> {
    match release {
        None => Some(releases.len()),
        Some(name) => releases.iter().position(|r| r == name),
    }
}

/// Whether an item is in the current release, the first not shipped.
#[must_use]
pub fn in_current(releases: &[String], release: Option<&str>) -> bool {
    release.is_some() && release_rank(releases, release) == Some(0)
}

/// The role an open item is taken by. A plan with children is due for its audit once everything
/// under it, at any depth, is closed; before that it belongs to no role.
fn role_of(c: &Candidate, ties: &[Tie], open: &HashSet<i64>) -> Option<Role> {
    match c.kind {
        Kind::Work => Some(Role::Work),
        Kind::Audit => {
            let under = descendants(ties, c.rid);
            if under.is_empty() {
                Some(Role::Plan)
            } else if under.is_disjoint(open) {
                Some(Role::Audit)
            } else {
                None
            }
        }
        _ => Some(Role::Plan),
    }
}

fn takeable(c: &Candidate, f: &Filter, ties: &[Tie], open: &HashSet<i64>) -> bool {
    c.open
        && c.turn == Some("agent")
        && !c.claimed
        && !c.waiting
        && !c.conflict
        && !c.kind.is_read_only()
        && role_of(c, ties, open).is_some_and(|r| f.roles.is_empty() || f.roles.contains(&r))
        && f.under.is_none_or(|u| u.contains(&c.rid))
        && f.complexity.is_none_or(|x| c.complexity == Some(x))
        && f.key.is_none_or(|k| c.key == k)
        && release_rank(f.releases, c.release).is_some()
        && (!f.current_release_only || in_current(f.releases, c.release))
        && f.theme
            .is_none_or(|t| c.theme.is_some_and(|mine| mine.eq_ignore_ascii_case(t)))
}

/// The queue an agent takes from, as `(rid, tier)`: the earliest release first, then the role's place in
/// the filter's roles, then the most urgent, then the oldest.
#[must_use]
pub fn next(items: &[Candidate], ties: &[Tie], filter: &Filter, limit: usize) -> Vec<(i64, usize)> {
    let open: HashSet<i64> = items.iter().filter(|c| c.open).map(|c| c.rid).collect();
    let mut rows: Vec<_> = items
        .iter()
        .filter(|c| takeable(c, filter, ties, &open))
        .filter(|c| filter.priority.is_none_or(|p| c.tier <= p))
        .collect();
    let role_rank = |c: &Candidate| {
        role_of(c, ties, &open)
            .and_then(|r| filter.roles.iter().position(|x| *x == r))
            .unwrap_or(0)
    };
    rows.sort_by_key(|c| {
        (
            release_rank(filter.releases, c.release),
            role_rank(c),
            c.tier,
            c.opened_at,
            c.rid,
        )
    });
    rows.into_iter()
        .take(limit)
        .map(|c| (c.rid, c.tier))
        .collect()
}

/// The needs an ask can name, in the order the owner's queue groups them: a device or thing in
/// hand, an account or store, an action from the owner's machine, a judgement.
pub const NEEDS: [&str; 4] = ["hold", "access", "act", "judge"];

/// The open asks the owner's queue holds when the project sets no `owner_limit`.
pub const OWNER_LIMIT: usize = 20;

/// The stored facts the owner's queue orders by. `derived` marks a decision an agent derived and the
/// owner has not yet confirmed or overturned; `need` is the one its ask named.
#[derive(Clone, Debug)]
pub struct OwnerRow<'a> {
    pub rid: i64,
    pub key: &'a str,
    pub kind: Kind,
    pub waiting: bool,
    pub derived: bool,
    pub need: Option<&'a str>,
    pub theme: Option<&'a str>,
    pub release: Option<&'a str>,
    pub tier: usize,
    pub asked_at: &'a str,
}

/// What `docket todo` narrows the owner's queue by.
#[derive(Clone, Debug, Default)]
pub struct OwnerFilter<'a> {
    pub priority: Option<usize>,
    pub key: Option<&'a str>,
    pub theme: Option<&'a str>,
    pub releases: &'a [String],
    pub limit: Option<usize>,
}

impl<'a> OwnerFilter<'a> {
    #[must_use]
    pub fn of(releases: &'a [String]) -> Self {
        OwnerFilter {
            releases,
            ..Self::default()
        }
    }
}

/// The owner's queue: each row's rid and group, and how many items wait on something first.
#[derive(Clone, Debug, Default)]
pub struct OwnerQueue {
    pub rows: Vec<(i64, &'static str)>,
    pub waiting: usize,
}

fn group_of(r: &OwnerRow) -> (usize, &'static str) {
    if r.derived {
        return (0, "derived");
    }
    if r.kind == Kind::Decision {
        return (1, "question");
    }
    match NEEDS.iter().position(|n| Some(*n) == r.need) {
        Some(i) => (2 + i, NEEDS[i]),
        None => (2 + NEEDS.len(), "other"),
    }
}

/// The queue the owner works from: derived answers to confirm, then questions, then asks by need;
/// within a group the earliest release, the most urgent, the oldest. Items waiting on something
/// else are left out and counted.
#[must_use]
pub fn owner_queue(items: &[OwnerRow], filter: &OwnerFilter) -> OwnerQueue {
    let mut rows: Vec<&OwnerRow> = items
        .iter()
        .filter(|r| filter.priority.is_none_or(|p| r.tier <= p))
        .filter(|r| filter.key.is_none_or(|k| r.key == k))
        .filter(|r| {
            filter
                .theme
                .is_none_or(|t| r.theme.is_some_and(|mine| mine.eq_ignore_ascii_case(t)))
        })
        .collect();
    let waiting = rows.iter().filter(|r| r.waiting && !r.derived).count();
    rows.retain(|r| r.derived || !r.waiting);
    rows.sort_by_key(|r| {
        (
            group_of(r).0,
            release_rank(filter.releases, r.release).unwrap_or(usize::MAX),
            r.tier,
            r.asked_at,
            r.rid,
        )
    });
    OwnerQueue {
        rows: rows
            .into_iter()
            .take(filter.limit.unwrap_or(usize::MAX))
            .map(|r| (r.rid, group_of(r).1))
            .collect(),
        waiting,
    }
}

/// Why an ask or an undecided question is refused: the owner already holds `limit` open asks.
#[must_use]
pub fn owner_limit_refusal(open: usize, limit: usize) -> Option<String> {
    (open >= limit).then(|| {
        format!(
            "The owner already holds {open} open asks, the owner_limit of {limit}. Derive it from a \
             recorded decision (docket answer --derived), depend on an ask already open, or wait."
        )
    })
}

#[cfg(test)]
#[path = "tests/queue.rs"]
mod tests;
