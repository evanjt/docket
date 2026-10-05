//! What stalls a queue for good: items that hold each other in a cycle, and an item held by one that
//! ships in a later release. An edge runs from an open item to what holds it: the item it waits on,
//! or, for a container waiting until everything it opened is closed, each of its open members.

use std::collections::{BTreeMap, BTreeSet};

use crate::queue::release_rank;

/// Every item on a cycle of holds, a self-hold included. An item whose chain only runs into a cycle
/// is not on it.
#[must_use]
pub fn cycles(edges: &BTreeMap<i64, Vec<i64>>) -> BTreeSet<i64> {
    let mut out = BTreeSet::new();
    for &start in edges.keys() {
        let mut seen = BTreeSet::new();
        let mut stack: Vec<i64> = edges[&start].clone();
        while let Some(n) = stack.pop() {
            if n == start {
                out.insert(start);
                break;
            }
            if seen.insert(n)
                && let Some(next) = edges.get(&n)
            {
                stack.extend(next);
            }
        }
    }
    out
}

/// Each hold that runs into a later release, as `(held, holder)`: the held item can never close
/// before the later release's work is done. `rank` is each item's place in the releases.
#[must_use]
pub fn held_later(edges: &BTreeMap<i64, Vec<i64>>, rank: &BTreeMap<i64, usize>) -> Vec<(i64, i64)> {
    let at = |rid: &i64| rank.get(rid).copied().unwrap_or(0);
    let mut out = Vec::new();
    for (from, tos) in edges {
        for to in tos {
            if at(to) > at(from) {
                out.push((*from, *to));
            }
        }
    }
    out
}

/// Whether a hold of an item in `held` by one in `holder` runs into a later release. A theme the
/// releases do not list ranks with the current release.
#[must_use]
pub fn runs_later(releases: &[String], held: Option<&str>, holder: Option<&str>) -> bool {
    release_rank(releases, holder) > release_rank(releases, held)
}

#[cfg(test)]
#[path = "tests/stall.rs"]
mod tests;
