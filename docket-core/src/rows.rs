//! The rows the server's routes answer with, typed for every client that reads them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::word::Kind;

/// One key of a project: what it holds and the turn a new item starts on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeySpec {
    pub key: String,
    pub kind: Kind,
    #[serde(default)]
    pub meaning: Option<String>,
    #[serde(default)]
    pub turn: Option<String>,
}

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
    pub keys: Vec<KeySpec>,
    #[serde(default)]
    pub themes: Vec<Theme>,
    #[serde(default)]
    pub skills: BTreeMap<String, String>,
    #[serde(default)]
    pub updated_at: String,
}

impl ProjectRow {
    /// What a key holds, work when the project does not name it.
    #[must_use]
    pub fn kind(&self, key: &str) -> Kind {
        self.keys
            .iter()
            .find(|k| k.key == key)
            .map_or(Kind::Work, |k| k.kind)
    }

    /// The keys holding one kind, in the project's order.
    #[must_use]
    pub fn keys_of(&self, kind: Kind) -> Vec<String> {
        self.keys
            .iter()
            .filter(|k| k.kind == kind)
            .map(|k| k.key.clone())
            .collect()
    }
}

/// An item as stored, as `/items` lists it: no body, rids rather than ids.
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
    #[serde(default)]
    pub wait_on: Option<String>,
    #[serde(default)]
    pub wait_item: Option<i64>,
    #[serde(default)]
    pub wait_ref: Option<String>,
    #[serde(default)]
    pub decision: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub group_name: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub rank: Option<i64>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub opened_at: String,
    #[serde(default)]
    pub updated_at: String,
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
    pub claim_job: Option<String>,
    #[serde(default)]
    pub claim_on: Option<String>,
    #[serde(default)]
    pub wait_on: Option<String>,
    #[serde(default)]
    pub wait_ref: Option<String>,
    #[serde(default)]
    pub decision: Option<String>,
    #[serde(default)]
    pub decided_at: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub complexity: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub rank: Option<i64>,
    #[serde(default)]
    pub tags: Vec<String>,
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

/// How far a package is: members done of all, and how many are claimed now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
    pub live: u64,
}

/// One item as `/show` prints it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shown {
    #[serde(flatten)]
    pub row: Row,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub opened: Vec<String>,
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

    /// Tickets still open: every word but done, dropped and standing.
    #[must_use]
    pub fn open(&self) -> u64 {
        self.by_word
            .iter()
            .filter(|(w, _)| !matches!(w.as_str(), "done" | "dropped" | "standing"))
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
