use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

/// What one item-to-item edge means. A parent edge is the only one with structure: the item belongs
/// to the plan. Related and origin, what spawned the item, are plain ties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Parent,
    Related,
    Origin,
}

impl Edge {
    /// The edge a stored link kind, or `parent` for an item's parent column, names.
    #[must_use]
    pub fn parse(kind: &str) -> Option<Self> {
        match kind {
            "parent" => Some(Edge::Parent),
            "related" => Some(Edge::Related),
            "origin" => Some(Edge::Origin),
            _ => None,
        }
    }
}

/// One edge from `rid` to `to`; a parent edge reads as child to parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tie {
    pub rid: i64,
    pub edge: Edge,
    pub to: i64,
}

impl Tie {
    #[must_use]
    pub fn is_parent(&self) -> bool {
        self.edge == Edge::Parent
    }
}

fn children_of(ties: &[Tie]) -> HashMap<i64, Vec<i64>> {
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    for t in ties.iter().filter(|t| t.is_parent()) {
        children.entry(t.to).or_default().push(t.rid);
    }
    children
}

fn below(children: &HashMap<i64, Vec<i64>>, root: i64) -> HashSet<i64> {
    let (mut seen, mut todo) = (HashSet::new(), vec![root]);
    while let Some(x) = todo.pop() {
        for &c in children.get(&x).into_iter().flatten() {
            if c != root && seen.insert(c) {
                todo.push(c);
            }
        }
    }
    seen
}

/// Every item under `rid` along parent edges, to any depth. The one walk down a plan.
#[must_use]
pub fn descendants(ties: &[Tie], rid: i64) -> HashSet<i64> {
    below(&children_of(ties), rid)
}

/// The plans above `rid`, its parent first, up to the top or the first one seen twice.
#[must_use]
pub fn ancestors(ties: &[Tie], rid: i64) -> Vec<i64> {
    let up: HashMap<i64, i64> = ties
        .iter()
        .filter(|t| t.is_parent())
        .map(|t| (t.rid, t.to))
        .collect();
    let (mut out, mut seen, mut at) = (Vec::new(), HashSet::from([rid]), rid);
    while let Some(&p) = up.get(&at) {
        if !seen.insert(p) {
            break;
        }
        out.push(p);
        at = p;
    }
    out
}

/// The labels an item carries: its own, then those of each plan above it, nearest first, each once.
/// A descendant inherits at read time, so a label is stored on the item it was given to alone.
#[must_use]
pub fn labels_carried<S: BuildHasher>(
    ties: &[Tie],
    own: &HashMap<i64, Vec<String>, S>,
    rid: i64,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for at in std::iter::once(rid).chain(ancestors(ties, rid)) {
        for label in own.get(&at).into_iter().flatten() {
            if !out.contains(label) {
                out.push(label.clone());
            }
        }
    }
    out
}

/// What belongs to a standing item: anything tied to it either way, with everything under that at
/// any depth. Standing items are never members of each other.
#[must_use]
pub fn members_of<S: BuildHasher>(
    ties: &[Tie],
    standing: &HashSet<i64, S>,
    rid: i64,
) -> HashSet<i64> {
    let children = children_of(ties);
    let mut out = HashSet::new();
    for t in ties.iter().filter(|t| !t.is_parent()) {
        let near = match (t.rid == rid, t.to == rid) {
            (true, _) => t.to,
            (_, true) => t.rid,
            _ => continue,
        };
        if standing.contains(&near) {
            continue;
        }
        out.insert(near);
        out.extend(below(&children, near));
    }
    out.retain(|x| !standing.contains(x));
    out
}

#[cfg(test)]
#[path = "tests/member.rs"]
mod tests;
