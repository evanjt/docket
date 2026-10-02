//! An item as the rules read it, and the changes a rule returns.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::word::Kind;

/// A verb that cannot apply. The message is the whole explanation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refused(pub String);

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

/// The stored columns of one item, tags decoded.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub rid: i64,
    pub project: String,
    pub key: String,
    pub num: i64,
    pub id: String,
    pub title: String,
    pub state: String,
    pub turn: Option<String>,
    pub turn_note: Option<String>,
    pub asked_at: Option<String>,
    pub claim_branch: Option<String>,
    pub claim_host: Option<String>,
    pub claim_since: Option<String>,
    pub claim_runner: Option<String>,
    pub claim_job: Option<String>,
    pub claim_on: Option<String>,
    pub wait_on: Option<String>,
    pub wait_item: Option<i64>,
    pub wait_ref: Option<String>,
    pub wait_since: Option<String>,
    pub decision: Option<String>,
    pub decided_at: Option<String>,
    pub resolution: Option<String>,
    pub superseded_by: Option<i64>,
    pub scope: Option<String>,
    pub complexity: Option<String>,
    pub group_name: Option<String>,
    pub theme: Option<String>,
    pub rank: Option<i64>,
    pub tags: Vec<String>,
    pub body: String,
    pub conflict: i64,
    pub opened_at: String,
    pub updated_at: String,
}

/// One column a verb sets. A verb returns the list of them, in the order it names them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Field {
    Title(String),
    State(String),
    Turn(Option<String>),
    TurnNote(Option<String>),
    AskedAt(Option<String>),
    ClaimBranch(Option<String>),
    ClaimHost(Option<String>),
    ClaimSince(Option<String>),
    ClaimRunner(Option<String>),
    ClaimJob(Option<String>),
    ClaimOn(Option<String>),
    WaitOn(Option<String>),
    WaitItem(Option<i64>),
    WaitRef(Option<String>),
    WaitSince(Option<String>),
    Decision(Option<String>),
    DecidedAt(Option<String>),
    Resolution(Option<String>),
    SupersededBy(Option<i64>),
    Scope(Option<String>),
    Complexity(Option<String>),
    GroupName(Option<String>),
    Theme(Option<String>),
    Rank(Option<i64>),
    Tags(Vec<String>),
    Body(String),
    Conflict(i64),
}

impl Item {
    /// The item with the changes applied, as the row reads after the update.
    pub fn apply(&mut self, changes: &[Field]) {
        for c in changes {
            match c.clone() {
                Field::Title(v) => self.title = v,
                Field::State(v) => self.state = v,
                Field::Turn(v) => self.turn = v,
                Field::TurnNote(v) => self.turn_note = v,
                Field::AskedAt(v) => self.asked_at = v,
                Field::ClaimBranch(v) => self.claim_branch = v,
                Field::ClaimHost(v) => self.claim_host = v,
                Field::ClaimSince(v) => self.claim_since = v,
                Field::ClaimRunner(v) => self.claim_runner = v,
                Field::ClaimJob(v) => self.claim_job = v,
                Field::ClaimOn(v) => self.claim_on = v,
                Field::WaitOn(v) => self.wait_on = v,
                Field::WaitItem(v) => self.wait_item = v,
                Field::WaitRef(v) => self.wait_ref = v,
                Field::WaitSince(v) => self.wait_since = v,
                Field::Decision(v) => self.decision = v,
                Field::DecidedAt(v) => self.decided_at = v,
                Field::Resolution(v) => self.resolution = v,
                Field::SupersededBy(v) => self.superseded_by = v,
                Field::Scope(v) => self.scope = v,
                Field::Complexity(v) => self.complexity = v,
                Field::GroupName(v) => self.group_name = v,
                Field::Theme(v) => self.theme = v,
                Field::Rank(v) => self.rank = v,
                Field::Tags(v) => self.tags = v,
                Field::Body(v) => self.body = v,
                Field::Conflict(v) => self.conflict = v,
            }
        }
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

/// One key of a project's matrix.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeySpec {
    pub key: String,
    pub kind: Kind,
    #[serde(default)]
    pub meaning: Option<String>,
    #[serde(default)]
    pub turn: Option<String>,
}

/// A project's slug and keys, all a rule needs of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Project {
    pub slug: String,
    pub keys: Vec<KeySpec>,
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
