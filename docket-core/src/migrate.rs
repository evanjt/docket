//! The move of a project's rows onto the core: releases as rows, dependencies, one parent plan, an
//! origin link, areas, labels, a priority column and assignment rows. `plan` is the one place the mapping
//! is written; it reads the rows as they stood before the drop (`OldItem`, `OldProject`) and returns every change with the cases that need
//! a person's eye. A case whose repair turns on an undecided rule waits on it, and nothing is
//! written while one does.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::area::Placement;
use crate::assignment;
use crate::dump::{DumpPage, EventDump, ItemDump};
use crate::item::Refused;
use crate::rows::{KeySpec, Theme};
use crate::word::{Kind, PRIORITIES, priority};

/// One project's rows: items with their links by id, and events.
pub struct Rows<'a> {
    pub project: &'a OldProject,
    pub items: Vec<&'a OldItem>,
    pub events: Vec<&'a EventDump>,
}

impl<'a> Rows<'a> {
    /// The rows of each project a page carries, in the page's order.
    #[must_use]
    pub fn of(page: &'a OldPage) -> Vec<Rows<'a>> {
        page.projects
            .iter()
            .map(|project| Rows {
                project,
                items: page
                    .items
                    .iter()
                    .filter(|i| i.project == project.slug)
                    .collect(),
                events: page
                    .events
                    .iter()
                    .filter(|e| e.project == project.slug)
                    .collect(),
            })
            .collect()
    }
}

/// One item as its row stood before the drop of the columns the core replaced, with ids for every
/// reference: the shape the earlier migrations build and the plan reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OldItem {
    pub project: String,
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
    pub superseded_by: Option<String>,
    pub scope: Option<String>,
    pub complexity: Option<String>,
    pub group: Option<String>,
    pub theme: Option<String>,
    /// The release it is in by name; none is the backlog.
    pub release: Option<String>,
    /// The area it is in by name.
    pub area: Option<String>,
    pub rank: Option<i64>,
    /// What the item is; a file written before the column carries none, and a restore reads it from
    /// the key.
    #[serde(rename = "type")]
    pub item_type: String,
    /// A file written before the column carries none, and a restore reads it from the tags.
    pub priority: String,
    pub tags: Vec<String>,
    pub related: Vec<String>,
    /// The plan the item belongs to.
    pub parent: Option<String>,
    /// What spawned the item; none when nothing is recorded.
    pub origin: Option<Vec<String>>,
    /// The `opened` links an item file carried before parents and origins, read so an older dump
    /// restores; never written.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub opened: Vec<String>,
    /// What the item depends on; none when it depends on nothing.
    pub depends: Option<Vec<String>>,
    /// The labels it was given, never those it inherits; none when it was given none.
    pub labels: Option<Vec<String>>,
    pub opened_at: String,
    pub updated_at: String,
    pub body: String,
}

/// A project row as it stood before the drop of its keys and themes, its JSON columns decoded.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OldProject {
    pub slug: String,
    pub keys: Value,
    pub remotes: Value,
    pub themes: Value,
    pub cite_roots: Value,
    pub repos: Value,
    pub fleet_repo: Option<String>,
    pub integration_ref: Option<String>,
    pub worktree_hint: Option<String>,
    pub test_hint: Option<String>,
    pub skills: Value,
    pub created_at: String,
    pub updated_at: String,
    /// Every release, shipped or not, in position order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub releases: Vec<crate::release::Release>,
    /// Every area, in position order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<crate::area::Area>,
    /// Every label, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<crate::label::Label>,
    /// Every publication, newest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publications: Vec<crate::publication::Publication>,
}

/// Every project, item and event in the shape the plan reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OldPage {
    pub projects: Vec<OldProject>,
    pub items: Vec<OldItem>,
    pub events: Vec<EventDump>,
}

impl OldPage {
    /// A dump page read in the shape the plan reads: each item's claim and turn from its open
    /// assignment, and each project's keys from the types of the items filed under them. What the
    /// drop removed reads as unset.
    #[must_use]
    pub fn of(page: &DumpPage) -> Self {
        let projects = page
            .projects
            .iter()
            .map(|p| {
                let mut keys: BTreeMap<String, Kind> = BTreeMap::new();
                for i in page.items.iter().filter(|i| i.project == p.slug) {
                    keys.entry(key_num(&i.id).0.to_string())
                        .or_insert_with(|| crate::word::kind_of_type(&i.item_type));
                }
                let keys: Vec<KeySpec> = keys
                    .into_iter()
                    .map(|(key, kind)| KeySpec {
                        key,
                        kind,
                        meaning: None,
                        turn: None,
                    })
                    .collect();
                OldProject {
                    slug: p.slug.clone(),
                    keys: serde_json::to_value(keys).unwrap_or_default(),
                    remotes: p.remotes.clone(),
                    themes: serde_json::Value::Array(Vec::new()),
                    cite_roots: p.cite_roots.clone(),
                    repos: p.repos.clone(),
                    fleet_repo: p.fleet_repo.clone(),
                    integration_ref: p.integration_ref.clone(),
                    worktree_hint: p.worktree_hint.clone(),
                    test_hint: p.test_hint.clone(),
                    skills: p.skills.clone(),
                    created_at: p.created_at.clone(),
                    updated_at: p.updated_at.clone(),
                    releases: p.releases.clone(),
                    areas: p.areas.clone(),
                    labels: p.labels.clone(),
                    publications: p.publications.clone(),
                }
            })
            .collect();
        OldPage {
            projects,
            items: page.items.iter().map(OldItem::of).collect(),
            events: page.events.clone(),
        }
    }
}

impl OldItem {
    /// A dumped item in the old shape, its claim and turn read from its open assignment.
    #[must_use]
    pub fn of(i: &ItemDump) -> Self {
        let held = i
            .assignments
            .iter()
            .flatten()
            .find(|a| a.ended_at.is_none())
            .map(|a| assignment::Held::of(a.kind, a));
        let h = assignment::HeldFields::of(&i.state, held.as_ref());
        OldItem {
            project: i.project.clone(),
            id: i.id.clone(),
            title: i.title.clone(),
            state: i.state.clone(),
            turn: h.turn,
            turn_note: h.turn_note,
            asked_at: h.asked_at,
            claim_branch: h.claim_branch,
            claim_host: h.claim_host,
            claim_since: h.claim_since,
            claim_runner: h.claim_runner,
            claim_job: h.claim_job,
            claim_on: h.claim_on,
            decision: i.decision.clone(),
            decided_at: i.decided_at.clone(),
            resolution: i.resolution.clone(),
            superseded_by: i.superseded_by.clone(),
            complexity: i.complexity.clone(),
            release: i.release.clone(),
            area: i.area.clone(),
            item_type: i.item_type.clone(),
            priority: i.priority.clone(),
            related: i.related.clone(),
            parent: i.parent.clone(),
            origin: i.origin.clone(),
            depends: i
                .depends
                .as_ref()
                .map(|d| d.iter().map(|d| d.on.clone()).collect()),
            labels: i.labels.clone(),
            opened_at: i.opened_at.clone(),
            updated_at: i.updated_at.clone(),
            body: i.body.clone(),
            ..OldItem::default()
        }
    }
}

/// Where an item goes whose theme is not a release. It carries the theme as a label either way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Themes {
    Backlog,
    /// Its parent plan's release when that plan is in one, else the backlog.
    #[default]
    Plan,
}

/// The rules the cases turn on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Rules {
    pub themes: Themes,
}

/// A decision a risky case waits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Decision {
    Themes,
    Unplaced,
}

impl Decision {
    #[must_use]
    pub fn what(self) -> &'static str {
        match self {
            Decision::Themes => "where an item goes whose theme is not a release",
            Decision::Unplaced => "an agent's placement (docket decide ID --area NAME)",
        }
    }
}

/// A release row: the releases fact in order, then each theme named as a version, in version order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Release {
    pub name: String,
    pub position: usize,
    pub from_fact: bool,
}

/// An area row the move makes, after the project's areas: one from each concept item in concept id
/// order, its title and body the description; then one for each area an agent's placement names, in
/// the order first placed, its `about` the description; then `unsorted`, for the closed items nothing
/// places, marked `history`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Area {
    pub name: String,
    pub description: String,
    pub position: usize,
    /// The concept item it is made from; none for an area a placement names, and for `unsorted`.
    pub concept: Option<String>,
    /// The area holds closed items only, and no open item is placed in it.
    pub history: bool,
}

/// The area the closed items nothing places go to, and what it says it holds.
pub const UNSORTED: &str = "unsorted";
pub const UNSORTED_ABOUT: &str = "closed items no area claimed at the migration";

/// How the items of one project came by their areas, for the dry run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Placed {
    pub by_plan: usize,
    pub by_concept: usize,
    pub by_placement: usize,
    /// Closed items nothing places, sent to `unsorted`.
    pub unsorted: usize,
    /// Open items nothing places, left with no area.
    pub unplaced: usize,
}

/// One attempt at an item, a claim or an ask, as `assignment::rebuild` gives it. No end is the
/// attempt held now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Assignment {
    pub id: String,
    pub kind: &'static str,
    pub branch: String,
    pub host: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub outcome: Option<&'static str>,
}

/// One change to one item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum Change {
    /// The item's release, `None` the backlog.
    InRelease {
        id: String,
        release: Option<String>,
    },
    /// The item's type becomes plan.
    Plan {
        id: String,
    },
    /// The item's type becomes `kind`, for an item under a key the project made.
    Typed {
        id: String,
        kind: String,
    },
    Priority {
        id: String,
        priority: String,
    },
    Label {
        id: String,
        label: String,
    },
    /// A standing item turned into the label its related items carry.
    BecomesLabel {
        id: String,
        label: String,
    },
    /// A concept item dropped, the area it became standing in for it.
    BecomesArea {
        id: String,
        area: String,
    },
    InArea {
        id: String,
        area: String,
    },
    /// The area an agent placed an item in with `decide --area`, applied as written, and the basis.
    Placed {
        id: String,
        area: String,
        basis: String,
    },
    DependsOn {
        id: String,
        on: String,
    },
    /// The wait on everything a plan opened goes: its children hold it.
    GateRemoved {
        id: String,
    },
    /// A wait on a condition becomes a task on the owner, which the item depends on.
    OwnerTask {
        id: String,
        condition: String,
    },
    Parent {
        id: String,
        parent: String,
    },
    Origin {
        id: String,
        from: String,
    },
    Related {
        id: String,
        to: String,
        why: String,
    },
    /// The owner's turn becomes the owner as assignee.
    Assignee {
        id: String,
    },
    Assignment(Assignment),
}

impl Change {
    #[must_use]
    pub fn line(&self) -> String {
        match self {
            Change::InRelease { id, release } => match release {
                Some(r) => format!("{id} in release {r}"),
                None => format!("{id} in the backlog"),
            },
            Change::Plan { id } => format!("{id} is a plan"),
            Change::Typed { id, kind } => format!("{id} is a {kind}"),
            Change::Priority { id, priority } => format!("{id} at priority {priority}"),
            Change::Label { id, label } => format!("{id} labelled {label}"),
            Change::BecomesLabel { id, label } => format!("{id} becomes the label {label}"),
            Change::BecomesArea { id, area } => format!("{id} is dropped: became area {area}"),
            Change::InArea { id, area } => format!("{id} in area {area}"),
            Change::Placed { id, area, basis } => {
                format!("{id} placed in {area} (derived: {basis})")
            }
            Change::DependsOn { id, on } => format!("{id} depends on {on}"),
            Change::GateRemoved { id } => {
                format!("{id} stops waiting on its gate: its children hold it")
            }
            Change::OwnerTask { id, condition } => {
                format!("{id} depends on a new task for the owner: {condition}")
            }
            Change::Parent { id, parent } => format!("{id} under plan {parent}"),
            Change::Origin { id, from } => format!("{id} came from {from}"),
            Change::Related { id, to, why } => format!("{id} related to {to}: {why}"),
            Change::Assignee { id } => format!("{id} assigned to the owner"),
            Change::Assignment(a) => {
                let end = match (&a.ended_at, a.outcome) {
                    (None, _) => "held now".to_string(),
                    (Some(at), Some(o)) => format!("to {at}, {o}"),
                    (Some(at), None) => format!("to {at}, outcome unknown"),
                };
                if a.kind == assignment::Kind::Ask.as_str() {
                    format!("{} with the owner from {} {end}", a.id, a.started_at)
                } else {
                    format!(
                        "{} attempt on {} at {} from {} {end}",
                        a.id, a.branch, a.host, a.started_at
                    )
                }
            }
        }
    }
}

/// A case that needs a person's eye before the move is written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "case", rename_all = "snake_case")]
pub enum Case {
    /// An open plan with open children in later releases, by name.
    HeldPlan {
        plan: String,
        release: String,
        later: Vec<(String, String)>,
    },
    /// The hold of `id` on `on` would close a cycle; `path` runs from `on` back to `id`.
    Cycle {
        id: String,
        on: String,
        path: Vec<String>,
    },
    /// An item reachable from more than one plan.
    TwoPlans {
        id: String,
        kept: String,
        others: Vec<String>,
    },
    /// An item depending on one in a later release.
    Inversion {
        id: String,
        release: String,
        on: String,
        on_release: String,
    },
    /// A theme that is not a release, and the open items carrying it.
    FoldedTheme { theme: String, open: usize },
    /// A key the project made: its items keep their ids and take a type and a label for the key.
    OwnKey {
        key: String,
        meaning: String,
        items: usize,
        open: usize,
    },
    /// The project names no release, so every item goes to the backlog.
    NoRelease { open: usize },
    /// The item files docket-dump writes change shape.
    DumpFormat { files: usize },
    /// Names in the releases fact that are no semantic version.
    ReleaseName { names: Vec<String> },
    /// The releases fact is not in strictly rising version order.
    ReleaseOrder { names: Vec<String> },
    /// An item tied to concepts other than the area its plan gives it.
    PlanArea {
        id: String,
        plan: String,
        area: String,
        own: Vec<String>,
    },
    /// An item under no plan with an area, tied to several concepts: it keeps the oldest tie.
    SeveralConcepts {
        id: String,
        area: String,
        gave_up: Vec<String>,
    },
    /// The open items no plan, concept or placement places.
    Unplaced { open: Vec<String> },
    /// An agent's placement of an item a plan or a concept places already.
    PlacementIgnored {
        id: String,
        placed: String,
        area: String,
    },
}

impl Case {
    fn text(&self) -> String {
        match self {
            Case::HeldPlan {
                plan,
                release,
                later,
            } => {
                let later: Vec<String> = later.iter().map(|(i, r)| format!("{i} in {r}")).collect();
                format!(
                    "plan {plan} in {release} is held by later work: {}",
                    later.join(", ")
                )
            }
            Case::Cycle { id, on, path } => {
                format!(
                    "{id} held by {on} closes a cycle: {} -> {on}",
                    path.join(" -> ")
                )
            }
            Case::TwoPlans { id, kept, others } => {
                format!("{id} is under plans {kept}, {}", others.join(", "))
            }
            Case::Inversion {
                id,
                release,
                on,
                on_release,
            } => format!("{id} in {release} depends on {on} in {on_release}"),
            Case::FoldedTheme { theme, open } => {
                format!("theme {theme} is not a release, on {open} open items")
            }
            Case::OwnKey {
                key,
                meaning,
                items,
                open,
            } => format!("key {key} ({meaning}) holds {items} items, {open} open"),
            Case::NoRelease { open } => {
                format!("the project names no release, with {open} open items")
            }
            Case::DumpFormat { files } => {
                format!("docket-dump's item files change shape, {files} files")
            }
            Case::ReleaseName { names } => {
                format!(
                    "the releases fact names no semantic version: {}",
                    names.join(", ")
                )
            }
            Case::ReleaseOrder { names } => {
                format!(
                    "the releases fact is out of version order: {}",
                    names.join(", ")
                )
            }
            Case::PlanArea {
                id,
                plan,
                area,
                own,
            } => format!(
                "{id} is tied to {} and under plan {plan} in {area}",
                own.join(", ")
            ),
            Case::SeveralConcepts { id, area, gave_up } => format!(
                "{id} is tied to {area} and {} with no plan to settle it",
                gave_up.join(", ")
            ),
            Case::Unplaced { open } => format!(
                "{} open items have no area from a plan, a concept or a placement",
                open.len()
            ),
            Case::PlacementIgnored { id, placed, area } => format!(
                "{id} is placed in {placed} by an agent and in {area} by its plan or concept"
            ),
        }
    }

    /// Whether the case stops the write until the data is corrected.
    fn blocks(&self) -> bool {
        matches!(self, Case::ReleaseName { .. } | Case::ReleaseOrder { .. })
    }

    /// What a waiting case is named by in a refusal.
    fn subject(&self) -> String {
        match self {
            Case::HeldPlan { plan, .. } => plan.clone(),
            Case::FoldedTheme { theme, .. } => theme.clone(),
            Case::Cycle { id, .. }
            | Case::TwoPlans { id, .. }
            | Case::Inversion { id, .. }
            | Case::PlanArea { id, .. }
            | Case::SeveralConcepts { id, .. }
            | Case::PlacementIgnored { id, .. } => id.clone(),
            Case::OwnKey { key, .. } => key.clone(),
            Case::Unplaced { open } => open.join(", "),
            Case::NoRelease { .. }
            | Case::DumpFormat { .. }
            | Case::ReleaseName { .. }
            | Case::ReleaseOrder { .. } => String::new(),
        }
    }
}

/// A risky case, what the move does about it, and the decision it waits on, if any.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Risky {
    pub case: Case,
    pub action: String,
    pub waits: Option<Decision>,
}

impl Risky {
    #[must_use]
    pub fn line(&self) -> String {
        match self.waits {
            Some(d) => format!(
                "{}: {} (waits on {})",
                self.case.text(),
                self.action,
                d.what()
            ),
            None => format!("{}: {}", self.case.text(), self.action),
        }
    }
}

/// What the move changes in one project.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Changes {
    pub project: String,
    pub releases: Vec<Release>,
    /// The areas the move makes from concept items, placements and the closed items left over.
    pub areas: Vec<Area>,
    pub placed: Placed,
    /// Every label the move makes, with its description when the rows give one.
    pub labels: BTreeMap<String, String>,
    /// Every change, item by item in key and number order.
    pub changes: Vec<Change>,
    pub risky: Vec<Risky>,
    /// The open items a repair moves into the current release, which grows by their count.
    pub grows: Vec<String>,
}

impl Changes {
    /// # Errors
    /// Refused while a risky case waits on a decision, naming each decision and its cases.
    pub fn writable(&self) -> Result<(), Refused> {
        let mut waiting: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
        for r in &self.risky {
            if let Some(d) = r.waits {
                waiting.entry(d.what()).or_default().push(r.case.subject());
            }
        }
        let bad: Vec<String> = self
            .risky
            .iter()
            .filter(|r| r.case.blocks())
            .map(|r| r.case.text())
            .collect();
        if !bad.is_empty() {
            return Err(Refused(format!(
                "{}: nothing is written while the releases fact stands: {}",
                self.project,
                bad.join("; ")
            )));
        }
        if waiting.is_empty() {
            return Ok(());
        }
        let parts: Vec<String> = waiting
            .iter()
            .map(|(what, on)| format!("{what} ({})", on.join(", ")))
            .collect();
        Err(Refused(format!(
            "{}: nothing is written while a case waits on a decision: {}",
            self.project,
            parts.join("; ")
        )))
    }

    /// The releases, the areas with their member counts, labels, every change and every risky case,
    /// a line each.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![format!("{}:", self.project)];
        for r in &self.releases {
            let from = if r.from_fact {
                "the releases fact"
            } else {
                "a version theme"
            };
            out.push(format!(
                "  release {} at {}, from {from}",
                r.name, r.position
            ));
        }
        for a in &self.areas {
            let members = self
                .changes
                .iter()
                .filter(|c| matches!(c, Change::InArea { area, .. } if *area == a.name))
                .count();
            let from = match (&a.concept, a.history) {
                (Some(c), _) => c.as_str(),
                (None, true) => "closed items only",
                (None, false) => "a placement",
            };
            out.push(format!(
                "  area {} at {}, from {from}, {members} items",
                a.name, a.position
            ));
        }
        let concepts = self.areas.iter().filter(|a| a.concept.is_some()).count();
        let placements = self
            .areas
            .iter()
            .filter(|a| a.concept.is_none() && !a.history)
            .count();
        out.push(format!(
            "  areas: {concepts} from concepts, {placements} from placements"
        ));
        let n = &self.placed;
        out.push(format!(
            "  items: {} by plan, {} by concept, {} by placement, {} closed to {UNSORTED}, {} open unplaced",
            n.by_plan, n.by_concept, n.by_placement, n.unsorted, n.unplaced
        ));
        for (name, about) in &self.labels {
            if about.is_empty() {
                out.push(format!("  label {name}"));
            } else {
                out.push(format!("  label {name}: {about}"));
            }
        }
        out.extend(self.changes.iter().map(|c| format!("  {}", c.line())));
        if !self.grows.is_empty() {
            out.push(format!(
                "  the current release grows by {}: {}",
                self.grows.len(),
                self.grows.join(", ")
            ));
        }
        out.push(format!("  {} risky cases", self.risky.len()));
        out.extend(self.risky.iter().map(|r| format!("  risky: {}", r.line())));
        out
    }
}

/// A theme's semantic version, or `None` when it is not one (`1.1` and `v2.0` are not).
#[must_use]
pub fn version(theme: &str) -> Option<Version> {
    crate::release::semantic(theme).ok()
}

/// The fields an item file loses and gains, named in the dump case.
/// The wait text the old store gave a plan holding its audit until everything it opened closed.
const STORE_GATE: &str = "everything it opened is closed";

const DUMP_SHAPE: &str = "the theme, group, tags, rank, turn, wait and claim fields give way to release, priority, labels, parent, origin, dependencies and assignments; a full dump pass follows the copy";

/// Where an item sits: a release's position, or the backlog after every release.
type Place = Option<usize>;

fn later(a: Place, b: Place) -> bool {
    a.unwrap_or(usize::MAX) > b.unwrap_or(usize::MAX)
}

/// An id's key and number, for key and number order.
fn key_num(id: &str) -> (&str, u64) {
    let split = id.find(|c: char| c.is_ascii_digit()).unwrap_or(id.len());
    (&id[..split], id[split..].parse().unwrap_or(0))
}

/// The keys every project has: the types' own and the story, concept, idea and package keys the
/// move already maps. Any other key is the project's.
const FIXED_KEYS: [&str; 10] = ["T", "B", "Q", "R", "I", "A", "STY", "CON", "CID", "PK"];

struct Project<'a> {
    rules: Rules,
    items: Vec<&'a OldItem>,
    by_id: BTreeMap<&'a str, &'a OldItem>,
    kinds: BTreeMap<String, Kind>,
    /// The meaning of each key the project made.
    meanings: BTreeMap<String, String>,
    releases: Vec<Release>,
    place: BTreeMap<String, Place>,
    out: Changes,
    /// Each item's changes, gathered then written out in item order.
    each: BTreeMap<String, Vec<Change>>,
    /// The plans that could be each item's parent, earliest open first.
    candidates: BTreeMap<String, Vec<String>>,
    parent: BTreeMap<String, String>,
    deps: Vec<(String, String)>,
    /// Items whose theme is no release, placed by their plan once the parents are known.
    by_plan: Vec<String>,
    /// Each item whose theme is not a release, with the theme.
    theme_of: Vec<(String, String)>,
}

impl<'a> Project<'a> {
    fn new(rows: &Rows<'a>, rules: Rules) -> Self {
        let mut items = rows.items.clone();
        items.sort_by_key(|i| key_num(&i.id));
        let keys: Vec<KeySpec> =
            serde_json::from_value(rows.project.keys.clone()).unwrap_or_default();
        let meanings = keys
            .iter()
            .filter(|k| !FIXED_KEYS.contains(&k.key.as_str()))
            .map(|k| (k.key.clone(), k.meaning.clone().unwrap_or_default()))
            .collect();
        let kinds = keys.into_iter().map(|k| (k.key, k.kind)).collect();
        Project {
            rules,
            by_id: items.iter().map(|i| (i.id.as_str(), *i)).collect(),
            items,
            kinds,
            meanings,
            releases: Vec::new(),
            place: BTreeMap::new(),
            out: Changes {
                project: rows.project.slug.clone(),
                ..Changes::default()
            },
            each: BTreeMap::new(),
            candidates: BTreeMap::new(),
            parent: BTreeMap::new(),
            deps: Vec::new(),
            by_plan: Vec::new(),
            theme_of: Vec::new(),
        }
    }

    fn kind(&self, id: &str) -> Kind {
        self.kinds.get(key_num(id).0).copied().unwrap_or(Kind::Work)
    }

    fn open(&self, id: &str) -> bool {
        self.by_id.get(id).is_some_and(|i| i.state == "open")
    }

    fn push(&mut self, id: &str, change: Change) {
        self.each.entry(id.to_string()).or_default().push(change);
    }

    fn release_name(&self, place: Place) -> String {
        place
            .and_then(|p| self.releases.get(p))
            .map_or_else(|| "the backlog".to_string(), |r| r.name.clone())
    }

    fn place_of(&self, id: &str) -> Place {
        self.place.get(id).copied().flatten()
    }

    /// The releases fact, with the semantic version themes it does not name placed among its names
    /// in version order. A fact name that is no semantic version, or a fact out of order, is risky.
    fn releases(&mut self, skills: &serde_json::Value) {
        let listed: Vec<String> = skills
            .get("releases")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let parsed: Vec<Option<Version>> = listed.iter().map(|n| version(n)).collect();
        let bad: Vec<String> = listed
            .iter()
            .zip(&parsed)
            .filter(|(_, v)| v.is_none())
            .map(|(n, _)| n.clone())
            .collect();
        let mut ordered = bad.is_empty();
        if !bad.is_empty() {
            self.out.risky.push(Risky {
                case: Case::ReleaseName { names: bad },
                action: "nothing is written until the fact names semantic versions".to_string(),
                waits: None,
            });
        } else if !parsed
            .windows(2)
            .all(|w| matches!((&w[0], &w[1]), (Some(a), Some(b)) if a.cmp_precedence(b).is_lt()))
        {
            ordered = false;
            self.out.risky.push(Risky {
                case: Case::ReleaseOrder {
                    names: listed.clone(),
                },
                action: "nothing is written until the fact is in version order".to_string(),
                waits: None,
            });
        }
        let mut rows: Vec<(String, Option<Version>, bool)> = listed
            .iter()
            .cloned()
            .zip(parsed)
            .map(|(n, v)| (n, v, true))
            .collect();
        let mut themes: Vec<(Version, &str)> = self
            .items
            .iter()
            .filter_map(|i| i.theme.as_deref())
            .filter(|t| !listed.iter().any(|r| r == t))
            .filter_map(|t| version(t).map(|v| (v, t)))
            .collect();
        themes.sort_by(|a, b| a.0.cmp_precedence(&b.0).then(a.1.cmp(b.1)));
        themes.dedup_by(|a, b| a.0.cmp_precedence(&b.0).is_eq());
        for (v, t) in themes {
            let at = if ordered {
                let taken = rows
                    .iter()
                    .any(|r| r.1.as_ref().is_some_and(|rv| rv.cmp_precedence(&v).is_eq()));
                if taken {
                    continue;
                }
                rows.iter()
                    .position(|r| r.1.as_ref().is_some_and(|rv| rv.cmp_precedence(&v).is_gt()))
                    .unwrap_or(rows.len())
            } else {
                rows.len()
            };
            rows.insert(at, (t.to_string(), Some(v), false));
        }
        self.releases = rows
            .into_iter()
            .enumerate()
            .map(|(position, (name, _, from_fact))| Release {
                name,
                position,
                from_fact,
            })
            .collect();
    }

    /// Each item's release and labels: tags, group, a theme that is not a release, a story, an idea.
    fn places_and_labels(&mut self, notes: &[Theme]) {
        let current: Place = (!self.releases.is_empty()).then_some(0);
        let items = self.items.clone();
        for i in &items {
            let release = i
                .theme
                .as_deref()
                .map(|t| (t, self.releases.iter().position(|r| r.name == t)));
            let place = match release {
                None => current,
                Some((_, Some(p))) => Some(p),
                Some((t, None)) => {
                    self.theme(i, t, notes);
                    None
                }
            };
            self.place.insert(i.id.clone(), place);
            self.labels_of(i);
        }
        let open = self.items.iter().filter(|i| i.state == "open").count();
        if current.is_none() && open > 0 {
            self.out.risky.push(Risky {
                case: Case::NoRelease { open },
                action: "every item goes to the backlog".to_string(),
                waits: None,
            });
        }
    }

    /// An item whose theme is not a release: labelled with it, and left unplaced for `theme_places`.
    fn theme(&mut self, i: &OldItem, theme: &str, notes: &[Theme]) {
        let label = format!("area:{theme}");
        let about = notes
            .iter()
            .find(|n| n.name == theme)
            .and_then(|n| n.note.clone());
        self.out
            .labels
            .insert(label.clone(), about.unwrap_or_default());
        self.push(
            &i.id,
            Change::Label {
                id: i.id.clone(),
                label,
            },
        );
        self.theme_of.push((i.id.clone(), theme.to_string()));
        if self.rules.themes == Themes::Plan {
            self.by_plan.push(i.id.clone());
        }
    }

    /// Each item waiting on its plan for a release takes the plan's, up the chain of plans; then
    /// each theme that is not a release is listed with where its open items went.
    fn theme_places(&mut self) {
        self.chain_themes();
        let mut themes: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for (id, theme) in &self.theme_of {
            if self.items.iter().any(|i| i.id == *id && i.state == "open") {
                let n = themes.entry(theme).or_default();
                if self.place_of(id).is_some() {
                    n.0 += 1;
                } else {
                    n.1 += 1;
                }
            }
        }
        let folded: Vec<Risky> = themes
            .into_iter()
            .map(|(theme, (plan, backlog))| Risky {
                case: Case::FoldedTheme {
                    theme: theme.to_string(),
                    open: plan + backlog,
                },
                action: format!("{plan} to their plan's release, {backlog} to the backlog"),
                waits: None,
            })
            .collect();
        self.out.risky.extend(folded);
    }

    /// Each key the project made that holds items, with its counts.
    fn own_keys(&mut self) {
        let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for i in &self.items {
            let key = key_num(&i.id).0;
            if self.meanings.contains_key(key)
                && matches!(
                    self.kind(&i.id),
                    Kind::Work | Kind::Decision | Kind::Research
                )
            {
                let n = counts.entry(key).or_default();
                n.0 += 1;
                n.1 += usize::from(i.state == "open");
            }
        }
        let own: Vec<Risky> = counts
            .into_iter()
            .map(|(key, (items, open))| Risky {
                case: Case::OwnKey {
                    key: key.to_string(),
                    meaning: self.meanings[key].clone(),
                    items,
                    open,
                },
                action: "keeps its ids; new items file under T with the label".to_string(),
                waits: None,
            })
            .collect();
        self.out.risky.extend(own);
    }

    fn chain_themes(&mut self) {
        let waiting = std::mem::take(&mut self.by_plan);
        for _ in 0..waiting.len() {
            let mut moved = false;
            for id in &waiting {
                let at = self.parent.get(id).and_then(|p| self.place_of(p));
                if at.is_some() && self.place_of(id) != at {
                    self.place.insert(id.clone(), at);
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
    }

    fn labels_of(&mut self, i: &OldItem) {
        let id = i.id.clone();
        let tier = priority(&i.tags);
        if tier != "normal" {
            self.push(
                &id,
                Change::Priority {
                    id: id.clone(),
                    priority: tier.to_string(),
                },
            );
        }
        let mut labels: Vec<String> = i
            .tags
            .iter()
            .filter(|t| !PRIORITIES.contains(&t.as_str()))
            .cloned()
            .collect();
        if let Some(g) = &i.group {
            labels.push(crate::label::of_group(g));
        }
        let kind = self.kind(&id);
        let key = key_num(&id).0;
        if self.meanings.contains_key(key) {
            let own = match kind {
                Kind::Work => Some("task"),
                Kind::Decision => Some("question"),
                Kind::Research => Some("investigation"),
                _ => None,
            };
            if let Some(own) = own {
                self.each
                    .entry(id.clone())
                    .or_default()
                    .extend([Change::Typed {
                        id: id.clone(),
                        kind: own.to_string(),
                    }]);
                let label = format!("key:{}", key.to_lowercase());
                self.out
                    .labels
                    .insert(label.clone(), self.meanings[key].clone());
                labels.extend([label]);
            }
        }
        if matches!(kind, Kind::Story | Kind::Package) {
            self.push(&id, Change::Plan { id: id.clone() });
        }
        if kind == Kind::Story {
            labels.push("story".to_string());
        }
        if kind == Kind::Idea {
            let label = goal_label(i);
            let about = i.body.trim();
            self.out.labels.insert(
                label.clone(),
                if about.is_empty() { &i.title } else { about }.to_string(),
            );
            self.push(
                &id,
                Change::BecomesLabel {
                    id: id.clone(),
                    label,
                },
            );
        }
        let standing: Vec<String> = self
            .items
            .iter()
            .filter(|o| self.kind(&o.id) == Kind::Idea)
            .filter(|o| tied(i, o))
            .filter(|o| o.id != i.id)
            .map(|o| goal_label(o))
            .collect();
        labels.extend(standing);
        for label in labels {
            self.out.labels.entry(label.clone()).or_default();
            self.push(
                &id,
                Change::Label {
                    id: id.clone(),
                    label,
                },
            );
        }
    }

    /// Each concept item, in id order, as an area: one of the project's areas when it has the name,
    /// else a new row after them. A name two concepts share takes the second's id.
    fn concept_areas(&mut self, existing: &[crate::area::Area]) -> BTreeMap<String, String> {
        let mut named = BTreeMap::new();
        let concepts: Vec<&OldItem> = self
            .items
            .iter()
            .copied()
            .filter(|i| self.kind(&i.id) == Kind::Concept)
            .collect();
        for c in concepts {
            let mut name = area_name(&c.title);
            if name.is_empty() {
                name = c.id.to_lowercase();
            }
            let same = |a: &str| a.to_lowercase() == name.to_lowercase();
            if let Some(a) = existing.iter().find(|a| same(&a.name)) {
                named.insert(c.id.clone(), a.name.clone());
                continue;
            }
            if self.out.areas.iter().any(|a| same(&a.name)) {
                name = format!("{name}-{}", c.id.to_lowercase());
            }
            let description = if c.body.trim().is_empty() {
                c.title.clone()
            } else {
                format!("{}\n\n{}", c.title, c.body.trim())
            };
            self.out.areas.push(Area {
                name: name.clone(),
                description,
                position: existing.len() + self.out.areas.len(),
                concept: Some(c.id.clone()),
                history: false,
            });
            named.insert(c.id.clone(), name);
        }
        named
    }

    /// Each concept item becomes an area, then each item takes the area of its nearest plan up the
    /// parent edges that has one, else its own concept, the oldest tie when it has several, else the
    /// area of its latest placement by an agent, as written. A plan is placed before its children, so
    /// it passes its area down. A closed item nothing places goes to `unsorted`, made only when one
    /// does and never passed down; an open one waits on a placement. An idea item becomes a label and
    /// goes to `unsorted` like a closed item.
    fn areas(&mut self, existing: &[crate::area::Area], events: &[&EventDump]) {
        let named = self.concept_areas(existing);
        let concepts: Vec<&OldItem> = self
            .items
            .iter()
            .copied()
            .filter(|i| named.contains_key(&i.id))
            .collect();
        let ages = tie_ages(events);
        let placements = placements(events);
        let mut items: Vec<&OldItem> = self
            .items
            .iter()
            .copied()
            .filter(|i| !matches!(self.kind(&i.id), Kind::Concept | Kind::Idea))
            .collect();
        items.sort_by_cached_key(|i| self.depth(&i.id));
        let mut placed: BTreeMap<String, String> = BTreeMap::new();
        let (mut open, mut closed) = (Vec::new(), Vec::new());
        for i in items {
            let own = own_areas(i, &concepts, &named, &ages);
            let plan = self
                .parent
                .get(&i.id)
                .and_then(|p| placed.get(p).map(|a| (p.clone(), a.clone())));
            let area = match (self.rule_area(i, plan, own), placements.get(&i.id)) {
                (Some(area), Some((p, _))) => {
                    self.out.risky.push(Risky {
                        case: Case::PlacementIgnored {
                            id: i.id.clone(),
                            placed: p.area.clone(),
                            area: area.clone(),
                        },
                        action: "the agent's placement is ignored".to_string(),
                        waits: None,
                    });
                    area
                }
                (Some(area), None) => area,
                (None, Some((p, basis))) => {
                    let area = self.placement_area(existing, p);
                    self.push(
                        &i.id,
                        Change::Placed {
                            id: i.id.clone(),
                            area: area.clone(),
                            basis: basis.clone(),
                        },
                    );
                    self.out.placed.by_placement += 1;
                    area
                }
                (None, None) if i.area.is_some() => i.area.clone().unwrap_or_default(),
                (None, None) => {
                    if i.state == "open" {
                        open.push(i.id.clone());
                    } else {
                        closed.push(i.id.clone());
                    }
                    continue;
                }
            };
            placed.insert(i.id.clone(), area);
        }
        self.send_to_unsorted(existing, closed, &mut placed);
        for c in &concepts {
            let area = named[&c.id].clone();
            placed.insert(c.id.clone(), area.clone());
            self.push(
                &c.id,
                Change::BecomesArea {
                    id: c.id.clone(),
                    area,
                },
            );
        }
        for (id, area) in placed {
            self.push(
                &id,
                Change::InArea {
                    id: id.clone(),
                    area,
                },
            );
        }
        if !open.is_empty() {
            open.sort_by(|a, b| key_num(a).cmp(&key_num(b)));
            self.out.placed.unplaced = open.len();
            self.out.risky.push(Risky {
                case: Case::Unplaced { open },
                action: "they are left with no area".to_string(),
                waits: Some(Decision::Unplaced),
            });
        }
    }

    /// The closed items nothing placed, and each idea item, go to `unsorted`, made only when one does.
    fn send_to_unsorted(
        &mut self,
        existing: &[crate::area::Area],
        mut closed: Vec<String>,
        placed: &mut BTreeMap<String, String>,
    ) {
        closed.extend(
            self.items
                .iter()
                .filter(|i| self.kind(&i.id) == Kind::Idea)
                .map(|i| i.id.clone()),
        );
        if closed.is_empty() {
            return;
        }
        let area = self.unsorted(existing);
        self.out.placed.unsorted = closed.len();
        for id in closed {
            placed.insert(id, area.clone());
        }
    }

    /// The area of an item's nearest plan with one, else its oldest tie to a concept, each choice
    /// that gives up another tie listed for an open item.
    fn rule_area(
        &mut self,
        i: &OldItem,
        plan: Option<(String, String)>,
        mut own: Vec<String>,
    ) -> Option<String> {
        if let Some((plan, area)) = plan {
            own.retain(|a| *a != area);
            if !own.is_empty() && i.state == "open" {
                self.out.risky.push(Risky {
                    case: Case::PlanArea {
                        id: i.id.clone(),
                        plan,
                        area: area.clone(),
                        own,
                    },
                    action: format!("it takes its plan's area {area}"),
                    waits: None,
                });
            }
            self.out.placed.by_plan += 1;
            return Some(area);
        }
        if own.is_empty() {
            return None;
        }
        let gave_up = own.split_off(1);
        let area = own.remove(0);
        if !gave_up.is_empty() && i.state == "open" {
            self.out.risky.push(Risky {
                case: Case::SeveralConcepts {
                    id: i.id.clone(),
                    area: area.clone(),
                    gave_up,
                },
                action: format!("it takes {area}, its oldest tie"),
                waits: None,
            });
        }
        self.out.placed.by_concept += 1;
        Some(area)
    }

    /// The area a placement names: one the project has or the move makes, by name in any case, else
    /// a new row after those, its `about` the description.
    fn placement_area(&mut self, existing: &[crate::area::Area], p: &Placement) -> String {
        let same = |a: &str| a.to_lowercase() == p.area.to_lowercase();
        if let Some(a) = existing.iter().find(|a| same(&a.name)) {
            return a.name.clone();
        }
        if let Some(a) = self.out.areas.iter().find(|a| same(&a.name)) {
            return a.name.clone();
        }
        self.out.areas.push(Area {
            name: p.area.clone(),
            description: p.about.clone().unwrap_or_default(),
            position: existing.len() + self.out.areas.len(),
            concept: None,
            history: false,
        });
        p.area.clone()
    }

    /// The `unsorted` area, made last when the project has none by that name.
    fn unsorted(&mut self, existing: &[crate::area::Area]) -> String {
        let same = |a: &str| a.to_lowercase() == UNSORTED;
        if let Some(a) = existing.iter().find(|a| same(&a.name)) {
            return a.name.clone();
        }
        self.out.areas.push(Area {
            name: UNSORTED.to_string(),
            description: UNSORTED_ABOUT.to_string(),
            position: existing.len() + self.out.areas.len(),
            concept: None,
            history: true,
        });
        UNSORTED.to_string()
    }

    /// How many parent edges lie above an item.
    fn depth(&self, id: &str) -> usize {
        let mut seen = BTreeSet::from([id]);
        let mut at = id;
        while let Some(p) = self.parent.get(at) {
            if !seen.insert(p) {
                break;
            }
            at = p;
        }
        seen.len()
    }

    /// The plans nearest up each chain of openers from `from`, stopping at the first plan on each.
    fn plans_above(&self, from: &str) -> Vec<String> {
        let (mut seen, mut todo, mut out) = (BTreeSet::new(), VecDeque::from([from]), Vec::new());
        while let Some(x) = todo.pop_front() {
            if !seen.insert(x) {
                continue;
            }
            if x != from && self.kind(x).is_plan() {
                out.push(x.to_string());
                continue;
            }
            if let Some(i) = self.by_id.get(x) {
                todo.extend(i.opened.iter().map(String::as_str));
            }
        }
        out
    }

    /// Each item's origins from `opened`, and the plans that could be its parent, earliest open
    /// first: the parent it has already, an opener that is a plan, and the nearest plans above one
    /// that is not.
    fn openers(&mut self) {
        let items = self.items.clone();
        for i in &items {
            let mut plans: Vec<String> = i
                .parent
                .iter()
                .filter(|p| self.by_id.contains_key(p.as_str()))
                .cloned()
                .collect();
            let opened: Vec<&str> = i
                .opened
                .iter()
                .map(String::as_str)
                .filter(|o| self.by_id.contains_key(o))
                .collect();
            for o in opened {
                if self.kind(o).is_plan() {
                    plans.push(o.to_string());
                } else {
                    self.push(
                        &i.id,
                        Change::Origin {
                            id: i.id.clone(),
                            from: o.to_string(),
                        },
                    );
                    plans.extend(self.plans_above(o));
                }
            }
            plans.retain(|p| p != &i.id);
            let order = |p: &String| {
                let item = self.by_id[p.as_str()];
                (item.state != "open", item.opened_at.clone())
            };
            plans.sort_by(|a, b| order(a).cmp(&order(b)).then(key_num(a).cmp(&key_num(b))));
            plans.dedup();
            if !plans.is_empty() {
                self.candidates.insert(i.id.clone(), plans);
            }
        }
    }

    /// The first plan left of each item's candidates is its parent; the rest become related.
    fn parents(&mut self) {
        let candidates = std::mem::take(&mut self.candidates);
        let items = self.items.clone();
        for i in &items {
            let Some(mut plans) = candidates.get(&i.id).cloned() else {
                continue;
            };
            if plans.is_empty() {
                continue;
            }
            let others = plans.split_off(1);
            let kept = plans.remove(0);
            for o in &others {
                let why = format!("also under plan {o}, its parent is {kept}");
                self.push(
                    &i.id,
                    Change::Related {
                        id: i.id.clone(),
                        to: o.clone(),
                        why,
                    },
                );
            }
            if !others.is_empty() && i.state == "open" {
                self.out.risky.push(Risky {
                    case: Case::TwoPlans {
                        id: i.id.clone(),
                        kept: kept.clone(),
                        others: others.clone(),
                    },
                    action: format!(
                        "{kept}, the earliest open, is the parent; {} become related",
                        others.join(", ")
                    ),
                    waits: None,
                });
            }
            self.parent.insert(i.id.clone(), kept);
        }
    }

    /// Item waits to dependencies, the gate removed, a condition to a task for the owner.
    fn waits(&mut self) {
        let items = self.items.clone();
        for i in items.iter().filter(|i| i.state == "open") {
            let Some(on) = i.wait_ref.clone() else {
                continue;
            };
            let id = i.id.clone();
            match i.wait_on.as_deref() {
                Some("item") if self.by_id.contains_key(on.as_str()) => self.deps.push((id, on)),
                Some("condition") if on == STORE_GATE => {
                    self.push(&id, Change::GateRemoved { id: id.clone() });
                }
                Some(_) => self.push(
                    &id,
                    Change::OwnerTask {
                        id: id.clone(),
                        condition: on,
                    },
                ),
                None => {}
            }
        }
    }

    /// Every candidate parent edge, then the dependencies, added in item order; an edge that would
    /// close a cycle is kept as `related` instead. A plan is held by its children, an item by what it
    /// depends on.
    fn cycles(&mut self) {
        let mut holds: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let parents: Vec<(String, String)> = self
            .items
            .iter()
            .filter_map(|i| self.candidates.get(&i.id).map(|plans| (&i.id, plans)))
            .flat_map(|(c, plans)| plans.iter().map(move |p| (p.clone(), c.clone())))
            .collect();
        let deps = std::mem::take(&mut self.deps);
        let edges = parents
            .into_iter()
            .map(|e| (e, true))
            .chain(deps.into_iter().map(|e| (e, false)));
        for ((id, on), is_parent) in edges {
            if let Some(path) = path(&holds, &on, &id) {
                let shown = format!("{} -> {on}", path.join(" -> "));
                let (from, to) = if is_parent { (&on, &id) } else { (&id, &on) };
                self.push(
                    from,
                    Change::Related {
                        id: from.clone(),
                        to: to.clone(),
                        why: format!("it closed a cycle: {shown}"),
                    },
                );
                if is_parent && let Some(plans) = self.candidates.get_mut(&on) {
                    plans.retain(|p| p != &id);
                }
                self.out.risky.push(Risky {
                    case: Case::Cycle {
                        id: id.clone(),
                        on: on.clone(),
                        path,
                    },
                    action: format!("the hold of {id} on {on} is kept as related"),
                    waits: None,
                });
                continue;
            }
            holds.entry(id.clone()).or_default().push(on.clone());
            if !is_parent {
                self.deps.push((id, on));
            }
        }
    }

    /// Repairs release order until it holds: an open child in a later release than its open plan is
    /// pulled into the plan's release, and an open dependency in a later release than its open
    /// dependant into the dependant's. A pulled plan is looked at again, so its children and its
    /// dependencies follow it.
    fn repair_release_order(&mut self) {
        let mut held: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        loop {
            let mut kids: Vec<(&String, &String)> = self
                .parent
                .iter()
                .filter(|(child, plan)| {
                    self.open(child)
                        && self.open(plan)
                        && later(self.place_of(child), self.place_of(plan))
                })
                .collect();
            kids.sort_by(|a, b| {
                key_num(a.1)
                    .cmp(&key_num(b.1))
                    .then(key_num(a.0).cmp(&key_num(b.0)))
            });
            if let Some((child, plan)) = kids.first().map(|(c, p)| ((*c).clone(), (*p).clone())) {
                let (at, was) = (self.place_of(&plan), self.place_of(&child));
                held.entry(plan)
                    .or_default()
                    .push((child.clone(), self.release_name(was)));
                self.pull(&child, at);
                continue;
            }
            let found = self.deps.iter().find(|(id, on)| {
                self.open(id) && self.open(on) && later(self.place_of(on), self.place_of(id))
            });
            let Some((id, on)) = found.cloned() else {
                break;
            };
            let (at, was) = (self.place_of(&id), self.place_of(&on));
            self.pull(&on, at);
            self.out.risky.push(Risky {
                case: Case::Inversion {
                    id: id.clone(),
                    release: self.release_name(at),
                    on: on.clone(),
                    on_release: self.release_name(was),
                },
                action: format!("{on} is pulled into {}", self.release_name(at)),
                waits: None,
            });
        }
        let mut plans: Vec<(String, Vec<(String, String)>)> = held.into_iter().collect();
        plans.sort_by(|a, b| key_num(&a.0).cmp(&key_num(&b.0)));
        for (plan, pulled) in plans {
            let mut named: Vec<(String, String)> = Vec::new();
            for (child, was) in pulled {
                if !named.iter().any(|(c, _)| *c == child) {
                    named.push((child, was));
                }
            }
            named.sort_by(|a, b| key_num(&a.0).cmp(&key_num(&b.0)));
            let at = self.place_of(&plan);
            let children: Vec<&str> = named.iter().map(|(c, _)| c.as_str()).collect();
            let action = format!(
                "{} move into {}",
                children.join(", "),
                self.release_name(at)
            );
            self.out.risky.push(Risky {
                case: Case::HeldPlan {
                    plan: plan.clone(),
                    release: self.release_name(at),
                    later: named,
                },
                action,
                waits: None,
            });
        }
        self.out.grows.sort_by(|a, b| key_num(a).cmp(&key_num(b)));
    }

    fn pull(&mut self, id: &str, to: Place) {
        if to == Some(0) && self.place_of(id) != Some(0) && !self.out.grows.iter().any(|g| g == id)
        {
            self.out.grows.push(id.to_string());
        }
        self.place.insert(id.to_string(), to);
    }

    /// Past attempts rebuilt from the events, made to agree with the claim and the ask held now.
    fn assignments(&mut self, events: &[&EventDump]) {
        let items = self.items.clone();
        for i in &items {
            let mine: Vec<assignment::Event> = events
                .iter()
                .filter(|e| e.item.as_deref() == Some(i.id.as_str()))
                .map(|e| assignment::Event::from(*e))
                .collect();
            let mut rows = assignment::rebuild(&mine);
            assignment::settle(&mut rows, &assignment::Now::of(i));
            if i.state == "open" && i.turn.as_deref() == Some("user") {
                self.push(&i.id, Change::Assignee { id: i.id.clone() });
            }
            for a in rows {
                self.push(
                    &i.id,
                    Change::Assignment(Assignment {
                        id: i.id.clone(),
                        kind: a.kind.as_str(),
                        branch: a.branch.unwrap_or_default(),
                        host: a.host,
                        started_at: a.started_at,
                        ended_at: a.ended_at,
                        outcome: a.outcome.map(assignment::Outcome::as_str),
                    }),
                );
            }
        }
    }

    /// Every change, item by item: release first, then what was gathered, parent and dependencies.
    fn finish(mut self) -> Changes {
        let deps = std::mem::take(&mut self.deps);
        for (id, on) in deps {
            self.push(&id, Change::DependsOn { id: id.clone(), on });
        }
        let parents = std::mem::take(&mut self.parent);
        for (id, parent) in parents {
            self.push(
                &id,
                Change::Parent {
                    id: id.clone(),
                    parent,
                },
            );
        }
        for i in &self.items {
            let release = self
                .place_of(&i.id)
                .and_then(|p| self.releases.get(p))
                .map(|r| r.name.clone());
            self.out.changes.push(Change::InRelease {
                id: i.id.clone(),
                release,
            });
            self.out
                .changes
                .extend(self.each.remove(&i.id).unwrap_or_default());
        }
        self.out.risky.push(Risky {
            case: Case::DumpFormat {
                files: self.items.len(),
            },
            action: DUMP_SHAPE.to_string(),
            waits: None,
        });
        self.out.releases = self.releases;
        self.out
    }
}

/// An area's name from a concept's title: the part before the first colon, or the whole title, in
/// lower case with spaces as hyphens.
#[must_use]
pub fn area_name(title: &str) -> String {
    let head = title.split(':').next().unwrap_or(title);
    head.split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

/// The label a central idea becomes: `goal:` and its title's words before the first colon, the id when
/// the title gives none.
fn goal_label(i: &OldItem) -> String {
    let slug = area_name(&i.title);
    format!(
        "goal:{}",
        if slug.is_empty() {
            i.id.to_lowercase()
        } else {
            slug
        }
    )
}

/// The areas of the concepts an item is tied to, its oldest tie first, then in id order.
fn own_areas(
    i: &OldItem,
    concepts: &[&OldItem],
    named: &BTreeMap<String, String>,
    ages: &BTreeMap<(String, String), String>,
) -> Vec<String> {
    let age = |c: &OldItem| {
        ages.get(&pair(&i.id, &c.id))
            .cloned()
            .unwrap_or_else(|| i.opened_at.clone().max(c.opened_at.clone()))
    };
    let mut own: Vec<&OldItem> = concepts.iter().copied().filter(|c| tied(i, c)).collect();
    own.sort_by_cached_key(|c| (age(c), key_num(&c.id)));
    let mut seen = BTreeSet::new();
    own.iter()
        .map(|c| named[&c.id].clone())
        .filter(|a| seen.insert(a.clone()))
        .collect()
}

/// Each item's latest `decided` event that carries a placement, with the basis it was derived on.
fn placements(events: &[&EventDump]) -> BTreeMap<String, (Placement, String)> {
    let mut ordered: Vec<&&EventDump> = events.iter().filter(|e| e.kind == "decided").collect();
    ordered.sort_by(|a, b| a.at.cmp(&b.at).then(a.uid.cmp(&b.uid)));
    let mut out = BTreeMap::new();
    for e in ordered {
        let (Some(item), Some(data)) = (&e.item, &e.data) else {
            continue;
        };
        let Some(p) = Placement::of(data) else {
            continue;
        };
        let basis = serde_json::from_str::<serde_json::Value>(data)
            .ok()
            .and_then(|v| v["derived"].as_str().map(str::to_string))
            .unwrap_or_default();
        out.insert(item.clone(), (p, basis));
    }
    out
}

/// Whether two items are tied by a `related` link, either way.
fn tied(a: &OldItem, b: &OldItem) -> bool {
    a.id != b.id && (a.related.contains(&b.id) || b.related.contains(&a.id))
}

fn pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// When each `related` tie standing now was made, by the events that link and unlink it: the link
/// after the last unlink. A tie no event records is as old as the later of its two items.
fn tie_ages(events: &[&EventDump]) -> BTreeMap<(String, String), String> {
    let mut ordered: Vec<&&EventDump> = events.iter().filter(|e| e.kind == "edited").collect();
    ordered.sort_by(|a, b| a.at.cmp(&b.at).then(a.uid.cmp(&b.uid)));
    let mut since: BTreeMap<(String, String), Option<String>> = BTreeMap::new();
    for e in ordered {
        let (Some(item), Some(note)) = (&e.item, &e.note) else {
            continue;
        };
        if let Some(to) = note.strip_prefix("link related ") {
            let at = since.entry(pair(item, to.trim())).or_default();
            if at.is_none() {
                *at = Some(e.at.clone());
            }
        } else if let Some(to) = note.strip_prefix("unlink related ") {
            since.insert(pair(item, to.trim()), None);
        }
    }
    since
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect()
}

/// The path of holds from `from` to `to`, both ends included, when there is one.
fn path(holds: &BTreeMap<String, Vec<String>>, from: &str, to: &str) -> Option<Vec<String>> {
    let mut back: BTreeMap<&str, &str> = BTreeMap::new();
    let mut todo = VecDeque::from([from]);
    let mut seen = BTreeSet::from([from]);
    while let Some(x) = todo.pop_front() {
        if x == to {
            let mut out = vec![x.to_string()];
            let mut at = x;
            while let Some(prev) = back.get(at) {
                out.push((*prev).to_string());
                at = prev;
            }
            out.reverse();
            return Some(out);
        }
        for n in holds.get(x).into_iter().flatten() {
            if seen.insert(n.as_str()) {
                back.insert(n.as_str(), x);
                todo.push_back(n.as_str());
            }
        }
    }
    None
}

/// What moving one project's rows onto the core changes, and the cases that need a person's eye.
#[must_use]
pub fn plan(rows: &Rows, rules: Rules) -> Changes {
    let mut p = Project::new(rows, rules);
    let notes: Vec<Theme> = serde_json::from_value(rows.project.themes.clone()).unwrap_or_default();
    p.releases(&rows.project.skills);
    p.places_and_labels(&notes);
    p.openers();
    p.waits();
    p.cycles();
    p.parents();
    p.areas(&rows.project.areas, &rows.events);
    p.theme_places();
    p.own_keys();
    p.repair_release_order();
    p.assignments(&rows.events);
    p.finish()
}

#[cfg(test)]
#[path = "tests/migrate.rs"]
mod tests;
