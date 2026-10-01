use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

/// One item-to-item link, `opened` or `related`; an opened tie reads as child to parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tie {
    pub rid: i64,
    pub opened: bool,
    pub to: i64,
}

fn children_of(ties: &[Tie]) -> HashMap<i64, Vec<i64>> {
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    for t in ties.iter().filter(|t| t.opened) {
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

/// Every item `rid` opened, and everything those opened, to any depth.
#[must_use]
pub fn opened_under(ties: &[Tie], rid: i64) -> HashSet<i64> {
    below(&children_of(ties), rid)
}

/// What belongs to a standing item: anything tied to it either way, with all opened under that at any
/// depth. Standing items are never members of each other.
#[must_use]
pub fn members_of<S: BuildHasher>(
    ties: &[Tie],
    standing: &HashSet<i64, S>,
    rid: i64,
) -> HashSet<i64> {
    let children = children_of(ties);
    let mut out = HashSet::new();
    for t in ties {
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

/// Every tie by the item at either end, and every item by those that opened it.
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
            near.entry(t.rid).or_default().push(t.to);
            near.entry(t.to).or_default().push(t.rid);
            if t.opened {
                parents.entry(t.rid).or_default().push(t.to);
            }
        }
        Self { near, parents }
    }

    /// The concepts an item belongs to: tied to it either way, or to anything that opened it, at any
    /// depth.
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
