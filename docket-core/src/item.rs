//! An item as the rules read it, and the changes a rule returns.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::assignment::{Ask, Claim, Held};
use crate::word::{ItemType, Kind};

/// A verb that cannot apply. The message is the whole explanation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refused(pub String);

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

/// The stored columns of one item and its open assignment. What it waits on is read from its
/// dependencies, never stored here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub rid: i64,
    pub project: String,
    pub key: String,
    pub num: i64,
    pub id: String,
    pub title: String,
    pub state: String,
    /// The item's open assignment: the claim an agent holds on it, or the owner's ask.
    pub held: Option<Held>,
    pub decision: Option<String>,
    pub decided_at: Option<String>,
    pub resolution: Option<String>,
    pub superseded_by: Option<i64>,
    /// The plan the item belongs to.
    pub parent_rid: Option<i64>,
    pub complexity: Option<String>,
    /// The release row the item is in; none is the backlog.
    pub release_id: Option<i64>,
    /// The area row the item is in.
    pub area_id: Option<i64>,
    /// What the item is; its rules follow from it.
    pub item_type: ItemType,
    /// One of critical, high, normal and low.
    pub priority: String,
    pub body: String,
    pub opened_at: String,
    pub updated_at: String,
}

/// One column a verb sets. A verb returns the list of them, in the order it names them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Field {
    Title(String),
    State(String),
    Decision(Option<String>),
    DecidedAt(Option<String>),
    Resolution(Option<String>),
    SupersededBy(Option<i64>),
    ParentRid(Option<i64>),
    Complexity(Option<String>),
    ReleaseId(Option<i64>),
    AreaId(Option<i64>),
    Priority(String),
    Body(String),
}

impl Item {
    /// The claim an agent holds on the item.
    #[must_use]
    pub fn claim(&self) -> Option<&Claim> {
        match &self.held {
            Some(Held::Claim(c)) => Some(c),
            _ => None,
        }
    }

    /// The branch holding the item.
    #[must_use]
    pub fn claim_branch(&self) -> Option<&str> {
        self.claim().map(|c| c.branch.as_str())
    }

    /// What the owner was asked, while it is their turn.
    #[must_use]
    pub fn ask(&self) -> Option<&Ask> {
        match &self.held {
            Some(Held::Ask(a)) => Some(a),
            _ => None,
        }
    }

    /// Whose turn the open item is, `user` while the owner's ask is open, `agent` otherwise; none
    /// once it is closed.
    #[must_use]
    pub fn turn(&self) -> Option<&'static str> {
        turn_of(&self.state, self.ask().is_some())
    }

    /// The item with the changes applied, as the row reads after the update.
    pub fn apply(&mut self, changes: &[Field]) {
        for c in changes {
            match c.clone() {
                Field::Title(v) => self.title = v,
                Field::State(v) => self.state = v,
                Field::Decision(v) => self.decision = v,
                Field::DecidedAt(v) => self.decided_at = v,
                Field::Resolution(v) => self.resolution = v,
                Field::SupersededBy(v) => self.superseded_by = v,
                Field::ParentRid(v) => self.parent_rid = v,
                Field::Complexity(v) => self.complexity = v,
                Field::ReleaseId(v) => self.release_id = v,
                Field::AreaId(v) => self.area_id = v,
                Field::Priority(v) => self.priority = v,
                Field::Body(v) => self.body = v,
            }
        }
    }
}

/// Whose turn an item in `state` is: the owner's while an ask is open, the agents' while it is
/// open, none once it is closed.
#[must_use]
pub fn turn_of(state: &str, asked: bool) -> Option<&'static str> {
    match (state, asked) {
        ("open", true) => Some("user"),
        ("open", false) => Some("agent"),
        _ => None,
    }
}

/// Who acts, from where, and when.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ctx {
    pub host: String,
    pub branch: String,
    pub now: String,
    pub force: bool,
}

/// A project's slug, all a rule needs of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Project {
    pub slug: String,
}

impl Kind {
    /// The word the key matrix stores for the kind.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Work => "work",
            Kind::Decision => "decision",
            Kind::Research => "research",
            Kind::Audit => "audit",
            Kind::Story => "story",
            Kind::Concept => "concept",
            Kind::Idea => "idea",
            Kind::Package => "package",
        }
    }

    /// The kind a stored word names.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "work" => Some(Kind::Work),
            "decision" => Some(Kind::Decision),
            "research" => Some(Kind::Research),
            "audit" => Some(Kind::Audit),
            "story" => Some(Kind::Story),
            "concept" => Some(Kind::Concept),
            "idea" => Some(Kind::Idea),
            "package" => Some(Kind::Package),
            _ => None,
        }
    }
}
