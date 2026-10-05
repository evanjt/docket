use serde::{Deserialize, Serialize};

pub const PRIORITIES: [&str; 4] = ["critical", "high", "normal", "low"];

/// What a project's key holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Work,
    Decision,
    Research,
    Audit,
    Story,
    Concept,
    Idea,
    Package,
}

impl Kind {
    /// A standing kind is open for good: never queued, never closed.
    #[must_use]
    pub fn is_standing(self) -> bool {
        matches!(self, Kind::Concept | Kind::Idea)
    }

    /// A kind kept to read: plans group the work, so nothing new is filed under these or claimed.
    #[must_use]
    pub fn is_read_only(self) -> bool {
        matches!(
            self,
            Kind::Package | Kind::Concept | Kind::Idea | Kind::Story
        )
    }
}

/// The stored facts the status word is derived from.
#[derive(Clone, Copy, Debug)]
pub struct Facts<'a> {
    pub state: &'a str,
    pub kind: Kind,
    pub claimed: bool,
    pub waiting: bool,
    pub turn: Option<&'a str>,
}

/// The one status word a person reads. A package still building is known only by its open members.
#[must_use]
pub fn word<'a>(facts: &Facts<'a>, open_members: u64) -> &'a str {
    if facts.state != "open" {
        return facts.state;
    }
    if facts.kind.is_standing() {
        return "standing";
    }
    if facts.claimed {
        return if facts.kind == Kind::Package {
            "checking"
        } else {
            "building"
        };
    }
    if facts.waiting {
        return "blocked";
    }
    if facts.turn == Some("user") {
        return "parked";
    }
    if open_members > 0 {
        return "building";
    }
    "ready"
}

/// An item's status word, the same on every surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Word {
    Done,
    Dropped,
    InProgress,
    Blocked,
    WaitingOnOwner,
    Parked,
    Building,
    AuditDue,
    Ready,
}

impl Word {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Word::Done => "done",
            Word::Dropped => "dropped",
            Word::InProgress => "in progress",
            Word::Blocked => "blocked",
            Word::WaitingOnOwner => "waiting on owner",
            Word::Parked => "parked",
            Word::Building => "building",
            Word::AuditDue => "audit due",
            Word::Ready => "ready",
        }
    }
}

/// What an item's status is derived from, gathered once from the item and its graph.
#[derive(Clone, Copy, Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Standing<'a> {
    pub state: &'a str,
    /// An assignment of the item is open.
    pub open_assignment: bool,
    /// A dependency of the item is not yet satisfied.
    pub unsatisfied_dependency: bool,
    /// The item's assignee is a person, or it is a question without a decision.
    pub owner_turn: bool,
    /// The item has a release.
    pub released: bool,
    /// What the item opened, to any depth: open and closed.
    pub open_members: u64,
    pub closed_members: u64,
}

/// The one status word of an item. The first rule that matches wins.
#[must_use]
pub fn status(item: &Standing<'_>) -> Word {
    match item.state {
        "done" => return Word::Done,
        "dropped" => return Word::Dropped,
        _ => {}
    }
    if item.open_assignment {
        Word::InProgress
    } else if item.unsatisfied_dependency {
        Word::Blocked
    } else if item.owner_turn {
        Word::WaitingOnOwner
    } else if !item.released {
        Word::Parked
    } else if item.open_members > 0 {
        Word::Building
    } else if item.closed_members > 0 {
        Word::AuditDue
    } else {
        Word::Ready
    }
}

/// The tier an item's tags name, normal when they name none.
#[must_use]
pub fn priority(tags: &[String]) -> &'static str {
    PRIORITIES
        .into_iter()
        .find(|p| tags.iter().any(|t| t == p))
        .unwrap_or("normal")
}

#[cfg(test)]
#[path = "tests/word.rs"]
mod tests;
