//! A project's facts: what each one means, its default, and which a lead needs before it dispatches.

use std::collections::BTreeMap;

use crate::item::Refused;
use crate::text::py_repr;

/// Every fact a project carries, in the order `docket skills` prints them, with its one-line meaning.
pub const FACTS: [(&str, &str); 15] = [
    ("owner", "the owner's name, as the skills address them"),
    (
        "worktree",
        "the command that makes a worktree for item B14 on branch audit/b14-$n",
    ),
    ("merge", "the command that merges audit/b14-$n back"),
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
        "the releases in the order they ship, the current first, as \"1.0 1.1 2.0\"; an item's theme names its release, and work with no theme or a theme not listed is the current release's. next orders by release, then priority",
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

/// The value a fact takes when the project sets none.
pub const DEFAULTS: [(&str, &str); 6] = [
    ("owner", "the owner"),
    ("checkout", "."),
    ("mode", "run"),
    ("job_timeout", "120"),
    ("stale_claim", "120"),
    ("lead_lapse", "10"),
];

/// Facts docket writes itself, never set by hand.
pub const WRITTEN: [&str; 1] = ["flow"];

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
pub fn gaps(skills: &BTreeMap<String, String>) -> Vec<String> {
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
pub const COUNTS: [&str; 3] = ["stale_claim", "job_timeout", "lead_lapse"];

/// The keys a `models` entry may name: a build's complexity, then the audit, plan and lead roles.
pub const MODEL_KEYS: [&str; 7] = ["high", "medium", "low", "unrated", "audit", "plan", "lead"];

/// The runners a job can run on.
pub const RUNNERS: [&str; 2] = ["claude", "codex"];

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
    if key == "releases" {
        let names = releases(Some(value));
        if let Some(twice) = names
            .iter()
            .enumerate()
            .find_map(|(i, n)| names[..i].contains(n).then_some(n))
        {
            return Err(Refused(format!("releases names {twice} twice")));
        }
    }
    Ok(())
}

/// The releases a `releases` value lists, in the order they ship.
#[must_use]
pub fn releases(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// The theme an item filed for a release carries: `current` is the first release, and none while the
/// project lists none; a listed release, or a theme items already carry (an area outside the releases,
/// which ranks with the current release), is itself.
///
/// # Errors
/// Refused when the name is empty, or is neither a release nor a theme in use.
pub fn release_theme(
    given: &str,
    listed: &[String],
    in_use: bool,
) -> Result<Option<String>, Refused> {
    let given = given.trim();
    let choices = if listed.is_empty() {
        String::new()
    } else {
        format!(" one of {},", listed.join(" "))
    };
    if given.is_empty() {
        return Err(Refused(format!(
            "a release is current,{choices} or a theme items already carry"
        )));
    }
    if given == "current" {
        return Ok(listed.first().cloned());
    }
    if in_use || listed.iter().any(|r| r == given) {
        return Ok(Some(given.to_string()));
    }
    Err(Refused(format!(
        "{given} is neither a release nor a theme in use here: give current,{choices} or a theme items already carry"
    )))
}

/// Digits only, and not all of them zero.
fn is_count(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.bytes().any(|b| b != b'0')
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

/// What a job runs on, from the project's `models`: a build by its ticket's complexity, an unrated
/// one by `unrated` then `medium`; an audit, a plan or a lead by its own entry, then `high`. `None` when no
/// entry applies or `models` does not read.
#[must_use]
pub fn model_for(
    skills: &BTreeMap<String, String>,
    role: Role,
    complexity: Option<&str>,
) -> Option<Model> {
    let models = models_of(skills.get("models")?).ok()?;
    let order: &[&str] = match (role, complexity) {
        (Role::Audit, _) => &["audit", "high"],
        (Role::Plan, _) => &["plan", "high"],
        (Role::Lead, _) => &["lead", "high"],
        (Role::Build, Some(c @ ("high" | "medium" | "low"))) => &[c][..],
        (Role::Build, _) => &["unrated", "medium"],
    };
    order.iter().find_map(|k| models.get(*k).cloned())
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
