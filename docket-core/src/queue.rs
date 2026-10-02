use std::collections::HashSet;

use crate::like;
use crate::member::{Tie, opened_under};
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
    pub tier: usize,
    pub opened_at: &'a str,
}

/// The three roles a session takes from the queue with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// A goal into tickets: a plan that has opened nothing yet, an investigation, a decided question.
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
    pub role: Option<Role>,
    pub priority: Option<usize>,
    pub key: Option<&'a str>,
    pub under: Option<&'a HashSet<i64>>,
    pub complexity: Option<&'a str>,
    pub theme: Option<&'a str>,
    /// The releases in the order they ship, from the `releases` fact; empty orders by priority alone.
    pub releases: &'a [String],
}

/// Where an item's release falls: its theme's place in the releases, and 0, the current release,
/// for no theme or a theme the releases do not list.
#[must_use]
pub fn release_rank(releases: &[String], theme: Option<&str>) -> usize {
    theme
        .and_then(|t| releases.iter().position(|r| r == t))
        .unwrap_or(0)
}

/// The role an open item is taken by. A plan that has opened something is due for its audit once
/// everything it opened, at any depth, is closed; before that it belongs to no role.
fn role_of(c: &Candidate, ties: &[Tie], open: &HashSet<i64>) -> Option<Role> {
    match c.kind {
        Kind::Work => Some(Role::Work),
        Kind::Audit => {
            let under = opened_under(ties, c.rid);
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
        && f.role.is_none_or(|r| role_of(c, ties, open) == Some(r))
        && f.under.is_none_or(|u| u.contains(&c.rid))
        && f.complexity.is_none_or(|x| c.complexity == Some(x))
        && f.key.is_none_or(|k| c.key == k)
        && f.theme
            .is_none_or(|t| c.theme.is_some_and(|mine| like::contains(mine, t)))
}

/// The queue an agent takes from, as `(rid, tier)`: the earliest release first, then the most urgent,
/// then the oldest.
#[must_use]
pub fn next(items: &[Candidate], ties: &[Tie], filter: &Filter, limit: usize) -> Vec<(i64, usize)> {
    let open: HashSet<i64> = items.iter().filter(|c| c.open).map(|c| c.rid).collect();
    let mut rows: Vec<_> = items
        .iter()
        .filter(|c| takeable(c, filter, ties, &open))
        .filter(|c| filter.priority.is_none_or(|p| c.tier <= p))
        .collect();
    rows.sort_by_key(|c| {
        (
            release_rank(filter.releases, c.theme),
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

#[cfg(test)]
#[path = "tests/queue.rs"]
mod tests;
