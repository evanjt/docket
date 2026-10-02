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
