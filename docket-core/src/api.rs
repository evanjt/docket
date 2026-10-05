//! The request and response of every write verb, shared by the server and its clients.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What every verb's request carries besides its own arguments.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Common {
    pub project: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub force: bool,
}

/// A refusal, as the body of a 409.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refusal {
    pub refused: String,
}

/// A path an item cites.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cite {
    pub path: String,
    pub line: Option<i64>,
    pub kind: String,
}

/// A package's members: closed or dropped, all of them, and those claimed now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub live: usize,
}

/// One item in the shape `docket show --json` prints: ids instead of rids, the derived word and tier.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemView {
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
    pub wait_ref: Option<String>,
    pub wait_since: Option<String>,
    pub decision: Option<String>,
    pub decided_at: Option<String>,
    pub resolution: Option<String>,
    pub scope: Option<String>,
    pub complexity: Option<String>,
    pub theme: Option<String>,
    pub rank: Option<i64>,
    pub tags: Vec<String>,
    pub body: String,
    pub conflict: i64,
    pub opened_at: String,
    pub updated_at: String,
    pub group: Option<String>,
    pub word: String,
    pub priority: String,
    pub superseded_by: Option<String>,
    pub related: Vec<String>,
    pub opened: Vec<String>,
    pub cites: Vec<Cite>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
}

/// An item named in passing: a released waiter, a plan come due.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Brief {
    pub id: String,
    pub title: String,
}

/// A decided question close to a new one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decided {
    pub id: String,
    pub title: String,
    pub decision: String,
}

/// A done item close to one being asked about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Twin {
    pub id: String,
    pub title: String,
    pub resolution: String,
}

/// Files a claim shares with another live claim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    pub holder: String,
    pub branch: String,
    pub host: String,
    pub paths: Vec<String>,
}

// ---- requests ----

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NewRequest {
    #[serde(flatten)]
    pub common: Common,
    pub key: String,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub turn: Option<String>,
    #[serde(default)]
    pub complexity: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
    /// The release it is filed for: `current`, a listed release, or a theme in use; sets the theme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AddRequest {
    #[serde(flatten)]
    pub common: Common,
    pub title: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    /// The fleet job that saw it, so the brakes count it.
    #[serde(default)]
    pub from: Option<String>,
    /// The release it is filed for, as `NewRequest::release`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StartRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    #[serde(default)]
    pub runner: Option<String>,
    #[serde(default)]
    pub job: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub on: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReleaseRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub bounce: bool,
    #[serde(default)]
    pub rebase: Option<String>,
    #[serde(default)]
    pub runner: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// How the attempt ended: landed, conflict, gate, blocked or failed.
    #[serde(default)]
    pub outcome: Option<String>,
}

/// What a finished job's own records say, posted to the open claim of its item.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct JobReportRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub exit: Option<i32>,
    #[serde(default)]
    pub tokens_in: Option<i64>,
    #[serde(default)]
    pub tokens_out: Option<i64>,
    #[serde(default)]
    pub cost_reported: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CloseRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub gates: Option<String>,
    #[serde(default)]
    pub runner: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DropRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    #[serde(default)]
    pub why: Option<String>,
    #[serde(default)]
    pub superseded_by: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReopenRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    pub why: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WaitRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    #[serde(default)]
    pub on: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
}

/// `dep add` and `dep rm`: the items `id` depends on, added or removed.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DepRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    pub on: Vec<String>,
    #[serde(default)]
    pub remove: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResumeRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// `ask` and its alias `park`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AskRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    pub note: String,
    /// What the ask needs of the owner: one of `queue::NEEDS`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub need: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReplyRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    pub note: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AnswerRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    pub decision: String,
    #[serde(default)]
    pub derived: Option<String>,
    /// The items that carry the decision out, already filed: the question closes on them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub carried_by: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DecideRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    pub choice: String,
    pub basis: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PriorityRequest {
    #[serde(flatten)]
    pub common: Common,
    pub ids: Vec<String>,
    pub tier: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RateRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    pub level: String,
}

/// One `--set FIELD=VALUE`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetField {
    pub field: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EditRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    #[serde(default)]
    pub set: Vec<SetField>,
    #[serde(default)]
    pub append: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    /// The `updated_at` the caller last read; a whole-body replace against a newer row is refused.
    #[serde(default)]
    pub expect_updated_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LinkRequest {
    #[serde(flatten)]
    pub common: Common,
    pub a: Vec<String>,
    pub kind: String,
    pub b: String,
    #[serde(default)]
    pub remove: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct KeyRequest {
    #[serde(flatten)]
    pub common: Common,
    pub key: String,
    pub kind: String,
    pub meaning: String,
    #[serde(default)]
    pub turn: Option<String>,
}

/// `retry`: an item the loop parked or sent back, given a fresh start.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RetryRequest {
    #[serde(flatten)]
    pub common: Common,
    pub id: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// `fact`: one project fact set, or unset by an empty value.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FactRequest {
    #[serde(flatten)]
    pub common: Common,
    pub key: String,
    #[serde(default)]
    pub value: String,
    /// Set the owner-level value of an agent setting, read by every project, not the project's.
    #[serde(default)]
    pub all_projects: bool,
}

/// The checkout a client stands in, for the project it belongs to.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProjectRequest {
    /// The slug the first origin URL names, if any does.
    #[serde(default)]
    pub candidate: Option<String>,
    /// The origin URLs met from the checkout up to the outermost repository.
    #[serde(default)]
    pub remotes: Vec<String>,
    /// The outermost repository's directory name.
    pub basename: String,
    /// Create the project when nothing matches.
    #[serde(default)]
    pub create: bool,
}

// ---- responses ----

/// `new` and `add`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Opened {
    pub item: ItemView,
    pub kind: String,
    /// Decided questions close to a new question, the prior decisions an agent derives from.
    pub decided_like: Vec<Decided>,
}

/// One item moved: `release`, `reopen`, `wait`, `resume`, `reply`, `decide`, `rate`, `edit`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Moved {
    pub item: ItemView,
}

/// Several items moved: `priority`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovedMany {
    pub items: Vec<ItemView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Started {
    pub item: ItemView,
    pub kind: String,
    pub shares: Vec<Share>,
    pub worktree_hint: Option<String>,
    /// The other open items of its group.
    pub group_others: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Closed {
    pub item: ItemView,
    pub released: Vec<Brief>,
    /// The ids a decision's resolution says it opened, now linked.
    pub opened: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dropped {
    pub item: ItemView,
    pub released: Vec<Brief>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asked {
    pub item: ItemView,
    /// Done items close to this one, read before raising it.
    pub twins: Vec<Twin>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answered {
    pub item: ItemView,
    pub released: Vec<Brief>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Linked {
    pub items: Vec<String>,
    pub kind: String,
    pub to: String,
    pub removed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeySet {
    pub key: String,
    pub kind: String,
    pub meaning: String,
    pub turn: String,
}

/// The project a checkout matched or created; `how` is matched, created, ambiguous or none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectResolved {
    pub slug: Option<String>,
    pub how: String,
    /// The projects it could be, when it could be several.
    pub matches: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reindexed {
    pub project: String,
    pub items: usize,
}

/// The facts a project sets that docket reads; a retired fact still stored is left out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    pub project: String,
    pub skills: BTreeMap<String, String>,
    /// The owner-level agent settings, held on the server for every project.
    #[serde(default)]
    pub owner: BTreeMap<String, String>,
    /// The priority tiers, most urgent first.
    #[serde(default)]
    pub priorities: Vec<String>,
    /// The complexity levels, highest first.
    #[serde(default)]
    pub levels: Vec<String>,
}

/// One fact written: the facts as they stand after it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactSet {
    pub project: String,
    pub key: String,
    pub skills: BTreeMap<String, String>,
    #[serde(default)]
    pub owner: BTreeMap<String, String>,
}

/// `docket machine set` and `remove`: the fields given, or the machine removed. The owner's key only.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineRequest {
    #[serde(flatten)]
    pub set: crate::machine::Set,
    #[serde(default)]
    pub remove: bool,
}

/// Every machine, by name: `GET /machines` and the answer to a machine write.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machines {
    pub machines: Vec<crate::machine::Machine>,
}

/// `docket lead take`, `renew` or `give`: the act, and the session naming the holder on its host.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadRequest {
    #[serde(flatten)]
    pub common: Common,
    pub act: String,
    pub session: String,
}

/// A project's lead claim as it stands: `GET /lead`, and the answer to an act with what it did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadState {
    pub project: String,
    pub lead: Option<crate::lead::Lead>,
    /// The claim went unrenewed for the lapse, and the next take takes it over.
    pub lapsed: bool,
    pub lapses_at: Option<String>,
    pub lapse_minutes: i64,
    /// took, took over, renewed or gave; absent on a read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// The claim a takeover replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<crate::lead::Lead>,
}

/// A stored JSON column, passed through as it is.
pub type Json = Value;
