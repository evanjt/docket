use std::collections::HashSet;

use crate::like;
use crate::member::Tie;
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
}

/// The role an open item is taken by: a plan is audited once it has opened something, since it is
/// in the queue only when all of that is closed.
fn role_of(c: &Candidate, opened: &HashSet<i64>) -> Role {
    match c.kind {
        Kind::Work => Role::Work,
        Kind::Audit if opened.contains(&c.rid) => Role::Audit,
        _ => Role::Plan,
    }
}

fn takeable(c: &Candidate, f: &Filter, opened: &HashSet<i64>) -> bool {
    c.open
        && c.turn == Some("agent")
        && !c.claimed
        && !c.waiting
        && !c.conflict
        && !c.kind.is_read_only()
        && f.role.is_none_or(|r| role_of(c, opened) == r)
        && f.under.is_none_or(|u| u.contains(&c.rid))
        && f.complexity.is_none_or(|x| c.complexity == Some(x))
        && f.key.is_none_or(|k| c.key == k)
        && f.theme
            .is_none_or(|t| c.theme.is_some_and(|mine| like::contains(mine, t)))
}

/// The queue an agent takes from, as `(rid, tier)`: most urgent first, then oldest first.
#[must_use]
pub fn next(items: &[Candidate], ties: &[Tie], filter: &Filter, limit: usize) -> Vec<(i64, usize)> {
    let opened: HashSet<i64> = ties.iter().filter(|t| t.opened).map(|t| t.to).collect();
    let mut rows: Vec<_> = items
        .iter()
        .filter(|c| takeable(c, filter, &opened))
        .filter(|c| filter.priority.is_none_or(|p| c.tier <= p))
        .collect();
    rows.sort_by_key(|c| (c.tier, c.opened_at, c.rid));
    rows.into_iter()
        .take(limit)
        .map(|c| (c.rid, c.tier))
        .collect()
}

/// `(name, cut date)` from the release fact, or `None` when it is unset or carries no date.
#[must_use]
pub fn release_of(fact: Option<&str>) -> Option<(String, String)> {
    let parts: Vec<&str> = fact.unwrap_or_default().split_whitespace().collect();
    let (&cut, name) = parts.split_last()?;
    if name.is_empty() || !is_date(cut) {
        return None;
    }
    Some((name.join(" "), cut.to_string()))
}

fn is_date(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

#[cfg(test)]
#[path = "tests/queue.rs"]
mod tests;
