//! A project's facts: what each one means, its default, and which the loop needs before it dispatches.

use std::collections::BTreeMap;

use crate::item::Refused;
use crate::text::py_repr;

/// Every fact a project carries, in the order `docket skills` prints them, with its one-line meaning.
pub const FACTS: [(&str, &str); 36] = [
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
        "release",
        "the release that open work with no held theme makes up, its name and cut date (1.0.0 2026-10-01). Set, next lists only that work unless --all",
    ),
    (
        "remote",
        "the ssh host a fleet job builds on when the pool names none, whose home mirrors this one",
    ),
    (
        "checkout",
        "the repository under the root that the fleet mirrors and builds in, . for the root itself",
    ),
    (
        "mirror_exclude",
        "rsync patterns the mirror leaves out of the checkout, separated by spaces",
    ),
    (
        "remote_prepare",
        "shell run in the remote checkout after each mirror: dependencies, links",
    ),
    (
        "remote_setup",
        "shell run in each fresh remote worktree before a job starts in it",
    ),
    (
        "land",
        "the command that lands a collected branch, $BRANCH in it; a non-zero exit is a failed landing",
    ),
    (
        "gates",
        "the checks a job reruns after merging main, in words a job can follow",
    ),
    (
        "slice",
        "the systemd user slice a job runs in, to cap its memory",
    ),
    (
        "mode",
        "what the loop does: run dispatches, drain lets running jobs finish and dispatches none, pause does neither",
    ),
    (
        "pool",
        "job slots per host, as \"local=4 devbox=16\"; local is this machine, any other name an ssh host",
    ),
    (
        "pool_max",
        "the most slots the pool may be set to, by the owner only, so an agent driving the loop stays under it",
    ),
    (
        "model_build",
        "the model and effort a ticket is built on, as \"gpt-5.4 medium\"; a claude model runs on Claude Code",
    ),
    (
        "model_review",
        "the model and effort a package's review runs on",
    ),
    ("model_plan", "the model and effort the planner runs on"),
    (
        "file_cap",
        "how many live tickets may touch one file at once",
    ),
    (
        "lanes",
        "paths one ticket at a time may change, separated by spaces; a path ending in / is a directory",
    ),
    (
        "packages_live",
        "how many packages may be under way at once, so they finish rather than all start",
    ),
    (
        "ram_floor",
        "megabytes a host must have free to take another job",
    ),
    (
        "job_timeout",
        "minutes a job may run before the loop cancels it and releases its ticket",
    ),
    (
        "observe_cap",
        "how many observations one job may file to the inbox",
    ),
    (
        "plan_batch",
        "how many inbox tickets and open questions one planner job takes, and how full the inbox gets before one runs",
    ),
    (
        "jobs_per_day",
        "jobs the loop dispatches in a day before it pauses itself",
    ),
    (
        "brake_ratio",
        "observations filed per ticket closed, over the last hour, above which the loop pauses itself",
    ),
    (
        "loop_host",
        "the one host whose loop dispatches for this project",
    ),
    (
        "stale_claim",
        "minutes with no event on a claimed item before wip and the watch flag the claim",
    ),
    ("poll", "seconds between ticks of the loop"),
    (
        "flow",
        "the work model the project is on, set by its migration",
    ),
    (
        "paused_by",
        "why the loop paused itself, written by a brake and cleared by docket pool run",
    ),
    (
        "last_tick",
        "when the loop last ran on this host, written by the loop into this host's database",
    ),
];

/// The value a fact takes when the project sets none.
pub const DEFAULTS: [(&str, &str); 13] = [
    ("owner", "the owner"),
    ("checkout", "."),
    ("stale_claim", "120"),
    ("poll", "60"),
    ("mode", "pause"),
    ("pool", "local=2"),
    ("file_cap", "2"),
    ("packages_live", "4"),
    ("ram_floor", "2048"),
    ("job_timeout", "120"),
    ("observe_cap", "3"),
    ("brake_ratio", "0.5"),
    ("plan_batch", "20"),
];

/// Facts docket writes itself, never set by hand.
pub const WRITTEN: [&str; 3] = ["flow", "paused_by", "last_tick"];

/// Facts without which the loop cannot dispatch every role: a landing, and a model per role.
pub const NEEDED: [&str; 4] = ["land", "model_build", "model_review", "model_plan"];

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

/// Whether a fact, as it stands, keeps the loop from dispatching: a needed one unset, or a mode other
/// than run.
#[must_use]
pub fn blocks(skills: &BTreeMap<String, String>, key: &str) -> bool {
    if key == "mode" {
        return effective(skills, key).as_deref() != Some("run");
    }
    if key == "paused_by" {
        return matches!(value(skills, key), Value::Set(_));
    }
    NEEDED.contains(&key) && value(skills, key) == Value::Unset
}

/// Why the loop dispatches nothing, a phrase per reason in the order a fix takes them; empty when the
/// facts let it run.
#[must_use]
pub fn gaps(skills: &BTreeMap<String, String>) -> Vec<String> {
    let mut out = Vec::new();
    if let Value::Set(why) = value(skills, "paused_by") {
        out.push(format!("the loop paused itself: {why}"));
    }
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

/// The values a fact may take, checked when it is set so the loop never reads a typo.
pub const CHOICES: [(&str, &[&str]); 1] = [("mode", &["run", "drain", "pause"])];

/// Facts that hold a whole number above 0.
pub const COUNTS: [&str; 10] = [
    "stale_claim",
    "poll",
    "pool_max",
    "file_cap",
    "packages_live",
    "ram_floor",
    "job_timeout",
    "observe_cap",
    "jobs_per_day",
    "plan_batch",
];

/// Facts that hold a model and an optional effort.
pub const MODELS: [&str; 3] = ["model_build", "model_review", "model_plan"];

/// Facts only the owner's key sets.
pub const OWNER_ONLY: [&str; 1] = ["pool_max"];

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

/// The numbers, models and pools a fact's value must read as.
fn check_shape(key: &str, value: &str) -> Result<(), Refused> {
    if COUNTS.contains(&key) && !is_count(value) {
        return Err(Refused(format!(
            "{key} is a whole number above 0, not {}",
            py_repr(value)
        )));
    }
    if MODELS.contains(&key) && !(1..=2).contains(&value.split_whitespace().count()) {
        return Err(Refused(format!(
            "{key} is a model and an optional effort, as \"gpt-5.4 medium\", not {}",
            py_repr(value)
        )));
    }
    if key == "brake_ratio" && value.trim().parse::<f64>().is_err() {
        return Err(Refused(format!(
            "brake_ratio is a number, as 0.5, not {}",
            py_repr(value)
        )));
    }
    if key == "pool" {
        pool_of(value)?;
    }
    Ok(())
}

/// Digits only, and not all of them zero.
fn is_count(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.bytes().any(|b| b != b'0')
}

/// Slots per host from a pool fact, refused when any pair does not read as `host=slots`.
///
/// # Errors
/// A pair with no `=`, no host, or slots that are not digits.
pub fn pool_of(value: &str) -> Result<Vec<(String, u128)>, Refused> {
    let mut out = Vec::new();
    for part in value.split_whitespace() {
        let (host, n) = part.split_once('=').unwrap_or((part, ""));
        let digits = !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit());
        if host.is_empty() || !part.contains('=') || !digits {
            return Err(Refused(format!(
                "pool is host=slots pairs separated by spaces, as \"local=4 devbox=16\", not {}",
                py_repr(value)
            )));
        }
        out.push((host.to_string(), n.parse().unwrap_or(u128::MAX)));
    }
    Ok(out)
}

/// Refuses a pool, or a ceiling, that would put the pool's slots above `pool_max`.
///
/// # Errors
/// The slots the pool would hold exceed the ceiling.
pub fn ceiling(skills: &BTreeMap<String, String>, key: &str, value: &str) -> Result<(), Refused> {
    if value.is_empty() || (key != "pool" && key != "pool_max") {
        return Ok(());
    }
    let stored = |k: &str| skills.get(k).map_or("", String::as_str);
    let cap = if key == "pool_max" {
        value
    } else {
        stored("pool_max")
    };
    if cap.is_empty() {
        return Ok(());
    }
    let pool = if key == "pool" { value } else { stored("pool") };
    let slots: u128 = pool_of(pool)?
        .iter()
        .fold(0, |sum, (_, n)| sum.saturating_add(*n));
    if slots > cap.parse().unwrap_or(u128::MAX) {
        return Err(Refused(format!(
            "the pool would hold {slots} slots, above pool_max {cap}, the owner's ceiling"
        )));
    }
    Ok(())
}

/// The refusal a fleet job's key meets when it sets a fact the owner keeps.
#[must_use]
pub fn owner_only(key: &str) -> Option<String> {
    OWNER_ONLY.contains(&key).then(|| {
        format!("{key} is the owner's ceiling on the pool, and a fleet job does not set it")
    })
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

/// Slots per host from a pool fact, `local=4 devbox=16`; a pair that does not read is skipped.
#[must_use]
pub fn pool(text: &str) -> Vec<(String, u64)> {
    text.split_whitespace()
        .filter_map(|part| {
            let (host, n) = part.split_once('=')?;
            Some((host.to_string(), n.parse().ok()?))
        })
        .filter(|(host, _)| !host.is_empty())
        .collect()
}

#[cfg(test)]
#[path = "tests/fact.rs"]
mod tests;
