//! The move of a project's rows onto the core: releases as rows, dependencies, one parent plan, an
//! origin link, labels, a priority column and assignment rows. `plan` is the one place the mapping
//! is written; it reads the rows the dump carries and returns every change with the cases that need
//! a person's eye. A case whose repair turns on an undecided rule waits on it, and nothing is
//! written while one does.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::Serialize;

use crate::dump::{DumpPage, EventDump, ItemDump, ProjectDump};
use crate::fact;
use crate::item::Refused;
use crate::rows::{KeySpec, Theme};
use crate::rules::GATE;
use crate::word::{Kind, PRIORITIES, priority};

/// One project's rows as the dump carries them: items with their links by id, and events.
pub struct Rows<'a> {
    pub project: &'a ProjectDump,
    pub items: Vec<&'a ItemDump>,
    pub events: Vec<&'a EventDump>,
}

impl<'a> Rows<'a> {
    /// The rows of each project a page carries, in the page's order.
    #[must_use]
    pub fn of(page: &'a DumpPage) -> Vec<Rows<'a>> {
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

/// What happens to a plan held by a child in a later release.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Held {
    /// The later child leaves the plan and keeps an origin link to it.
    Detach,
    /// The plan moves to its latest child's release.
    MovePlan,
    /// The later children move into the plan's release.
    PullChildren,
}

/// Where an item goes whose theme is not a release. It carries the theme as a label either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Areas {
    Current,
    Backlog,
}

/// The rules the cases turn on; `None` is a rule not decided yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Rules {
    pub held: Option<Held>,
    pub areas: Option<Areas>,
}

/// A decision a risky case waits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Decision {
    Held,
    Areas,
}

impl Decision {
    #[must_use]
    pub fn what(self) -> &'static str {
        match self {
            Decision::Held => "what happens to a plan held by work in a later release",
            Decision::Areas => "where an item goes whose theme is not a release",
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

/// One attempt at an item, from its claim events or its claim now. No end is the claim held now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Assignment {
    pub id: String,
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
            Change::Priority { id, priority } => format!("{id} at priority {priority}"),
            Change::Label { id, label } => format!("{id} labelled {label}"),
            Change::BecomesLabel { id, label } => format!("{id} becomes the label {label}"),
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
                format!(
                    "{} attempt on {} at {} from {} {end}",
                    a.id, a.branch, a.host, a.started_at
                )
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
    /// The project names no release, so every item goes to the backlog.
    NoRelease { open: usize },
    /// The item files docket-dump writes change shape.
    DumpFormat { files: usize },
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
            Case::NoRelease { open } => {
                format!("the project names no release, with {open} open items")
            }
            Case::DumpFormat { files } => {
                format!("docket-dump's item files change shape, {files} files")
            }
        }
    }

    /// What a waiting case is named by in a refusal.
    fn subject(&self) -> String {
        match self {
            Case::HeldPlan { plan, .. } => plan.clone(),
            Case::FoldedTheme { theme, .. } => theme.clone(),
            Case::Cycle { id, .. } | Case::TwoPlans { id, .. } | Case::Inversion { id, .. } => {
                id.clone()
            }
            Case::NoRelease { .. } | Case::DumpFormat { .. } => String::new(),
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

    /// The releases, labels, every change and every risky case, a line each.
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

/// A theme's version numbers when it names a version: digits joined by dots, at least two, with an
/// optional leading `v`.
#[must_use]
pub fn version(theme: &str) -> Option<Vec<u64>> {
    let parts: Vec<&str> = theme
        .strip_prefix('v')
        .unwrap_or(theme)
        .split('.')
        .collect();
    if parts.len() < 2 {
        return None;
    }
    parts.iter().map(|p| p.parse::<u64>().ok()).collect()
}

/// The fields an item file loses and gains, named in the dump case.
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

fn is_plan(kind: Kind) -> bool {
    matches!(kind, Kind::Audit | Kind::Story | Kind::Package)
}

struct Project<'a> {
    rules: Rules,
    items: Vec<&'a ItemDump>,
    by_id: BTreeMap<&'a str, &'a ItemDump>,
    kinds: BTreeMap<String, Kind>,
    releases: Vec<Release>,
    place: BTreeMap<String, Place>,
    out: Changes,
    /// Each item's changes, gathered then written out in item order.
    each: BTreeMap<String, Vec<Change>>,
    /// The plans that could be each item's parent, earliest open first.
    candidates: BTreeMap<String, Vec<String>>,
    parent: BTreeMap<String, String>,
    deps: Vec<(String, String)>,
}

impl<'a> Project<'a> {
    fn new(rows: &Rows<'a>, rules: Rules) -> Self {
        let mut items = rows.items.clone();
        items.sort_by_key(|i| key_num(&i.id));
        let keys: Vec<KeySpec> =
            serde_json::from_value(rows.project.keys.clone()).unwrap_or_default();
        let kinds = keys.into_iter().map(|k| (k.key, k.kind)).collect();
        Project {
            rules,
            by_id: items.iter().map(|i| (i.id.as_str(), *i)).collect(),
            items,
            kinds,
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

    /// The releases fact, then the version themes it does not name, in version order.
    fn releases(&mut self, skills: &serde_json::Value) {
        let listed = fact::releases(skills.get("releases").and_then(|v| v.as_str()));
        let mut themes: Vec<(Vec<u64>, &str)> = self
            .items
            .iter()
            .filter_map(|i| i.theme.as_deref())
            .filter(|t| !listed.iter().any(|r| r == t))
            .filter_map(|t| version(t).map(|v| (v, t)))
            .collect();
        themes.sort_unstable();
        themes.dedup();
        let named = listed.iter().map(|n| (n.clone(), true));
        let found = themes.into_iter().map(|(_, t)| (t.to_string(), false));
        self.releases = named
            .chain(found)
            .enumerate()
            .map(|(position, (name, from_fact))| Release {
                name,
                position,
                from_fact,
            })
            .collect();
    }

    /// Each item's release and labels: tags, group, an area theme, a story, a standing kind.
    fn places_and_labels(&mut self, notes: &[Theme]) {
        let current: Place = (!self.releases.is_empty()).then_some(0);
        let mut areas: BTreeMap<String, usize> = BTreeMap::new();
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
                    *areas.entry(t.to_string()).or_default() += usize::from(i.state == "open");
                    self.area(i, t, notes, current)
                }
            };
            self.place.insert(i.id.clone(), place);
            self.labels_of(i);
        }
        for (theme, open) in areas.into_iter().filter(|(_, n)| *n > 0) {
            let action = match self.rules.areas {
                Some(Areas::Backlog) => "to the backlog with the label".to_string(),
                Some(Areas::Current) => {
                    "folded into the current release with the label".to_string()
                }
                None => "folded into the current release with the label until the rule is decided"
                    .to_string(),
            };
            self.out.risky.push(Risky {
                case: Case::FoldedTheme { theme, open },
                action,
                waits: self.rules.areas.is_none().then_some(Decision::Areas),
            });
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

    /// An item whose theme is not a release: labelled with it, and placed by the rule.
    fn area(&mut self, i: &ItemDump, theme: &str, notes: &[Theme], current: Place) -> Place {
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
        match self.rules.areas {
            Some(Areas::Backlog) => None,
            Some(Areas::Current) | None => current,
        }
    }

    fn labels_of(&mut self, i: &ItemDump) {
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
            labels.push(format!("group:{g}"));
        }
        let kind = self.kind(&id);
        if matches!(kind, Kind::Story | Kind::Package) {
            self.push(&id, Change::Plan { id: id.clone() });
        }
        if kind == Kind::Story {
            labels.push("story".to_string());
        }
        if kind.is_standing() {
            let label = id.to_lowercase();
            self.out.labels.insert(label.clone(), i.title.clone());
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
            .filter(|o| self.kind(&o.id).is_standing())
            .filter(|o| i.related.contains(&o.id) || o.related.contains(&i.id))
            .filter(|o| o.id != i.id)
            .map(|o| o.id.to_lowercase())
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

    /// The plans nearest up each chain of openers from `from`, stopping at the first plan on each.
    fn plans_above(&self, from: &str) -> Vec<String> {
        let (mut seen, mut todo, mut out) = (BTreeSet::new(), VecDeque::from([from]), Vec::new());
        while let Some(x) = todo.pop_front() {
            if !seen.insert(x) {
                continue;
            }
            if x != from && is_plan(self.kind(x)) {
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
    /// first: an opener that is a plan, and the nearest plans above one that is not.
    fn openers(&mut self) {
        let items = self.items.clone();
        for i in &items {
            let mut plans: Vec<String> = Vec::new();
            let opened: Vec<&str> = i
                .opened
                .iter()
                .map(String::as_str)
                .filter(|o| self.by_id.contains_key(o))
                .collect();
            for o in opened {
                if is_plan(self.kind(o)) {
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
                Some("condition") if on == GATE => {
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

    /// Open plans with open children in later releases, repaired by the rule when it is decided.
    fn held_plans(&mut self) {
        let mut by_plan: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (child, plan) in &self.parent {
            if self.open(child)
                && self.open(plan)
                && later(self.place_of(child), self.place_of(plan))
            {
                by_plan.entry(plan.clone()).or_default().push(child.clone());
            }
        }
        let mut plans: Vec<(String, Vec<String>)> = by_plan.into_iter().collect();
        plans.sort_by(|a, b| key_num(&a.0).cmp(&key_num(&b.0)));
        for (plan, mut children) in plans {
            children.sort_by(|a, b| key_num(a).cmp(&key_num(b)));
            let at = self.place_of(&plan);
            let named: Vec<(String, String)> = children
                .iter()
                .map(|c| (c.clone(), self.release_name(self.place_of(c))))
                .collect();
            let action = match self.rules.held {
                None => format!(
                    "{plan} stays in {} with them until the rule is decided",
                    self.release_name(at)
                ),
                Some(Held::Detach) => {
                    for c in &children {
                        self.parent.remove(c);
                        self.push(
                            c,
                            Change::Origin {
                                id: c.clone(),
                                from: plan.clone(),
                            },
                        );
                    }
                    format!(
                        "{} leave {plan} and keep an origin link to it",
                        children.join(", ")
                    )
                }
                Some(Held::MovePlan) => {
                    let latest = children
                        .iter()
                        .map(|c| self.place_of(c))
                        .max_by_key(|p| p.unwrap_or(usize::MAX))
                        .flatten();
                    self.place.insert(plan.clone(), latest);
                    format!("{plan} moves to {}", self.release_name(latest))
                }
                Some(Held::PullChildren) => {
                    for c in &children {
                        self.pull(c, at);
                    }
                    format!(
                        "{} move into {}",
                        children.join(", "),
                        self.release_name(at)
                    )
                }
            };
            self.out.risky.push(Risky {
                case: Case::HeldPlan {
                    plan: plan.clone(),
                    release: self.release_name(at),
                    later: named,
                },
                action,
                waits: self.rules.held.is_none().then_some(Decision::Held),
            });
        }
    }

    fn pull(&mut self, id: &str, to: Place) {
        if to == Some(0) && self.place_of(id) != Some(0) && !self.out.grows.iter().any(|g| g == id)
        {
            self.out.grows.push(id.to_string());
        }
        self.place.insert(id.to_string(), to);
    }

    /// Each dependency in a later release than its dependant is pulled into the dependant's release,
    /// until none is.
    fn inversions(&mut self) {
        loop {
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
        self.out.grows.sort_by(|a, b| key_num(a).cmp(&key_num(b)));
    }

    /// Past attempts from the claim events, and the claim held now.
    fn assignments(&mut self, events: &[&EventDump]) {
        let items = self.items.clone();
        for i in &items {
            let mut rows: Vec<Assignment> = Vec::new();
            let mut live: Option<Assignment> = None;
            for e in events
                .iter()
                .filter(|e| e.item.as_deref() == Some(i.id.as_str()))
            {
                let outcome = match e.kind.as_str() {
                    "claimed" => {
                        if let Some(mut a) = live.take() {
                            a.ended_at = Some(e.at.clone());
                            a.outcome = Some("failed");
                            rows.push(a);
                        }
                        live = Some(Assignment {
                            id: i.id.clone(),
                            branch: e.branch.clone().unwrap_or_default(),
                            host: e.host.clone(),
                            started_at: e.at.clone(),
                            ended_at: None,
                            outcome: None,
                        });
                        continue;
                    }
                    "closed" => "landed",
                    "waited" => "blocked",
                    "released" | "claim_lost" | "dropped" => "failed",
                    _ => continue,
                };
                if let Some(mut a) = live.take() {
                    a.ended_at = Some(e.at.clone());
                    a.outcome = Some(outcome);
                    rows.push(a);
                }
            }
            match (live, &i.claim_branch) {
                (Some(a), Some(_)) => rows.push(a),
                (Some(mut a), None) => {
                    a.ended_at = Some(i.updated_at.clone());
                    rows.push(a);
                }
                (None, Some(branch)) => rows.push(Assignment {
                    id: i.id.clone(),
                    branch: branch.clone(),
                    host: i.claim_host.clone().unwrap_or_default(),
                    started_at: i.claim_since.clone().unwrap_or_default(),
                    ended_at: None,
                    outcome: None,
                }),
                (None, None) => {}
            }
            if i.state == "open" && i.turn.as_deref() == Some("user") {
                self.push(&i.id, Change::Assignee { id: i.id.clone() });
            }
            for a in rows {
                self.push(&i.id, Change::Assignment(a));
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
    p.held_plans();
    p.inversions();
    p.assignments(&rows.events);
    p.finish()
}

#[cfg(test)]
#[path = "tests/migrate.rs"]
mod tests;
