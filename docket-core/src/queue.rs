use std::collections::{HashMap, HashSet};

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
    pub scope: Option<&'a str>,
    pub complexity: Option<&'a str>,
    pub theme: Option<&'a str>,
    pub rank: Option<i64>,
    pub tier: usize,
    pub opened_at: &'a str,
}

/// What `docket next` narrows the queue by. `release` lists the themes inside the release.
#[derive(Clone, Debug, Default)]
pub struct Filter<'a> {
    pub scope: Option<&'a str>,
    pub priority: Option<usize>,
    pub key: Option<&'a str>,
    pub under: Option<&'a HashSet<i64>>,
    pub complexity: Option<&'a str>,
    pub theme: Option<&'a str>,
    pub without_theme: Option<&'a str>,
    pub release: Option<&'a [String]>,
}

struct Graph<'a> {
    by_rid: HashMap<i64, &'a Candidate<'a>>,
    parents: HashMap<i64, Vec<i64>>,
    children: HashMap<i64, Vec<i64>>,
}

impl<'a> Graph<'a> {
    fn new(items: &'a [Candidate<'a>], ties: &[Tie]) -> Self {
        let (mut parents, mut children): (HashMap<i64, Vec<i64>>, HashMap<i64, Vec<i64>>) =
            (HashMap::new(), HashMap::new());
        for t in ties.iter().filter(|t| t.opened) {
            parents.entry(t.rid).or_default().push(t.to);
            children.entry(t.to).or_default().push(t.rid);
        }
        let by_rid = items.iter().map(|c| (c.rid, c)).collect();
        Self {
            by_rid,
            parents,
            children,
        }
    }

    /// The open packages that opened an item.
    fn packages(&self, rid: i64) -> impl Iterator<Item = &Candidate<'a>> {
        self.parents
            .get(&rid)
            .into_iter()
            .flatten()
            .filter_map(|p| self.by_rid.get(p).copied())
            .filter(|p| p.open && p.kind == Kind::Package)
    }

    fn members(&self, rid: i64) -> impl Iterator<Item = &Candidate<'a>> {
        self.children
            .get(&rid)
            .into_iter()
            .flatten()
            .filter_map(|m| self.by_rid.get(m).copied())
    }

    /// A ticket is as urgent as the most urgent open package holding it.
    fn tier(&self, c: &Candidate) -> usize {
        self.packages(c.rid)
            .map(|p| p.tier)
            .fold(c.tier, usize::min)
    }

    /// A package is ready only for its review: it holds members and none is open.
    fn reviewable(&self, rid: i64) -> bool {
        self.children.get(&rid).is_some_and(|m| !m.is_empty()) && self.members(rid).all(|m| !m.open)
    }

    /// Whether a package holding the item has a member closed or claimed already.
    fn under_way(&self, rid: i64) -> bool {
        self.packages(rid)
            .any(|p| self.members(p.rid).any(|s| !s.open || s.claimed))
    }
}

fn takeable(c: &Candidate, f: &Filter) -> bool {
    c.open
        && c.turn == Some("agent")
        && !c.claimed
        && !c.waiting
        && !c.conflict
        && c.scope == f.scope
        && (!c.kind.is_standing() || f.key == Some(c.key))
        && f.under.is_none_or(|u| u.contains(&c.rid))
        && f.complexity.is_none_or(|x| c.complexity == Some(x))
        && f.key.is_none_or(|k| c.key == k)
        && f.theme
            .is_none_or(|t| c.theme.is_some_and(|mine| like::contains(mine, t)))
        && f.without_theme
            .is_none_or(|t| c.theme.is_none_or(|mine| !like::contains(mine, t)))
        && f.release
            .is_none_or(|themes| c.theme.is_none_or(|mine| themes.iter().any(|t| t == mine)))
}

/// The queue an agent takes from, most urgent first, as `(rid, effective tier)`. A review goes before
/// a fix, a fix before research or a decision, and a fix of a package under way before the rest.
#[must_use]
pub fn next(items: &[Candidate], ties: &[Tie], filter: &Filter, limit: usize) -> Vec<(i64, usize)> {
    let graph = Graph::new(items, ties);
    let mut rows: Vec<_> = items
        .iter()
        .filter(|c| takeable(c, filter))
        .filter(|c| c.kind != Kind::Package || graph.reviewable(c.rid))
        .map(|c| (graph.tier(c), c))
        .filter(|(tier, _)| filter.priority.is_none_or(|p| *tier <= p))
        .collect();
    rows.sort_by_key(|(tier, c)| {
        let class = match c.kind {
            Kind::Package => 0,
            Kind::Work => 1,
            _ => 2,
        };
        (
            *tier,
            class,
            !graph.under_way(c.rid),
            c.rank.is_none(),
            c.rank,
            c.opened_at,
        )
    });
    rows.into_iter()
        .take(limit)
        .map(|(tier, c)| (c.rid, tier))
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
