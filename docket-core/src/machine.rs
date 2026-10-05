//! The machines jobs run on, as the owner records them on the server: a name, an address, slots and
//! the runners it has. Nothing here names a real machine; every one is data.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::item::Refused;
use crate::text::py_repr;

/// The agents a machine can run a job on.
pub const RUNNERS: [&str; 2] = ["claude", "codex"];

/// The most jobs one machine is set to run at once.
pub const MAX_SLOTS: i64 = 64;

/// One machine as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    /// The host its key names, so a client knows the row of the machine it runs on.
    pub name: String,
    /// The ssh address the other machines reach it at.
    pub ssh: String,
    pub slots: i64,
    pub runners: Vec<String>,
    pub note: Option<String>,
    pub updated_at: String,
    /// Each runner that reported a usage limit here, with the reset it named, as a stamp. A limit
    /// the clock has passed is still listed until it is next written over.
    #[serde(default)]
    pub limits: BTreeMap<String, String>,
    /// The directories put before `PATH` in the command run on it over ssh, as the shell reads
    /// them; none means the user-install directories.
    #[serde(default)]
    pub path: Option<String>,
}

impl Machine {
    /// The reset of the runner's usage limit when it is still in force at `now`, a stamp.
    #[must_use]
    pub fn limited(&self, runner: &str, now: &str) -> Option<&str> {
        self.limits
            .get(runner)
            .map(String::as_str)
            .filter(|until| *until > now)
    }
}

/// A usage limit one runner reported on one machine: `POST /do/limit`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limit {
    pub machine: String,
    pub runner: String,
    /// The reset the runner named, as a stamp.
    pub until: String,
}

/// The reset a usage-limit message names, as a stamp: the last `resets YYYY-MM-DD HH:MM` in a
/// message that speaks of a limit, read as UTC. `None` for a message with no limit or no such date.
#[must_use]
pub fn reset_of(text: &str) -> Option<String> {
    if !text.to_lowercase().contains("limit") {
        return None;
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    words
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, w)| {
            w.trim_matches(|c: char| !c.is_alphabetic())
                .eq_ignore_ascii_case("resets")
        })
        .find_map(|(i, _)| {
            let date = words.get(i + 1)?;
            let (date, time) = match date.split_once('T') {
                Some((d, t)) => (d, t),
                None => (*date, *words.get(i + 2)?),
            };
            stamp_of(date, time.trim_end_matches(|c: char| !c.is_ascii_digit()))
        })
}

/// `2026-10-07` and `18:29` as `2026-10-07T18:29:00Z`, `None` unless both are in that shape.
fn stamp_of(date: &str, time: &str) -> Option<String> {
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    let d: Vec<&str> = date.split('-').collect();
    let t: Vec<&str> = time.split(':').collect();
    let ok = d.len() == 3
        && digits(d[0], 4)
        && digits(d[1], 2)
        && digits(d[2], 2)
        && t.len() == 2
        && digits(t[0], 2)
        && digits(t[1], 2);
    ok.then(|| format!("{date}T{time}:00Z"))
}

/// The reset a new report leaves: the later of it and the one stored.
#[must_use]
pub fn later(stored: Option<&str>, reported: &str) -> String {
    stored
        .filter(|s| *s > reported)
        .unwrap_or(reported)
        .to_string()
}

/// `docket machine set`: the fields given, the rest kept from the stored row.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Set {
    pub name: String,
    #[serde(default)]
    pub ssh: Option<String>,
    #[serde(default)]
    pub slots: Option<i64>,
    #[serde(default)]
    pub runners: Option<Vec<String>>,
    /// An empty note clears it.
    #[serde(default)]
    pub note: Option<String>,
    /// An empty path clears it.
    #[serde(default)]
    pub path: Option<String>,
}

/// The machine a set leaves: the stored one with what the set names, or a new one from the set alone.
///
/// # Errors
/// A new machine missing its address, slots or runners, or a value `check` refuses.
pub fn merged(stored: Option<&Machine>, set: &Set, now: &str) -> Result<Machine, Refused> {
    let out = if let Some(m) = stored {
        Machine {
            name: m.name.clone(),
            ssh: set.ssh.clone().unwrap_or_else(|| m.ssh.clone()),
            slots: set.slots.unwrap_or(m.slots),
            runners: set.runners.clone().unwrap_or_else(|| m.runners.clone()),
            note: set.note.clone().map_or_else(|| m.note.clone(), non_empty),
            updated_at: now.to_string(),
            limits: m.limits.clone(),
            path: set.path.clone().map_or_else(|| m.path.clone(), non_empty),
        }
    } else {
        new(set, now)?
    };
    check(&out)?;
    Ok(out)
}

/// A machine from a set alone, which must name its address, slots and runners.
fn new(set: &Set, now: &str) -> Result<Machine, Refused> {
    let missing: Vec<&str> = [
        ("ssh", set.ssh.is_none()),
        ("slots", set.slots.is_none()),
        ("runners", set.runners.is_none()),
    ]
    .into_iter()
    .filter_map(|(field, absent)| absent.then_some(field))
    .collect();
    if !missing.is_empty() {
        return Err(Refused(format!(
            "a new machine needs --ssh, --slots and --runners; {} has no {}",
            set.name,
            missing.join(", ")
        )));
    }
    Ok(Machine {
        name: set.name.clone(),
        ssh: set.ssh.clone().unwrap_or_default(),
        slots: set.slots.unwrap_or_default(),
        runners: set.runners.clone().unwrap_or_default(),
        note: set.note.clone().and_then(non_empty),
        updated_at: now.to_string(),
        limits: BTreeMap::new(),
        path: set.path.clone().and_then(non_empty),
    })
}

fn non_empty(text: String) -> Option<String> {
    (!text.trim().is_empty()).then_some(text)
}

/// Refuses a machine a lead could not reach or dispatch to.
///
/// # Errors
/// No name or a name of more than one word, no address, slots outside 1 to 64, no runner, a runner
/// docket does not run, or one named twice.
pub fn check(m: &Machine) -> Result<(), Refused> {
    let name = &m.name;
    if name.is_empty() {
        return Err(Refused("a machine needs a name".into()));
    }
    if name.split_whitespace().count() != 1 || name.trim() != name {
        return Err(Refused(format!(
            "a machine's name is one word, the host its key names, not {}",
            py_repr(name)
        )));
    }
    if m.ssh.trim().is_empty() || m.ssh.split_whitespace().count() != 1 {
        return Err(Refused(format!(
            "{name} needs an ssh address the other machines reach it at"
        )));
    }
    if m.path
        .as_deref()
        .is_some_and(|p| p.contains(['"', '`', '\n']))
    {
        return Err(Refused(format!(
            "{name}'s path is directories for PATH, with no double quote, backtick or newline"
        )));
    }
    if !(1..=MAX_SLOTS).contains(&m.slots) {
        return Err(Refused(format!(
            "{name}'s slots are 1 to {MAX_SLOTS}, not {}",
            m.slots
        )));
    }
    if m.runners.is_empty() {
        return Err(Refused(format!(
            "{name} needs at least one runner: {}",
            RUNNERS.join(", ")
        )));
    }
    for (i, r) in m.runners.iter().enumerate() {
        if !RUNNERS.contains(&r.as_str()) {
            return Err(Refused(format!(
                "{name}'s runners are {}, not {}",
                RUNNERS.join(", "),
                py_repr(r)
            )));
        }
        if m.runners[..i].contains(r) {
            return Err(Refused(format!("{name} names {r} twice")));
        }
    }
    Ok(())
}

/// Runners from `claude,codex` as the command line takes them; empty entries are skipped.
#[must_use]
pub fn runners_of(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
#[path = "tests/machine.rs"]
mod tests;
