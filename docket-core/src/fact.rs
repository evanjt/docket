//! A project's facts: what each one means, its default, and which a lead needs before it dispatches.

use std::collections::BTreeMap;

use crate::item::Refused;
use crate::text::py_repr;

/// Every fact a project carries, in the order `docket skills` prints them, with its one-line meaning.
pub const FACTS: [(&str, &str); 20] = [
    ("owner", "the owner's name, as the skills address them"),
    (
        "worktree",
        "the command that makes a worktree for item B14 on branch audit/b14-$n",
    ),
    ("merge", "the command that merges audit/b14-$n back"),
    (
        "publish",
        "the local ref a squash writes the published history to and the remote ref the owner pushes it to, as \"published origin/main\"; unset, the project does not squash",
    ),
    (
        "provision",
        "a shell command run in each job's new worktree before the job starts, such as linking node_modules; the job is refused when it fails",
    ),
    (
        "traps",
        "what breaks in a fresh worktree here, in a sentence or two",
    ),
    (
        "measure",
        "where a real number comes from: a key, a fixture, a stack, a device",
    ),
    (
        "hands",
        "what only the owner can do: devices, accounts, pushes, judgement calls",
    ),
    (
        "checkout",
        "the repository under the root that jobs build in, . for the root itself",
    ),
    (
        "gates",
        "the checks a job reruns after merging main, in words a job can follow",
    ),
    (
        "mode",
        "what a lead does: run dispatches, drain lets running jobs finish and dispatches none, pause dispatches nothing",
    ),
    (
        "models",
        "the runner, model and effort a job or a lead runs on, by complexity and role, as \"high=claude:MODEL:high medium=codex:MODEL low=... unrated=... audit=... plan=... lead=...\"",
    ),
    (
        "releases",
        "the releases not yet shipped in the order they ship, the current first, read from the release rows: docket releases add, move and ship change them. An item with no release is in the backlog. next orders by release, then priority",
    ),
    (
        "job_timeout",
        "minutes a job may run before the lead stops it and gives its ticket back",
    ),
    (
        "stale_claim",
        "minutes with no event on a claimed item before wip and the watch flag the claim",
    ),
    (
        "lead_lapse",
        "minutes a lead claim holds without a renewal before another session may take the lead over",
    ),
    (
        "owner_limit",
        "the open asks and undecided questions the owner may hold at once; past it an ask is refused",
    ),
    (
        "failure_limit",
        "the failed attempts at one item before it is assigned to the owner",
    ),
    (
        "prices",
        "money per million tokens by model, as \"model=input:output ...\", for the cost of an agent attempt",
    ),
    (
        "flow",
        "the work model the project is on, set by its migration",
    ),
];

/// Facts the old fleet loop read, gone with it: machines and the `models` fact took their place. A
/// stored value stays in the database and is no longer shown.
pub const RETIRED: [&str; 23] = [
    "remote",
    "mirror_exclude",
    "remote_prepare",
    "remote_setup",
    "land",
    "slice",
    "pool",
    "pool_max",
    "model_build",
    "model_review",
    "model_plan",
    "file_cap",
    "lanes",
    "packages_live",
    "ram_floor",
    "observe_cap",
    "plan_batch",
    "jobs_per_day",
    "brake_ratio",
    "loop_host",
    "poll",
    "paused_by",
    "last_tick",
];

/// The `models` entry's model name that leaves the choice to the runner: no model flag is passed.
pub const DEFAULT_MODEL: &str = "default";

/// The built-in `models`: the `claude` runner with its default model for every complexity and role.
pub const DEFAULT_MODELS: &str = "high=claude:default medium=claude:default low=claude:default \
                                  unrated=claude:default audit=claude:default plan=claude:default \
                                  lead=claude:default";

/// The facts that describe the owner's runners and pace, not the project: each resolves from the
/// project's value, then the owner-level value held on the server, then the built-in default.
pub const AGENT_SETTINGS: [&str; 5] =
    ["models", "job_timeout", "stale_claim", "lead_lapse", "mode"];

/// The value a fact takes when neither the project nor the owner sets one.
pub const DEFAULTS: [(&str, &str); 9] = [
    ("models", DEFAULT_MODELS),
    ("owner", "the owner"),
    ("checkout", "."),
    ("mode", "run"),
    ("job_timeout", "120"),
    ("stale_claim", "120"),
    ("lead_lapse", "10"),
    ("owner_limit", "20"),
    ("failure_limit", "2"),
];

/// Facts docket writes itself, never set by hand.
pub const WRITTEN: [&str; 2] = ["flow", "releases"];

/// Facts without which a lead cannot dispatch: the models its jobs run on.
pub const NEEDED: [&str; 1] = ["models"];

#[must_use]
pub fn meaning(key: &str) -> Option<&'static str> {
    FACTS.iter().find(|(k, _)| *k == key).map(|(_, m)| *m)
}

#[must_use]
pub fn default_of(key: &str) -> Option<&'static str> {
    DEFAULTS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// Where a fact's value comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Set(String),
    Default(&'static str),
    Unset,
}

/// A fact's value: the project's own, the default, or none.
#[must_use]
pub fn value(skills: &BTreeMap<String, String>, key: &str) -> Value {
    match skills.get(key).filter(|v| !v.is_empty()) {
        Some(v) => Value::Set(v.clone()),
        None => default_of(key).map_or(Value::Unset, Value::Default),
    }
}

/// Which level a fact's value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Project,
    Owner,
    Default,
}

impl Layer {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Layer::Project => "project",
            Layer::Owner => "owner",
            Layer::Default => "default",
        }
    }
}

/// A fact's value and the level it came from: the project's, for an agent setting the owner-level
/// one, else the built-in default. `None` when no level has one.
#[must_use]
pub fn layered(
    project: &BTreeMap<String, String>,
    owner: &BTreeMap<String, String>,
    key: &str,
) -> Option<(String, Layer)> {
    let set = |m: &BTreeMap<String, String>| m.get(key).filter(|v| !v.is_empty()).cloned();
    if let Some(v) = set(project) {
        return Some((v, Layer::Project));
    }
    if AGENT_SETTINGS.contains(&key)
        && let Some(v) = set(owner)
    {
        return Some((v, Layer::Owner));
    }
    default_of(key).map(|v| (v.to_string(), Layer::Default))
}

/// The project's facts with the owner-level agent settings it does not set itself.
#[must_use]
pub fn merged(
    project: &BTreeMap<String, String>,
    owner: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut out = project.clone();
    for key in AGENT_SETTINGS {
        if let Some((v, Layer::Owner)) = layered(project, owner, key) {
            out.insert(key.to_string(), v);
        }
    }
    out
}

/// Refuses an owner-level value that is not an agent setting or that the setting cannot hold. An
/// empty value unsets it.
///
/// # Errors
/// The key is no agent setting, or the value is outside what the setting holds.
pub fn check_owner(key: &str, value: &str) -> Result<(), Refused> {
    if !AGENT_SETTINGS.contains(&key) {
        return Err(Refused(format!(
            "{key} is a project fact: --all-projects takes one of {}",
            AGENT_SETTINGS.join(", ")
        )));
    }
    check(key, value)
}

/// The value as the loop reads it, set or default.
#[must_use]
pub fn effective(skills: &BTreeMap<String, String>, key: &str) -> Option<String> {
    match value(skills, key) {
        Value::Set(v) => Some(v),
        Value::Default(v) => Some(v.to_string()),
        Value::Unset => None,
    }
}

/// Why a lead dispatches nothing, a phrase per reason; empty when the facts let it run.
#[must_use]
pub fn gaps(project: &BTreeMap<String, String>, owner: &BTreeMap<String, String>) -> Vec<String> {
    let skills = &merged(project, owner);
    let mut out = Vec::new();
    let mode = effective(skills, "mode").unwrap_or_default();
    if mode != "run" {
        out.push(format!("mode is {mode}"));
    }
    for key in NEEDED {
        if value(skills, key) == Value::Unset {
            out.push(format!("no {key}"));
        }
    }
    out
}

/// The facts docket reads out of a stored set, leaving out retired and unknown keys.
#[must_use]
pub fn known(skills: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    skills
        .iter()
        .filter(|(k, _)| meaning(k).is_some())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// The values a fact may take, checked when it is set so the loop never reads a typo.
pub const CHOICES: [(&str, &[&str]); 1] = [("mode", &["run", "drain", "pause"])];

/// Facts that hold a whole number above 0.
pub const COUNTS: [&str; 5] = [
    "stale_claim",
    "job_timeout",
    "lead_lapse",
    "owner_limit",
    "failure_limit",
];

/// The keys a `models` entry may name: a build's complexity, then the audit, plan and lead roles.
pub const MODEL_KEYS: [&str; 7] = ["high", "medium", "low", "unrated", "audit", "plan", "lead"];

/// The runners a job can run on.
pub const RUNNERS: [&str; 2] = crate::machine::RUNNERS;

/// What `docket skills KEY` also answers, worked out rather than set, for scripts.
pub const COMPUTED: [(&str, &str); 3] = [
    ("root", "the project's root on this host"),
    ("slug", "the project's slug"),
    (
        "src",
        "the directory holding the docket command this runs from",
    ),
];

/// Refuses a value the loop or a skill could not use. An empty value unsets the fact.
///
/// # Errors
/// The key is no fact, is written by docket, or the value is outside what the fact holds.
pub fn check(key: &str, value: &str) -> Result<(), Refused> {
    if RETIRED.contains(&key) {
        return Err(Refused(format!(
            "{key} was a fact of the old loop and is no longer read"
        )));
    }
    if meaning(key).is_none() {
        let all: Vec<&str> = FACTS.iter().map(|(k, _)| *k).collect();
        return Err(Refused(format!(
            "{key} is not a skill fact. One of: {}",
            all.join(", ")
        )));
    }
    if value.is_empty() {
        return Ok(());
    }
    if key == "releases" {
        return Err(Refused(
            "releases are rows: docket releases add, move and ship change them".to_string(),
        ));
    }
    if WRITTEN.contains(&key) {
        return Err(Refused(format!(
            "{key} is written by docket, not set by hand"
        )));
    }
    if let Some((_, allowed)) = CHOICES.iter().find(|(k, _)| *k == key)
        && !allowed.contains(&value)
    {
        return Err(Refused(format!(
            "{key} is one of {}, not {}",
            allowed.join(", "),
            py_repr(value)
        )));
    }
    check_shape(key, value)
}

/// The numbers and models a fact's value must read as.
fn check_shape(key: &str, value: &str) -> Result<(), Refused> {
    if COUNTS.contains(&key) && !is_count(value) {
        return Err(Refused(format!(
            "{key} is a whole number above 0, not {}",
            py_repr(value)
        )));
    }
    if key == "models" {
        models_of(value).map_err(|why| Refused(format!("models: {why}")))?;
    }
    if key == "prices" {
        prices_of(value).map_err(|why| Refused(format!("prices: {why}")))?;
    }
    if key == "publish" {
        publish_of(value).map_err(|why| Refused(format!("publish: {why}")))?;
    }
    Ok(())
}

/// Where a squash writes and where its result is pushed: the `publish` fact read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Publish {
    /// The local ref a squash writes, such as `published`.
    pub local: String,
    /// The remote ref the owner pushes it to, as `REMOTE/BRANCH`, such as `origin/main`.
    pub remote: String,
}

/// The project's `publish` fact read, `None` when it is unset or does not read: the project does
/// not squash.
#[must_use]
pub fn publish(skills: &BTreeMap<String, String>) -> Option<Publish> {
    skills
        .get("publish")
        .filter(|v| !v.trim().is_empty())
        .and_then(|v| publish_of(v).ok())
}

/// The two refs of a `publish` value, or why it does not read.
///
/// # Errors
/// It is not a local ref and a `REMOTE/BRANCH` ref, or either is no ref git takes.
pub fn publish_of(value: &str) -> Result<Publish, String> {
    let shape = || {
        format!(
            "{} is not a local ref and a remote ref, as 'published origin/main'",
            py_repr(value)
        )
    };
    let words: Vec<&str> = value.split_whitespace().collect();
    let [local, remote] = words[..] else {
        return Err(shape());
    };
    for name in [local, remote] {
        if !is_ref_name(name) {
            return Err(format!("{} is no ref git takes", py_repr(name)));
        }
    }
    if !remote.contains('/') {
        return Err(shape());
    }
    Ok(Publish {
        local: local.to_string(),
        remote: remote.to_string(),
    })
}

/// A branch name git takes: no part empty or starting with a dot, no `..`, `@{`, control
/// character, space or any of `~^:?*[\`, not starting with `-` or ending with `.`, and no part
/// ending in `.lock`.
fn is_ref_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('.')
        && !name.contains("..")
        && !name.contains("@{")
        && name != "@"
        && !name
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
        && name.split('/').all(|part| {
            !part.is_empty() && !part.starts_with('.') && part.strip_suffix(".lock").is_none()
        })
}

/// Digits only, and not all of them zero.
fn is_count(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.bytes().any(|b| b != b'0')
}

/// Input and output price per million tokens, by model name.
pub type Prices = BTreeMap<String, (f64, f64)>;

/// The entries of a `prices` value, or why one does not read.
///
/// # Errors
/// An entry is not `model=input:output` with two non-negative numbers.
pub fn prices_of(value: &str) -> Result<Prices, String> {
    let mut out = Prices::new();
    for entry in value.split_whitespace() {
        let shape = || format!("{} is not model=input:output", py_repr(entry));
        let (model, rest) = entry.split_once('=').ok_or_else(shape)?;
        let (input, output) = rest.split_once(':').ok_or_else(shape)?;
        let price = |n: &str| {
            n.parse::<f64>()
                .ok()
                .filter(|p| p.is_finite() && *p >= 0.0)
                .ok_or_else(shape)
        };
        if model.is_empty() {
            return Err(shape());
        }
        out.insert(model.to_string(), (price(input)?, price(output)?));
    }
    Ok(out)
}

/// What a job runs on: a runner, its model, and the effort when one is named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub runner: String,
    pub model: String,
    pub effort: Option<String>,
}

/// The work a job does, which picks its `models` entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Build,
    Audit,
    Plan,
    /// The session that leads a project and dispatches the rest.
    Lead,
}

/// The entries of a `models` value by key, or why one does not read.
fn models_of(value: &str) -> Result<BTreeMap<String, Model>, String> {
    let mut out = BTreeMap::new();
    for entry in value.split_whitespace() {
        let shape = || format!("{} is not key=runner:model[:effort]", py_repr(entry));
        let (key, rest) = entry.split_once('=').ok_or_else(shape)?;
        let parts: Vec<&str> = rest.splitn(3, ':').collect();
        if key.is_empty() || parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
            return Err(shape());
        }
        if !MODEL_KEYS.contains(&key) {
            return Err(format!(
                "{} names {key}, not one of {}",
                py_repr(entry),
                MODEL_KEYS.join(", ")
            ));
        }
        if !RUNNERS.contains(&parts[0]) {
            return Err(format!(
                "{} runs on {}, not one of {}",
                py_repr(entry),
                parts[0],
                RUNNERS.join(", ")
            ));
        }
        let model = Model {
            runner: parts[0].to_string(),
            model: parts[1].to_string(),
            effort: parts.get(2).map(|e| (*e).to_string()),
        };
        if out.insert(key.to_string(), model).is_some() {
            return Err(format!("{} sets {key} a second time", py_repr(entry)));
        }
    }
    Ok(out)
}

/// What a job runs on, from `models` at the project, owner or built-in level: a build by its ticket's complexity, an unrated
/// one by `unrated` then `medium`; an audit, a plan or a lead by its own entry, then `high`. `None` when no
/// entry applies or `models` does not read.
#[must_use]
pub fn model_for(
    project: &BTreeMap<String, String>,
    owner: &BTreeMap<String, String>,
    role: Role,
    complexity: Option<&str>,
) -> Option<Model> {
    model_without(project, owner, role, complexity, &[])
}

/// `model_for`, never on a runner in `unavailable`: the entry the role and complexity pick when its
/// runner is available, else, when one is picked, the first entry on an available runner, the complexities from high to
/// low and then the audit, plan and lead entries.
#[must_use]
pub fn model_without(
    project: &BTreeMap<String, String>,
    owner: &BTreeMap<String, String>,
    role: Role,
    complexity: Option<&str>,
    unavailable: &[&str],
) -> Option<Model> {
    let (models, _) = layered(project, owner, "models")?;
    let models = models_of(&models).ok()?;
    let order: &[&str] = match (role, complexity) {
        (Role::Audit, _) => &["audit", "high"],
        (Role::Plan, _) => &["plan", "high"],
        (Role::Lead, _) => &["lead", "high"],
        (Role::Build, Some(c @ ("high" | "medium" | "low"))) => &[c][..],
        (Role::Build, _) => &["unrated", "medium"],
    };
    let own = order.iter().find_map(|k| models.get(*k))?;
    if !unavailable.contains(&own.runner.as_str()) {
        return Some(own.clone());
    }
    MODEL_KEYS
        .iter()
        .filter_map(|k| models.get(*k))
        .find(|m| !unavailable.contains(&m.runner.as_str()))
        .cloned()
}

/// What an audit runs on: the `models` entry for audits, unless its runner holds the most
/// assignments under the plan, in which case the first entry on another runner that is available.
/// `runs` counts the plan's assignments by runner. `None` as for `model_without`.
#[must_use]
pub fn audit_model(
    project: &BTreeMap<String, String>,
    owner: &BTreeMap<String, String>,
    runs: &BTreeMap<String, usize>,
    unavailable: &[&str],
) -> Option<Model> {
    let own = model_without(project, owner, Role::Audit, None, unavailable)?;
    let most = runs.values().copied().max().unwrap_or(0);
    if most == 0 || runs.get(&own.runner).copied().unwrap_or(0) < most {
        return Some(own);
    }
    let (models, _) = layered(project, owner, "models")?;
    let models = models_of(&models).ok()?;
    let mut avoid: Vec<&str> = unavailable.to_vec();
    avoid.push(own.runner.as_str());
    let other = MODEL_KEYS
        .iter()
        .filter_map(|k| models.get(*k))
        .find(|m| !avoid.contains(&m.runner.as_str()));
    Some(other.cloned().unwrap_or(own))
}

/// The facts with one set or unset, as stored after the write.
#[must_use]
pub fn with(skills: &BTreeMap<String, String>, key: &str, value: &str) -> BTreeMap<String, String> {
    let mut out = skills.clone();
    if value.is_empty() {
        out.remove(key);
    } else {
        out.insert(key.to_string(), value.to_string());
    }
    out
}

#[cfg(test)]
#[path = "tests/fact.rs"]
mod tests;
