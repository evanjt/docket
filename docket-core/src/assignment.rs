//! Assignments: one row per attempt at an item, a claim by an agent or an ask to the owner, with who
//! acted, when it started and ended, and how a claim ended. `step` is the one rule that turns an
//! event into a row opened or ended; the server applies it as each event is written, and `rebuild`
//! folds it over an item's events, so the rows can always be rebuilt from the history.

use serde::Serialize;
use serde_json::Value;

use crate::dump::{EventDump, ItemDump};

/// The assignee of a claim.
pub const AGENT: &str = "agent";
/// The assignee of an ask.
pub const OWNER: &str = "owner";
pub const OUTCOMES: [&str; 5] = ["landed", "conflict", "gate", "blocked", "failed"];

/// What an attempt is: an agent's claim, or the item handed to the owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Claim,
    Ask,
}

impl Kind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Claim => "claim",
            Kind::Ask => "ask",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "claim" => Some(Kind::Claim),
            "ask" => Some(Kind::Ask),
            _ => None,
        }
    }
}

/// How a claim ended: its work landed, its branch did not merge, its gates failed, it waits on
/// something, or it failed otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Landed,
    Conflict,
    Gate,
    Blocked,
    Failed,
}

impl Outcome {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Landed => "landed",
            Outcome::Conflict => "conflict",
            Outcome::Gate => "gate",
            Outcome::Blocked => "blocked",
            Outcome::Failed => "failed",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "landed" => Some(Outcome::Landed),
            "conflict" => Some(Outcome::Conflict),
            "gate" => Some(Outcome::Gate),
            "blocked" => Some(Outcome::Blocked),
            "failed" => Some(Outcome::Failed),
            _ => None,
        }
    }
}

/// One attempt at an item. No end is the attempt under way now. The actor, tokens and cost are
/// known only for rows written as the attempt happened.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Assignment {
    pub assignee: String,
    pub kind: Kind,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub outcome: Option<Outcome>,
    pub note: Option<String>,
    pub actor: Option<String>,
    pub branch: Option<String>,
    pub host: String,
    pub machine: Option<String>,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub role: Option<String>,
    pub job: Option<String>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub cost_reported: Option<f64>,
    /// What an ask needs of the owner: one of `queue::NEEDS`.
    pub need: Option<String>,
}

/// An event on one item, as the rule reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Event {
    pub kind: String,
    pub at: String,
    pub host: String,
    pub branch: Option<String>,
    pub note: Option<String>,
    pub data: Option<Value>,
}

impl From<&EventDump> for Event {
    fn from(e: &EventDump) -> Self {
        Event {
            kind: e.kind.clone(),
            at: e.at.clone(),
            host: e.host.clone(),
            branch: e.branch.clone(),
            note: e.note.clone(),
            data: e.data.as_deref().and_then(|d| serde_json::from_str(d).ok()),
        }
    }
}

/// How the open row ends: its outcome, and the note the event gives it when it gives one.
#[derive(Clone, Debug, PartialEq)]
pub struct End {
    pub outcome: Option<Outcome>,
    pub note: Option<String>,
}

/// What a finished job used, as its report states it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub cost_reported: Option<f64>,
}

/// What one event does: the open row ended, then a row opened, or the open claim given the usage of
/// its job.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Step {
    pub end: Option<End>,
    pub open: Option<Assignment>,
    pub usage: Option<Usage>,
}

/// What an event does to the item's assignments, given the kind of its open row.
///
/// A claim opens a claim row and ends one left open. A release ends it with the outcome it names, or
/// the one its note reads as; a close ends it landed, a drop or a lost claim failed, and a wait or an
/// ask by the holder blocked. An ask opens a row on the owner with its note, and a reply, an answer,
/// a resume, a close or a drop hands the item back. An item opened on the owner's turn is asked
/// from the start.
#[must_use]
pub fn step(e: &Event, open: Option<Kind>) -> Step {
    if e.kind == "job_reported" {
        return Step {
            usage: (open == Some(Kind::Claim)).then(|| usage_of(e)),
            ..Step::default()
        };
    }
    let ended = |outcome: Option<Outcome>, note: Option<String>| {
        open.map(|kind| End {
            outcome: if kind == Kind::Claim { outcome } else { None },
            note,
        })
    };
    let only_claim = |outcome| match open {
        Some(Kind::Claim) => ended(Some(outcome), None),
        _ => None,
    };
    let only_ask = || match open {
        Some(Kind::Ask) => ended(None, None),
        _ => None,
    };
    match e.kind.as_str() {
        "claimed" => Step {
            end: ended(Some(Outcome::Failed), None),
            open: Some(claim_row(e)),
            ..Step::default()
        },
        "released" => Step {
            end: match open {
                Some(Kind::Claim) => ended(Some(released(e)), e.note.clone()),
                _ => None,
            },
            open: None,
            ..Step::default()
        },
        "closed" => Step {
            end: ended(Some(Outcome::Landed), None),
            open: None,
            ..Step::default()
        },
        "dropped" => Step {
            end: ended(Some(Outcome::Failed), None),
            open: None,
            ..Step::default()
        },
        "claim_lost" => Step {
            end: only_claim(Outcome::Failed),
            open: None,
            ..Step::default()
        },
        "waited" => Step {
            end: only_claim(Outcome::Blocked),
            open: None,
            ..Step::default()
        },
        "asked" => Step {
            end: ended(Some(Outcome::Blocked), None),
            open: Some(Assignment {
                assignee: OWNER.to_string(),
                kind: Kind::Ask,
                note: e.note.clone(),
                need: field(e, "need"),
                ..row(e)
            }),
            ..Step::default()
        },
        "opened" if field(e, "turn").as_deref() == Some("user") => Step {
            end: None,
            open: Some(Assignment {
                assignee: OWNER.to_string(),
                kind: Kind::Ask,
                ..row(e)
            }),
            ..Step::default()
        },
        "replied" | "resumed" => Step {
            end: only_ask(),
            open: None,
            ..Step::default()
        },
        "decided" if field(e, "derived").is_none() => Step {
            end: only_ask(),
            open: None,
            ..Step::default()
        },
        _ => Step::default(),
    }
}

fn usage_of(e: &Event) -> Usage {
    let get = |name: &str| e.data.as_ref().and_then(|d| d.get(name));
    Usage {
        tokens_in: get("tokens_in").and_then(Value::as_i64),
        tokens_out: get("tokens_out").and_then(Value::as_i64),
        cost_reported: get("cost_reported").and_then(Value::as_f64),
    }
}

/// A row started by the event, on the agent, with nothing else known.
fn row(e: &Event) -> Assignment {
    Assignment {
        assignee: AGENT.to_string(),
        kind: Kind::Claim,
        started_at: e.at.clone(),
        ended_at: None,
        outcome: None,
        note: None,
        actor: None,
        branch: e.branch.clone(),
        host: e.host.clone(),
        machine: None,
        runner: None,
        model: None,
        effort: None,
        role: None,
        job: None,
        tokens_in: None,
        tokens_out: None,
        cost_reported: None,
        need: None,
    }
}

/// A claim row with the run its event names.
fn claim_row(e: &Event) -> Assignment {
    Assignment {
        machine: field(e, "machine"),
        runner: field(e, "runner"),
        model: field(e, "model"),
        effort: field(e, "effort"),
        role: field(e, "role"),
        job: field(e, "job"),
        ..row(e)
    }
}

fn field(e: &Event, name: &str) -> Option<String> {
    e.data
        .as_ref()
        .and_then(|d| d.get(name))
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

/// How a release ended its claim: the outcome it names, else a branch sent to be rebased is a
/// conflict, else what its note says.
fn released(e: &Event) -> Outcome {
    if let Some(o) = field(e, "outcome").as_deref().and_then(Outcome::parse) {
        return o;
    }
    if field(e, "rebase").is_some() {
        return Outcome::Conflict;
    }
    outcome_of(e.note.as_deref().unwrap_or_default())
}

/// The outcome a release note names in words, for releases written before the outcome was recorded.
#[must_use]
pub fn outcome_of(note: &str) -> Outcome {
    let note = note.to_lowercase();
    let says = |words: &[&str]| words.iter().any(|w| note.contains(w));
    if says(&["conflict", "did not land", "failed to land", "rebase"]) {
        Outcome::Conflict
    } else if says(&["gate", "clippy"]) {
        Outcome::Gate
    } else if says(&["blocked", "waits on", "waiting on"]) {
        Outcome::Blocked
    } else {
        Outcome::Failed
    }
}

/// The rows an item's events, oldest first, leave.
#[must_use]
pub fn rebuild(events: &[Event]) -> Vec<Assignment> {
    let mut rows: Vec<Assignment> = Vec::new();
    for e in events {
        let open = rows.last().filter(|a| a.ended_at.is_none()).map(|a| a.kind);
        let s = step(e, open);
        if let Some(end) = s.end
            && let Some(a) = rows.last_mut()
        {
            a.ended_at = Some(e.at.clone());
            a.outcome = end.outcome;
            if end.note.is_some() {
                a.note = end.note;
            }
        }
        if let (Some(u), Some(a)) = (s.usage, rows.last_mut()) {
            (a.tokens_in, a.tokens_out, a.cost_reported) =
                (u.tokens_in, u.tokens_out, u.cost_reported);
        }
        rows.extend(s.open);
    }
    rows
}

/// An item's claim as its columns hold it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Claim {
    pub branch: String,
    pub host: String,
    pub since: String,
    pub runner: Option<String>,
    pub job: Option<String>,
    pub on: Option<String>,
}

/// What an item holds now: its claim, when the owner was asked and with what, and its last update.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Now {
    pub claim: Option<Claim>,
    pub asked: Option<(String, Option<String>)>,
    pub updated_at: String,
}

impl Now {
    /// What a dumped item holds now.
    #[must_use]
    pub fn of(i: &ItemDump) -> Self {
        let claim = match (&i.claim_branch, &i.claim_host) {
            (Some(branch), Some(host)) => Some(Claim {
                branch: branch.clone(),
                host: host.clone(),
                since: i
                    .claim_since
                    .clone()
                    .unwrap_or_else(|| i.updated_at.clone()),
                runner: i.claim_runner.clone(),
                job: i.claim_job.clone(),
                on: i.claim_on.clone(),
            }),
            _ => None,
        };
        let asked = (i.state == "open" && i.turn.as_deref() == Some("user")).then(|| {
            (
                i.asked_at.clone().unwrap_or_else(|| i.opened_at.clone()),
                i.turn_note.clone(),
            )
        });
        Now {
            claim,
            asked,
            updated_at: i.updated_at.clone(),
        }
    }
}

/// The rebuilt rows made to agree with the item as it stands: a row the item no longer holds ends at
/// its last update with no known outcome, and a claim or an ask the events never opened is opened
/// from the item's own columns.
pub fn settle(rows: &mut Vec<Assignment>, now: &Now) {
    let holds = |kind| match kind {
        Kind::Claim => now.claim.is_some(),
        Kind::Ask => now.asked.is_some(),
    };
    if let Some(a) = rows
        .last_mut()
        .filter(|a| a.ended_at.is_none() && !holds(a.kind))
    {
        a.ended_at = Some(now.updated_at.clone());
    }
    if rows.last().is_some_and(|a| a.ended_at.is_none()) {
        return;
    }
    let base = |at: &str, host: &str| {
        row(&Event {
            at: at.to_string(),
            host: host.to_string(),
            ..Event::default()
        })
    };
    if let Some(c) = &now.claim {
        rows.push(Assignment {
            branch: Some(c.branch.clone()),
            machine: c.on.clone(),
            runner: c.runner.clone(),
            job: c.job.clone(),
            ..base(&c.since, &c.host)
        });
    } else if let Some((at, note)) = &now.asked {
        rows.push(Assignment {
            assignee: OWNER.to_string(),
            kind: Kind::Ask,
            note: note.clone(),
            ..base(at, "")
        });
    }
}

#[cfg(test)]
#[path = "tests/assignment.rs"]
mod tests;
