//! The rows the server's routes answer with, typed for every client that reads them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::assignment::{Held, HeldFields};
use crate::word::Kind;

/// One key of a project's key list before the drop: what it holds and the turn a new item started on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeySpec {
    pub key: String,
    pub kind: Kind,
    #[serde(default)]
    pub meaning: Option<String>,
    #[serde(default)]
    pub turn: Option<String>,
}

/// One of a project's themes before the drop, with its note.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// A project as `/projects` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRow {
    pub slug: String,
    #[serde(default)]
    pub skills: BTreeMap<String, String>,
    #[serde(default)]
    pub updated_at: String,
    /// The releases, filled from `/releases` by a reader that orders by them.
    #[serde(default, skip_serializing)]
    pub releases: crate::release::Listed,
    /// The areas, filled from `/areas` by a reader that names or counts by them.
    #[serde(default, skip_serializing)]
    pub areas: crate::area::Listed,
}

/// An item as stored, as `/items` lists it: no body, rids rather than ids. Its claim and turn are
/// filled from its open assignment, which `/items` does not carry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRow {
    pub rid: i64,
    pub project: String,
    pub key: String,
    pub num: i64,
    pub id: String,
    pub title: String,
    pub state: String,
    #[serde(default)]
    pub turn: Option<String>,
    #[serde(default)]
    pub turn_note: Option<String>,
    #[serde(default)]
    pub claim_branch: Option<String>,
    #[serde(default)]
    pub claim_host: Option<String>,
    #[serde(default)]
    pub claim_since: Option<String>,
    #[serde(default)]
    pub claim_job: Option<String>,
    #[serde(default)]
    pub claim_on: Option<String>,
    /// What its dependencies hold it on, `item` or `condition`: `/items` sends none, so a reader fills
    /// it from `/waiting`.
    #[serde(default)]
    pub wait_on: Option<String>,
    /// The id of the item that holds it, filled with `wait_on`.
    #[serde(default)]
    pub wait_ref: Option<String>,
    #[serde(default)]
    pub parent_rid: Option<i64>,
    #[serde(default)]
    pub decision: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub release_id: Option<i64>,
    #[serde(default)]
    pub area_id: Option<i64>,
    /// What the item is: task, bug, question, investigation or plan.
    #[serde(default, rename = "type")]
    pub item_type: String,
    #[serde(default)]
    pub opened_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// An item's open assignment, as `/held` lists a project's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldRow {
    pub rid: i64,
    pub held: Held,
}

impl ItemRow {
    /// The row's claim and turn set from the item's open assignment.
    pub fn hold(&mut self, held: Option<&Held>) {
        let f = HeldFields::of(&self.state, held);
        self.turn = f.turn;
        self.turn_note = f.turn_note;
        self.claim_branch = f.claim_branch;
        self.claim_host = f.claim_host;
        self.claim_since = f.claim_since;
        self.claim_job = f.claim_job;
        self.claim_on = f.claim_on;
    }
}

/// An item as the list routes and `/show` print it: ids for rids, with its word and priority.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub id: String,
    pub key: String,
    pub num: i64,
    pub project: String,
    pub title: String,
    pub state: String,
    pub word: String,
    pub priority: String,
    /// What the item is: task, bug, question, investigation or plan.
    #[serde(default, rename = "type")]
    pub item_type: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub superseded_by: Option<String>,
    #[serde(default)]
    pub turn: Option<String>,
    #[serde(default)]
    pub turn_note: Option<String>,
    #[serde(default)]
    pub asked_at: Option<String>,
    #[serde(default)]
    pub claim_branch: Option<String>,
    #[serde(default)]
    pub claim_host: Option<String>,
    #[serde(default)]
    pub claim_since: Option<String>,
    #[serde(default)]
    pub claim_runner: Option<String>,
    #[serde(default)]
    pub claim_job: Option<String>,
    #[serde(default)]
    pub claim_on: Option<String>,
    #[serde(default)]
    pub wait_on: Option<String>,
    #[serde(default)]
    pub wait_ref: Option<String>,
    #[serde(default)]
    pub wait_since: Option<String>,
    #[serde(default)]
    pub decision: Option<String>,
    #[serde(default)]
    pub decided_at: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub complexity: Option<String>,
    /// The release it is in; none is the backlog.
    #[serde(default)]
    pub release: Option<String>,
    /// The area it is in.
    #[serde(default)]
    pub area: Option<String>,
    /// The labels it carries: its own, then those of each plan above it.
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub opened_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// The search routes only: the matched words in brackets.
    #[serde(default)]
    pub snip: Option<String>,
}

/// A path an item cites, with its line when it names one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cite {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub line: Option<i64>,
    pub kind: String,
}

pub use crate::metrics::Progress;

/// One item as `/show` prints it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shown {
    #[serde(flatten)]
    pub row: Row,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub origin: Vec<String>,
    #[serde(default)]
    pub children: Vec<String>,
    #[serde(default)]
    pub cites: Vec<Cite>,
    #[serde(default)]
    pub progress: Option<Progress>,
}

/// The flow counts `/status` answers with.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub project: String,
    pub host: String,
    pub total: u64,
    pub by_word: BTreeMap<String, u64>,
    pub by_key: BTreeMap<String, BTreeMap<String, u64>>,
}

impl Status {
    #[must_use]
    pub fn count(&self, word: &str) -> u64 {
        self.by_word.get(word).copied().unwrap_or(0)
    }

    /// Tickets still open: every word but done and dropped.
    #[must_use]
    pub fn open(&self) -> u64 {
        self.by_word
            .iter()
            .filter(|(w, _)| !matches!(w.as_str(), "done" | "dropped"))
            .map(|(_, n)| n)
            .sum()
    }
}

/// One move recorded on an item or a project, as `/events` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRow {
    pub seq: i64,
    pub project: String,
    #[serde(default)]
    pub rid: Option<i64>,
    pub at: String,
    pub host: String,
    #[serde(default)]
    pub branch: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// A tie from an item to another item or to a path, as `/links` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRow {
    pub rid: i64,
    pub kind: String,
    #[serde(default)]
    pub to_rid: Option<i64>,
    #[serde(default)]
    pub to_path: Option<String>,
}

/// A decision an agent derived, as `/derived` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Derived {
    pub id: String,
    pub state: String,
    pub at: String,
    pub title: String,
    #[serde(default)]
    pub chose: Option<String>,
    #[serde(default)]
    pub basis: String,
}

/// A refusal: the body of every 4xx and 5xx.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refusal {
    pub error: String,
}
