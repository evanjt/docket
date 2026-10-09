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

pub use crate::metrics::Progress;

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
    pub complexity: Option<String>,
    /// The release it is in by name; none is the backlog.
    #[serde(default)]
    pub release: Option<String>,
    /// The area it is in by name.
    #[serde(default)]
    pub area: Option<String>,
    /// The labels it carries: its own, then those of each plan above it.
    #[serde(default)]
    pub labels: Vec<String>,
    pub body: String,
    /// Always 0: items no longer carry a sync conflict mark. Sent so a client built before reads the
    /// view.
    #[serde(default)]
    pub conflict: i64,
    pub opened_at: String,
    pub updated_at: String,
    pub group: Option<String>,
    /// The first of `repos`, which a client that reads one repository takes.
    #[serde(default)]
    pub repo: Option<String>,
    /// The repositories it changes, relative to the project root or `@OWNER/NAME` for another
    /// project's: its own `repo:` labels, else those of the nearest plan above it with any; none
    /// takes the `checkout` fact.
    #[serde(default)]
    pub repos: Vec<String>,
    pub word: String,
    pub priority: String,
    /// What the item is: task, bug, question, investigation or plan.
    #[serde(default, rename = "type")]
    pub item_type: String,
    pub superseded_by: Option<String>,
    pub related: Vec<String>,
    /// The plan the item belongs to.
    #[serde(default)]
    pub parent: Option<String>,
    /// What spawned the item.
    #[serde(default)]
    pub origin: Vec<String>,
    /// The items whose parent this is.
    #[serde(default)]
    pub children: Vec<String>,
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
    /// The release it is filed for: `current` or a release not shipped; none files it in the backlog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    /// The repositories it changes, comma-separated, each relative to the project root or `@OWNER/NAME`: the labels `repo:PATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The area it is filed in, by name; under a plan it is the plan's, and naming another is refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    /// The plan it is filed under; with no release it takes the plan's, and it may name an earlier
    /// one, never a later one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
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
    /// The area it is filed in, as `NewRequest::area`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    /// The plan it is filed under, as `NewRequest::parent`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
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
    /// The job's final report, its word first: `DONE`, `WAITING Q3` or `FAILED why`.
    #[serde(default)]
    pub report: Option<String>,
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
    /// The area the item reopens in; needed when its own area holds closed items only.
    #[serde(default)]
    pub area: Option<String>,
    /// The release it reopens in: current, a release not shipped, or "" for the backlog. Needed
    /// when its own release has shipped.
    #[serde(default)]
    pub release: Option<String>,
    /// Move too what the reopened item would leave out of order.
    #[serde(default)]
    pub carry: bool,
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
    /// The area an agent places the item in.
    #[serde(default)]
    pub area: Option<String>,
    /// What the placed area is about.
    #[serde(default)]
    pub about: Option<String>,
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
    /// The release it moves to, as `NewRequest::release`; empty moves it to the backlog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    /// Move with it every item the move would leave out of order: its dependants and plans when it
    /// goes later, its dependencies and children when it goes earlier.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub carry: bool,
    /// The area it and every item under it move to; refused on an item under a plan, whose area is
    /// the plan's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
}

/// `docket releases add|move|ship`: one release added, its open items moved, or it shipped.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReleasesRequest {
    #[serde(flatten)]
    pub common: Common,
    pub action: String,
    pub name: String,
    /// Where `move` and `ship` take the open items: `current`, a release, or empty for the backlog.
    #[serde(default)]
    pub to: Option<String>,
    /// `move` and `ship`: move too what the move would leave out of order, instead of refusing it.
    #[serde(default)]
    pub carry: bool,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// `docket areas add|edit|move|rm`: one area added, changed, moved to another place, or removed.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AreasRequest {
    #[serde(flatten)]
    pub common: Common,
    pub action: String,
    pub name: String,
    /// `edit`: the area's new name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename: Option<String>,
    /// `add` and `edit`: what the area holds; empty clears it on `edit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// `add` and `edit`: one of the four priority words; empty clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// `move`: the place it takes, the first being 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<i64>,
}

/// `docket label add|rm ID NAME`: a label given to an item, or taken from it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LabelRequest {
    #[serde(flatten)]
    pub common: Common,
    pub action: String,
    pub id: String,
    pub name: String,
    /// `add`: what the label means; replaces the description it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
}

/// The labels the item carries after the write.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LabelDone {
    pub id: String,
    pub labels: Vec<crate::label::Label>,
}

/// One label as `/labels` lists it, with how many items were given it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelRow {
    #[serde(flatten)]
    pub label: crate::label::Label,
    pub items: i64,
}

/// The areas after the write, in position order.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AreasDone {
    pub areas: Vec<crate::area::Area>,
}

/// The releases after the write, and the items it moved.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReleasesDone {
    pub releases: Vec<crate::release::Release>,
    pub moved: Vec<String>,
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

/// Each of `a` under the plan, or under no plan when `plan` is none.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ParentRequest {
    #[serde(flatten)]
    pub common: Common,
    pub a: Vec<String>,
    #[serde(default)]
    pub plan: Option<String>,
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
pub struct Parented {
    pub items: Vec<String>,
    /// The plan they are under now, none when they left one.
    pub plan: Option<String>,
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

/// What a squash wrote, recorded on the server: the published commit, the work snapshot whose tree it
/// carries, and the plans it covers by id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationRequest {
    #[serde(flatten)]
    pub common: Common,
    pub published: String,
    pub work: String,
    #[serde(default)]
    pub plans: Vec<String>,
}

/// A project's publications, newest first, the first being where the next squash starts:
/// `GET /publications` and the answer to a record.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Publications {
    pub project: String,
    pub publications: Vec<crate::publication::Publication>,
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

/// `remap`: done and dropped items' leading shas carried through an old-to-new commit map.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RemapRequest {
    #[serde(flatten)]
    pub common: Common,
    /// `[old, new]` full shas.
    pub map: Vec<(String, String)>,
    #[serde(default)]
    pub dry_run: bool,
}

/// One resolution the map rewrites.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemapRow {
    pub id: String,
    pub old: String,
    pub new: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Remapped {
    pub dry_run: bool,
    pub rows: Vec<RemapRow>,
}

/// `projects rename`: the project in `common` takes the slug `new`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRenameRequest {
    #[serde(flatten)]
    pub common: Common,
    pub new: String,
    #[serde(default)]
    pub dry_run: bool,
}

/// A label of some project that named the renamed one as `repo:@OLD[/PATH]`, renamed with it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenamedLabel {
    pub project: String,
    pub old: String,
    pub new: String,
}

/// What a rename moved: the rows of each table that now carry the new slug, and the labels renamed.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProjectRenamed {
    pub dry_run: bool,
    pub old: String,
    pub new: String,
    pub moved: BTreeMap<String, u64>,
    pub labels: Vec<RenamedLabel>,
}
