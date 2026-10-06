//! Every item of one project and the ties between them, read once per change, so words, packages and
//! trees are worked out here rather than asked for item by item.

use std::collections::{HashMap, HashSet};

use crate::member::{Edge, Tie, descendants};
use crate::metrics::{Progress, Subject, progress};
use crate::rows::{ItemRow, ProjectRow};
use crate::word::{Kind, Standing, kind_of_type};

/// The open plans as the word counts them: under way, and due for their audit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlanCount {
    pub open: u64,
    pub audit_due: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Board {
    pub project: ProjectRow,
    pub items: Vec<ItemRow>,
    pub ties: Vec<Tie>,
    by_id: HashMap<String, usize>,
    by_rid: HashMap<i64, usize>,
    children: HashMap<i64, Vec<i64>>,
}

impl Board {
    /// The board of the items and the plain ties given. Each item's parent edge is read from the
    /// item itself.
    #[must_use]
    pub fn new(project: ProjectRow, items: Vec<ItemRow>, mut ties: Vec<Tie>) -> Self {
        for i in &items {
            if let Some(p) = i.parent_rid {
                let t = Tie {
                    rid: i.rid,
                    edge: Edge::Parent,
                    to: p,
                };
                if !ties.contains(&t) {
                    ties.push(t);
                }
            }
        }
        let ids = items
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id.clone(), i))
            .collect();
        let rids = items.iter().enumerate().map(|(i, r)| (r.rid, i)).collect();
        let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
        for t in ties.iter().filter(|t| t.is_parent()) {
            children.entry(t.to).or_default().push(t.rid);
        }
        Self {
            project,
            items,
            ties,
            by_id: ids,
            by_rid: rids,
            children,
        }
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&ItemRow> {
        self.by_id.get(id).map(|i| &self.items[*i])
    }

    #[must_use]
    pub fn by_rid(&self, rid: i64) -> Option<&ItemRow> {
        self.by_rid.get(&rid).map(|i| &self.items[*i])
    }

    #[must_use]
    pub fn kind(&self, item: &ItemRow) -> Kind {
        kind_of_type(&item.item_type)
    }

    /// The items whose parent an item is, in key and number order.
    #[must_use]
    pub fn children(&self, rid: i64) -> Vec<&ItemRow> {
        let mut out: Vec<&ItemRow> = self
            .children
            .get(&rid)
            .into_iter()
            .flatten()
            .filter_map(|r| self.by_rid(*r))
            .collect();
        out.sort_by(|a, b| (&a.key, a.num).cmp(&(&b.key, b.num)));
        out
    }

    /// The items whose origin is an item, in key and number order.
    #[must_use]
    pub fn spawned(&self, rid: i64) -> Vec<&ItemRow> {
        let mut out: Vec<&ItemRow> = self
            .ties
            .iter()
            .filter(|t| t.edge == Edge::Origin && t.to == rid)
            .filter_map(|t| self.by_rid(t.rid))
            .collect();
        out.sort_by(|a, b| (&a.key, a.num).cmp(&(&b.key, b.num)));
        out.dedup_by_key(|i| i.rid);
        out
    }

    /// The one status word, as the server derives it.
    #[must_use]
    pub fn word(&self, item: &ItemRow) -> String {
        let kind = self.kind(item);
        let held = self.holds(item);
        let members: Vec<&ItemRow> = held.iter().filter_map(|r| self.by_rid(*r)).collect();
        let open = members.iter().filter(|m| m.state == "open").count() as u64;
        let closed = members.len() as u64 - open;
        Standing::of(
            &item.state,
            item.claim_branch.is_some(),
            item.wait_on.is_some(),
            item.turn.as_deref(),
        )
        .holding(kind, open, closed)
        .word()
        .to_string()
    }

    /// The word of an id, `None` for an id the project does not hold.
    #[must_use]
    pub fn word_of(&self, id: &str) -> Option<String> {
        self.get(id).map(|i| self.word(i))
    }

    /// The tickets that carry a word, packages aside as the flow counts them, by key and number.
    #[must_use]
    pub fn with_word(&self, w: &str) -> Vec<&ItemRow> {
        let mut out: Vec<&ItemRow> = self
            .items
            .iter()
            .filter(|i| self.kind(i) != Kind::Package && self.word(i) == w)
            .collect();
        out.sort_by(|a, b| (&a.key, a.num).cmp(&(&b.key, b.num)));
        out
    }

    /// What an item holds: a package's children, everything under a plan or story at any depth. Empty for a ticket.
    #[must_use]
    pub fn holds(&self, item: &ItemRow) -> HashSet<i64> {
        match self.kind(item) {
            Kind::Package => self.children(item.rid).iter().map(|c| c.rid).collect(),
            Kind::Audit | Kind::Story => descendants(&self.ties, item.rid),
            _ => HashSet::new(),
        }
    }

    /// An item as the metrics count it: its word, its claim, its area and whether it holds others.
    #[must_use]
    pub fn subject(&self, item: &ItemRow) -> Subject {
        let kind = self.kind(item);
        Subject {
            area: item.area_id,
            holder: kind.is_plan() || kind == Kind::Idea,
            ..Subject::member(&item.id, &self.word(item), item.claim_branch.is_some())
        }
    }

    /// Every item as the metrics count it.
    #[must_use]
    pub fn subjects(&self) -> Vec<Subject> {
        self.items.iter().map(|i| self.subject(i)).collect()
    }

    /// What an item holds, as the metrics count it.
    #[must_use]
    pub fn held_subjects(&self, item: &ItemRow) -> Vec<Subject> {
        self.holds(item)
            .into_iter()
            .filter_map(|r| self.by_rid(r))
            .map(|m| self.subject(m))
            .collect()
    }

    /// The progress of what an item holds.
    #[must_use]
    pub fn held_progress(&self, item: &ItemRow) -> Progress {
        progress(&self.held_subjects(item))
    }

    /// The plans whose word is under way, and those whose word is audit due.
    #[must_use]
    pub fn plan_count(&self) -> PlanCount {
        let plans = self.open_of(&[Kind::Audit]);
        let with = |w: &str| plans.iter().filter(|p| self.word(p) == w).count() as u64;
        PlanCount {
            open: with("under way"),
            audit_due: with("audit due"),
        }
    }

    /// Plans whose word is under way: those being worked first, then the nearest done.
    #[must_use]
    pub fn plans_under_way(&self) -> Vec<(&ItemRow, Progress)> {
        let mut out: Vec<(&ItemRow, Progress)> = self
            .open_of(&[Kind::Audit])
            .into_iter()
            .filter(|p| self.word(p) == "under way")
            .map(|p| (p, self.held_progress(p)))
            .collect();
        out.sort_by(|(a, x), (b, y)| {
            let near = |g: &Progress| g.done * 1000 / g.counted.max(1);
            (y.live, near(y), x.counted - x.done, &a.id).cmp(&(
                x.live,
                near(x),
                y.counted - y.done,
                &b.id,
            ))
        });
        out
    }

    /// Whether an open plan is due for its audit: it has children, everything under it, at any depth,
    /// is closed, and nobody holds it yet.
    #[must_use]
    pub fn due(&self, item: &ItemRow) -> bool {
        if item.state != "open" || self.kind(item) != Kind::Audit {
            return false;
        }
        item.wait_on.is_none()
            && item.claim_branch.is_none()
            && item.turn.as_deref() == Some("agent")
            && self.held_progress(item).settled()
    }

    /// Plans due for their audit, by key and number.
    #[must_use]
    pub fn due_audits(&self) -> Vec<&ItemRow> {
        self.open_of(&[Kind::Audit])
            .into_iter()
            .filter(|p| self.due(p))
            .collect()
    }

    /// Open items of the kinds, by key and number.
    #[must_use]
    pub fn open_of(&self, kinds: &[Kind]) -> Vec<&ItemRow> {
        let mut out: Vec<&ItemRow> = self
            .items
            .iter()
            .filter(|i| i.state == "open" && kinds.contains(&self.kind(i)))
            .collect();
        out.sort_by(|a, b| (&a.key, a.num).cmp(&(&b.key, b.num)));
        out
    }

    /// Items on the owner's turn, as `/todo` counts them.
    #[must_use]
    pub fn on_owner(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.state == "open" && i.turn.as_deref() == Some("user"))
            .count()
    }

    /// Questions still undecided, as `/questions` counts them.
    #[must_use]
    pub fn undecided(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.state == "open" && i.decision.is_none() && self.kind(i) == Kind::Decision)
            .count()
    }
}

#[cfg(test)]
#[path = "tests/board.rs"]
mod tests;
