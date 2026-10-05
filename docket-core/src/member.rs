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

/// Every plain tie by the item at either end, and every item's parent.
pub struct Neighbours {
    near: HashMap<i64, Vec<i64>>,
    parents: HashMap<i64, Vec<i64>>,
}

impl Neighbours {
    #[must_use]
    pub fn new(ties: &[Tie]) -> Self {
        let mut near: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut parents: HashMap<i64, Vec<i64>> = HashMap::new();
        for t in ties {
            if t.is_parent() {
                parents.entry(t.rid).or_default().push(t.to);
            } else {
                near.entry(t.rid).or_default().push(t.to);
                near.entry(t.to).or_default().push(t.rid);
            }
        }
        Self { near, parents }
    }

    /// The concepts an item belongs to: tied to it either way, or to any plan above it.
    #[must_use]
    pub fn concepts_of<S: BuildHasher>(
        &self,
        concepts: &HashSet<i64, S>,
        rid: i64,
    ) -> HashSet<i64> {
        let (mut seen, mut todo, mut out) = (HashSet::new(), vec![rid], HashSet::new());
        while let Some(x) = todo.pop() {
            if !seen.insert(x) {
                continue;
            }
            let near = self.near.get(&x).into_iter().flatten();
            out.extend(near.filter(|o| **o != rid && concepts.contains(*o)));
            todo.extend(self.parents.get(&x).into_iter().flatten());
        }
        out
    }
}

#[cfg(test)]
#[path = "tests/member.rs"]
mod tests;
