use serde::{Deserialize, Serialize};

pub const PRIORITIES: [&str; 4] = ["critical", "high", "normal", "low"];

/// What an item is, fixed by docket. Each type names the key its new items are filed under.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    #[default]
    Task,
    Bug,
    Question,
    Investigation,
    Plan,
}

impl ItemType {
    pub const ALL: [ItemType; 5] = [
        ItemType::Task,
        ItemType::Bug,
        ItemType::Question,
        ItemType::Investigation,
        ItemType::Plan,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ItemType::Task => "task",
            ItemType::Bug => "bug",
            ItemType::Question => "question",
            ItemType::Investigation => "investigation",
            ItemType::Plan => "plan",
        }
    }

    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == word)
    }

    /// The key new items of the type are filed under.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            ItemType::Task => "T",
            ItemType::Bug => "B",
            ItemType::Question => "Q",
            ItemType::Investigation => "I",
            ItemType::Plan => "A",
        }
    }

    /// The kind whose rules the type's items follow.
    #[must_use]
    pub fn kind(self) -> Kind {
        match self {
            ItemType::Task | ItemType::Bug => Kind::Work,
            ItemType::Question => Kind::Decision,
            ItemType::Investigation => Kind::Research,
            ItemType::Plan => Kind::Audit,
        }
    }

    /// The type a key files under, none for a retired key.
    #[must_use]
    pub fn filed_under(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    /// The type an item gets that was filed under a key of a project's matrix: its own key's when the
    /// key is a fixed one, else by the kind the project gave the key. A bug key holds bugs and every
    /// other work key tasks; concept and idea kinds are tasks until they are migrated.
    #[must_use]
    pub fn of_stored(key: &str, kind: Kind) -> Self {
        if let Some(t) = Self::filed_under(key) {
            return t;
        }
        match kind {
            Kind::Work | Kind::Concept | Kind::Idea => ItemType::Task,
            Kind::Decision => ItemType::Question,
            Kind::Research => ItemType::Investigation,
            Kind::Audit | Kind::Story | Kind::Package => ItemType::Plan,
        }
    }
}

/// The kind of the type a stored word names, work for a word that is none.
#[must_use]
pub fn kind_of_type(word: &str) -> Kind {
    ItemType::parse(word).unwrap_or_default().kind()
}

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
    /// A kind that is a plan and holds children: a plan, and the story and package kinds before it.
    #[must_use]
    pub fn is_plan(self) -> bool {
        matches!(self, Kind::Audit | Kind::Story | Kind::Package)
    }

    /// A package or story kind, which plans replace: kept to read, never queued or claimed.
    #[must_use]
    pub fn is_retired_plan(self) -> bool {
        matches!(self, Kind::Package | Kind::Story)
    }
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
    /// The item's assignee is the owner, or it is a question without a decision.
    pub owner_turn: bool,
    /// The item has a release.
    pub released: bool,
    /// What the item opened, to any depth: open and closed.
    pub open_members: u64,
    pub closed_members: u64,
}

impl<'a> Standing<'a> {
    /// The standing of an item from its stored columns: the claim is its open assignment, a wait
    /// its unsatisfied dependency, the user's turn the owner's, and every item is released.
    #[must_use]
    pub fn of(state: &'a str, claimed: bool, waiting: bool, turn: Option<&str>) -> Self {
        Standing {
            state,
            open_assignment: claimed,
            unsatisfied_dependency: waiting,
            owner_turn: turn == Some("user"),
            released: true,
            open_members: 0,
            closed_members: 0,
        }
    }

    /// The members a plan holds at any depth. A package is known by its open members only: a
    /// finished package is not audited.
    #[must_use]
    pub fn holding(self, kind: Kind, open: u64, closed: u64) -> Self {
        Standing {
            open_members: if kind.is_plan() { open } else { 0 },
            closed_members: if matches!(kind, Kind::Audit | Kind::Story) {
                closed
            } else {
                0
            },
            ..self
        }
    }

    /// The word every read shows.
    #[must_use]
    pub fn word(&self) -> &'static str {
        status(self).as_str()
    }
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

/// The tier an item's tags name, normal when they name none: how an item stored before the priority
/// column carries it.
#[must_use]
pub fn priority(tags: &[String]) -> &'static str {
    PRIORITIES
        .into_iter()
        .find(|p| tags.iter().any(|t| t == p))
        .unwrap_or("normal")
}

/// Tags with the priority words taken out.
#[must_use]
pub fn without_priority(tags: &[String]) -> Vec<String> {
    tags.iter()
        .filter(|t| !PRIORITIES.contains(&t.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "tests/word.rs"]
mod tests;
