//! A project's facts: what each one means, its default, and which the loop needs before it dispatches.

use std::collections::BTreeMap;

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
