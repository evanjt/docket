//! Assignments: one row per attempt at an item, a claim by an agent or an ask to the owner, with who
//! acted, when it started and ended, and how a claim ended. `step` is the one rule that turns an
//! event into a row opened or ended; the server applies it as each event is written, and `rebuild`
//! folds it over an item's events, so the rows can always be rebuilt from the history.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dump::EventDump;
use crate::migrate::OldItem;

/// The assignee of a claim.
pub const AGENT: &str = "agent";
/// The assignee of an ask.
pub const OWNER: &str = "owner";
/// The failed attempts at an item the project allows before the item is assigned to the owner.
pub const FAILURE_LIMIT: usize = 2;
/// How many of the project's last ended attempts must all have failed for the queue to report a halt.
pub const HALT_RUN: usize = 3;
/// Who an assignment is given to: the agent or the owner.
pub const ASSIGNEES: [&str; 2] = [AGENT, OWNER];
/// Whose turn an item is on.
pub const TURNS: [&str; 2] = ["agent", "user"];
/// The runners a claim may name: the agents a machine runs, and a job started elsewhere.
pub const CLAIM_RUNNERS: [&str; 3] = ["codex", "claude", "remote"];
/// The value of a resume's `ask` field when the owner was asked during the wait, which still stands.
pub const KEPT: &str = "kept";
/// The roles an attempt is made in.
pub const ROLES: [&str; 5] = ["build", "rebase", "review", "plan", "audit"];
pub const OUTCOMES: [&str; 5] = ["landed", "conflict", "gate", "blocked", "failed"];

/// What an attempt is: an agent's claim, or the item handed to the owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// When the job began and ended and how it exited, as its report states them.
    pub job_started_at: Option<String>,
    pub job_ended_at: Option<String>,
    pub job_exit: Option<i32>,
    /// What an ask needs of the owner: one of `queue::NEEDS`.
    pub need: Option<String>,
}

/// Whether an item with `failed` failed attempts is the owner's now, at the project's `limit`.
#[must_use]
pub fn past_failure_limit(failed: usize, limit: usize) -> bool {
    failed >= limit
}

/// Why the queue reports a halt: the project's last ended attempts, newest first, all failed.
#[must_use]
pub fn halt(newest_first: &[Outcome]) -> Option<String> {
    (newest_first.len() >= HALT_RUN
        && newest_first[..HALT_RUN]
            .iter()
            .all(|o| *o == Outcome::Failed))
    .then(|| {
        format!(
            "the last {HALT_RUN} ended attempts all failed: look at why before dispatching more"
        )
    })
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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub cost_reported: Option<f64>,
    pub job_started_at: Option<String>,
    pub job_ended_at: Option<String>,
    pub job_exit: Option<i32>,
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
/// a resume, a close or a drop hands the item back; a resume that keeps an ask made during the wait
/// leaves it open. An answer ends a claim on the question landed. An item opened on the owner's turn
/// is asked from the start.
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
        "resumed" if field(e, "ask").as_deref() == Some(KEPT) => Step::default(),
        "replied" | "resumed" => Step {
            end: only_ask(),
            open: None,
            ..Step::default()
        },
        "decided" if field(e, "derived").is_none() => Step {
            end: match open {
                Some(Kind::Claim) => ended(Some(Outcome::Landed), None),
                _ => only_ask(),
            },
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
        job_started_at: field(e, "start"),
        job_ended_at: field(e, "end"),
        job_exit: get("exit")
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok()),
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
        job_started_at: None,
        job_ended_at: None,
        job_exit: None,
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
            a.tokens_in = u.tokens_in.or(a.tokens_in);
            a.tokens_out = u.tokens_out.or(a.tokens_out);
            a.cost_reported = u.cost_reported.or(a.cost_reported);
            a.job_started_at = u.job_started_at.or(a.job_started_at.take());
            a.job_ended_at = u.job_ended_at.or(a.job_ended_at.take());
            a.job_exit = u.job_exit.or(a.job_exit);
        }
        rows.extend(s.open);
    }
    rows
}

/// An item's open claim: the branch and host that hold it, since when, and the job that runs it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub branch: String,
    pub host: String,
    pub since: String,
    pub runner: Option<String>,
    pub job: Option<String>,
    pub on: Option<String>,
}

/// The owner's open ask: since when, and what it needs of them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ask {
    pub since: String,
    pub note: Option<String>,
}

/// Who holds an item now, as its open assignment row says: an agent's claim or the owner's ask.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Held {
    Claim(Claim),
    Ask(Ask),
}

impl Held {
    /// The held row of an open assignment, from its stored columns.
    #[must_use]
    pub fn of(kind: Kind, a: &Assignment) -> Self {
        match kind {
            Kind::Claim => Held::Claim(Claim {
                branch: a.branch.clone().unwrap_or_default(),
                host: a.host.clone(),
                since: a.started_at.clone(),
                runner: a.runner.clone(),
                job: a.job.clone(),
                on: a.machine.clone(),
            }),
            Kind::Ask => Held::Ask(Ask {
                since: a.started_at.clone(),
                note: a.note.clone(),
            }),
        }
    }

    #[must_use]
    pub fn kind(&self) -> Kind {
        match self {
            Held::Claim(_) => Kind::Claim,
            Held::Ask(_) => Kind::Ask,
        }
    }
}

/// The claim and turn an item's rows carry under the names of the columns that once held them,
/// filled from its open assignment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldFields {
    pub turn: Option<String>,
    pub turn_note: Option<String>,
    pub asked_at: Option<String>,
    pub claim_branch: Option<String>,
    pub claim_host: Option<String>,
    pub claim_since: Option<String>,
    pub claim_runner: Option<String>,
    pub claim_job: Option<String>,
    pub claim_on: Option<String>,
}

impl HeldFields {
    /// The fields of an item in `state` held as `held` says.
    #[must_use]
    pub fn of(state: &str, held: Option<&Held>) -> Self {
        let turn = crate::item::turn_of(state, matches!(held, Some(Held::Ask(_))));
        let mut out = HeldFields {
            turn: turn.map(str::to_string),
            ..HeldFields::default()
        };
        match held {
            Some(Held::Claim(c)) => {
                out.claim_branch = Some(c.branch.clone());
                out.claim_host = Some(c.host.clone());
                out.claim_since = Some(c.since.clone());
                out.claim_runner.clone_from(&c.runner);
                out.claim_job.clone_from(&c.job);
                out.claim_on.clone_from(&c.on);
            }
            Some(Held::Ask(a)) => {
                out.turn_note.clone_from(&a.note);
                out.asked_at = Some(a.since.clone());
            }
            None => {}
        }
        out
    }
}

/// What an item's row before the drop holds now: its claim, when the owner was asked and with what,
/// and its last update.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Now {
    pub claim: Option<Claim>,
    pub asked: Option<Ask>,
    pub updated_at: String,
}

impl Now {
    /// What an item's row before the drop holds now.
    #[must_use]
    pub fn of(i: &OldItem) -> Self {
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
        let asked = (i.state == "open" && i.turn.as_deref() == Some("user")).then(|| Ask {
            since: i.asked_at.clone().unwrap_or_else(|| i.opened_at.clone()),
            note: i.turn_note.clone(),
        });
        Now {
            claim,
            asked,
            updated_at: i.updated_at.clone(),
        }
    }

    /// What the item holds, a claim before an ask.
    #[must_use]
    pub fn held(&self) -> Option<Held> {
        match (&self.claim, &self.asked) {
            (Some(c), _) => Some(Held::Claim(c.clone())),
            (None, Some(a)) => Some(Held::Ask(a.clone())),
            (None, None) => None,
        }
    }
}

/// The open row for what an item holds, as nothing but its holding knows it.
fn opened(held: &Held) -> Assignment {
    let base = |at: &str, host: &str| {
        row(&Event {
            at: at.to_string(),
            host: host.to_string(),
            ..Event::default()
        })
    };
    match held {
        Held::Claim(c) => Assignment {
            branch: Some(c.branch.clone()),
            machine: c.on.clone(),
            runner: c.runner.clone(),
            job: c.job.clone(),
            ..base(&c.since, &c.host)
        },
        Held::Ask(a) => Assignment {
            assignee: OWNER.to_string(),
            kind: Kind::Ask,
            note: a.note.clone(),
            ..base(&a.since, "")
        },
    }
}

/// How an item's open row is brought to agree with what the item holds: the open row ended, a row
/// opened, or the open ask given the note it holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Agreement {
    pub end: bool,
    pub open: Option<Assignment>,
    pub note: Option<String>,
}

/// What brings the open row to agree with the item: a claim agrees with an open claim on its
/// branch, an ask with an open ask, whose note it sets when it holds one; anything else ends the
/// open row and opens what the item holds.
#[must_use]
pub fn agree(open: Option<&Held>, now: &Now) -> Agreement {
    let want = now.held();
    match (open, &want) {
        (None, None) => Agreement::default(),
        (Some(Held::Claim(o)), Some(Held::Claim(w))) if o.branch == w.branch => {
            Agreement::default()
        }
        (Some(Held::Ask(o)), Some(Held::Ask(w))) => Agreement {
            note: w.note.clone().filter(|n| o.note.as_ref() != Some(n)),
            ..Agreement::default()
        },
        _ => Agreement {
            end: open.is_some(),
            open: want.as_ref().map(opened),
            note: None,
        },
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
    rows.extend(now.held().as_ref().map(opened));
}

#[cfg(test)]
#[path = "tests/assignment.rs"]
mod tests;
