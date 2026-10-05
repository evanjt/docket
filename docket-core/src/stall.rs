//! What holds an item, and what stalls a queue for good: items that hold each other in a cycle, and
//! an item held by one that ships in a later release. An edge runs from an open item to what holds
//! it: each dependency not yet satisfied, or, for a plan, each of its open members. A write that
//! would close a cycle is refused with the path it closes.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::hash::BuildHasher;

use crate::member::{Tie, descendants};
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

/// The path of holds from `from` to `to`, both ends included, when there is one. A new hold of `to`
/// on `from` would close a cycle along it, so the write that adds it is refused with this path.
#[must_use]
pub fn path(holds: &BTreeMap<i64, Vec<i64>>, from: i64, to: i64) -> Option<Vec<i64>> {
    let mut back: BTreeMap<i64, i64> = BTreeMap::new();
    let mut todo = VecDeque::from([from]);
    let mut seen = BTreeSet::from([from]);
    while let Some(x) = todo.pop_front() {
        if x == to {
            let mut out = vec![x];
            let mut at = x;
            while let Some(prev) = back.get(&at) {
                out.push(*prev);
                at = *prev;
            }
            out.reverse();
            return Some(out);
        }
        for n in holds.get(&x).into_iter().flatten() {
            if seen.insert(*n) {
                back.insert(*n, x);
                todo.push_back(*n);
            }
        }
    }
    None
}

/// What a dependency points at, as far as satisfying it goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Open,
    /// An open question with its decision recorded.
    Decided,
    Done,
    /// Dropped, with the item that superseded it, if any.
    Dropped(Option<i64>),
}

impl Target {
    #[must_use]
    pub fn of(state: &str, decided: bool, superseded_by: Option<i64>) -> Self {
        match state {
            "done" => Target::Done,
            "dropped" => Target::Dropped(superseded_by),
            _ if decided => Target::Decided,
            _ => Target::Open,
        }
    }
}

/// Where a chain of successors from `on` ends: the item it ends at and how that item stands. A loop
/// of successors ends where it comes back round.
fn settle(on: i64, targets: &BTreeMap<i64, Target>) -> (i64, Option<Target>) {
    let mut at = on;
    let mut seen = BTreeSet::new();
    loop {
        let t = targets.get(&at).copied();
        match t {
            Some(Target::Dropped(Some(next))) if seen.insert(at) && !seen.contains(&next) => {
                at = next;
            }
            _ => return (at, t),
        }
    }
}

/// The item that still holds a dependency on `on`: `on` itself while it is open and undecided, its
/// successor's holder when it was dropped for one, none once it is satisfied.
#[must_use]
pub fn holder(on: i64, targets: &BTreeMap<i64, Target>) -> Option<i64> {
    match settle(on, targets) {
        (at, Some(Target::Open)) => Some(at),
        _ => None,
    }
}

/// A dependency on `on` is satisfied once it is done or a decided question, or dropped without a
/// successor; a dropped item with a successor passes the dependency on to it.
#[must_use]
pub fn satisfied(on: i64, targets: &BTreeMap<i64, Target>) -> bool {
    holder(on, targets).is_none()
}

/// Whether the dependency on `on` was satisfied by a drop that named no successor: nothing was done
/// for it, so a check flags it.
#[must_use]
pub fn abandoned(on: i64, targets: &BTreeMap<i64, Target>) -> bool {
    matches!(settle(on, targets).1, Some(Target::Dropped(_)))
}

/// Whether a dependency on `on` reaches `via`: it is `via`, or a chain of successors runs from it to
/// `via`.
#[must_use]
pub fn passes(on: i64, via: i64, targets: &BTreeMap<i64, Target>) -> bool {
    let mut at = on;
    let mut seen = BTreeSet::new();
    loop {
        if at == via {
            return true;
        }
        match targets.get(&at) {
            Some(Target::Dropped(Some(next))) if seen.insert(at) => at = *next,
            _ => return false,
        }
    }
}

/// The items that still hold an item depending on each of `on`, in that order, each once.
#[must_use]
pub fn holders(on: &[i64], targets: &BTreeMap<i64, Target>) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    for h in on.iter().filter_map(|d| holder(*d, targets)) {
        if !out.contains(&h) {
            out.push(h);
        }
    }
    out
}

/// The holds of each gated plan: an edge from the plan to every open item it opened, at any depth,
/// through items already closed. `ties` are the opened and related ties, `open` the open items.
#[must_use]
pub fn gate_edges<S: BuildHasher>(
    ties: &[Tie],
    gated: &[i64],
    open: &HashSet<i64, S>,
) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    for &plan in gated {
        let mut members: Vec<i64> = descendants(ties, plan)
            .into_iter()
            .filter(|m| open.contains(m))
            .collect();
        members.sort_unstable();
        out.extend(members.into_iter().map(|m| (plan, m)));
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

/// Whether a hold of an item in `held` by one in `holder` runs into a later release. The backlog
/// ships after every release.
#[must_use]
pub fn runs_later(releases: &[String], held: Option<&str>, holder: Option<&str>) -> bool {
    let rank = |r| release_rank(releases, r).unwrap_or(usize::MAX);
    rank(holder) > rank(held)
}

/// The events that move an item forward. Edits, links, labels and release moves are bookkeeping and
/// leave an item as idle as it was.
const FORWARD: [&str; 11] = [
    "opened", "claimed", "released", "waited", "resumed", "asked", "replied", "decided", "closed",
    "dropped", "reopened",
];

/// When an item last moved forward: the latest of its `(kind, epoch seconds)` events that is one of
/// the forward kinds; none when it has none.
#[must_use]
pub fn idle_since(events: &[(&str, i64)]) -> Option<i64> {
    events
        .iter()
        .filter(|(kind, _)| FORWARD.contains(kind))
        .map(|(_, at)| *at)
        .max()
}

#[cfg(test)]
#[path = "tests/stall.rs"]
mod tests;
