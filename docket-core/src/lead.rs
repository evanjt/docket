//! A project's lead claim: at most one session leads a project, renewing its claim while it runs. A
//! claim not renewed within the lapse is free to take over.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::clock;
use crate::fact;
use crate::item::Refused;
use crate::pace::epoch;

/// The minutes a lead claim holds without a renewal when the project sets no `lead_lapse`.
pub const LAPSE: i64 = 10;

/// The lead claim as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lead {
    pub project: String,
    pub host: String,
    pub session: String,
    pub branch: Option<String>,
    pub since: String,
    pub renewed_at: String,
}

/// Who asks: the host of their key and the session they name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holder {
    pub host: String,
    pub session: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Take,
    Renew,
    Give,
}

impl Act {
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "take" => Some(Act::Take),
            "renew" => Some(Act::Renew),
            "give" => Some(Act::Give),
            _ => None,
        }
    }
}

/// What an act did. A takeover carries the claim it replaced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Took,
    TookOver(Lead),
    Renewed,
    Gave,
}

impl Outcome {
    /// The word the answer and the event carry.
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            Outcome::Took => "took",
            Outcome::TookOver(_) => "took over",
            Outcome::Renewed => "renewed",
            Outcome::Gave => "gave",
        }
    }
}

/// Whether the claim went unrenewed for the lapse. A renewal that does not read counts as lapsed.
#[must_use]
pub fn lapsed(lead: &Lead, now: i64, lapse_minutes: i64) -> bool {
    epoch(&lead.renewed_at).is_none_or(|t| now >= t + lapse_minutes * 60)
}

/// The minutes a lead claim holds, from the project's `lead_lapse`.
fn lapse_of(skills: &BTreeMap<String, String>) -> i64 {
    fact::effective(skills, "lead_lapse")
        .and_then(|v| v.parse().ok())
        .unwrap_or(LAPSE)
}

/// Whether a lead may be started now, or why not, a phrase per reason: a claim still held, then the
/// reasons `fact::gaps` gives. The claim is free when none is held or it went unrenewed for the lapse.
/// `skills` are the facts already merged with the owner level (`fact::merged`). The built-in `models`
/// does not count: starting a lead unasked spends on every project, so it waits for a set value.
///
/// # Errors
/// The reasons no lead starts.
pub fn should_start(
    claim: Option<&Lead>,
    skills: &BTreeMap<String, String>,
    now: i64,
) -> Result<(), Vec<String>> {
    let mut why = Vec::new();
    if let Some(lead) = claim.filter(|l| !lapsed(l, now, lapse_of(skills))) {
        why.push(format!("{} on {} holds the lead", lead.session, lead.host));
    }
    why.extend(fact::gaps(skills, &BTreeMap::new()));
    let models = fact::layered(skills, &BTreeMap::new(), "models");
    if models.is_none_or(|(_, layer)| layer == fact::Layer::Default) {
        why.push("no models".to_string());
    }
    if why.is_empty() { Ok(()) } else { Err(why) }
}

/// Whether the last start attempt, made at `last`, is older than the lapse. A start that failed
/// counts, so a broken launch is tried once a window and not on every refresh.
#[must_use]
pub fn attempt_due(last: Option<i64>, now: i64, skills: &BTreeMap<String, String>) -> bool {
    last.is_none_or(|t| now >= t + lapse_of(skills) * 60)
}

/// When the claim lapses unless renewed.
#[must_use]
pub fn lapses_at(lead: &Lead, lapse_minutes: i64) -> Option<String> {
    let t = epoch(&lead.renewed_at)? + lapse_minutes * 60;
    u64::try_from(t).ok().map(clock::stamp)
}

fn holds(lead: &Lead, who: &Holder) -> bool {
    lead.host == who.host && lead.session == who.session
}

/// What an act does to the project's claim, or why it is refused.
///
/// # Errors
/// A take while another holder renewed within the lapse; a renewal or a give by anyone but the
/// holder, or with no claim held.
pub fn decide(
    project: &str,
    held: Option<&Lead>,
    who: &Holder,
    act: Act,
    now: i64,
    lapse_minutes: i64,
) -> Result<Outcome, Refused> {
    let Some(lead) = held else {
        return match act {
            Act::Take => Ok(Outcome::Took),
            Act::Renew => Err(Refused(format!(
                "no lead holds {project}: docket lead take"
            ))),
            Act::Give => Err(Refused(format!("no lead holds {project}"))),
        };
    };
    if holds(lead, who) {
        return Ok(match act {
            Act::Take | Act::Renew => Outcome::Renewed,
            Act::Give => Outcome::Gave,
        });
    }
    match act {
        Act::Take if lapsed(lead, now, lapse_minutes) => Ok(Outcome::TookOver(lead.clone())),
        Act::Take => Err(Refused(format!(
            "{project} is led by {} on {} since {}, last renewed {}; it lapses at {} unless renewed",
            lead.session,
            lead.host,
            lead.since,
            lead.renewed_at,
            lapses_at(lead, lapse_minutes).unwrap_or_default()
        ))),
        Act::Renew | Act::Give => Err(Refused(format!(
            "{project} is led by {} on {}, not by {} on {}: only its holder {}",
            lead.session,
            lead.host,
            who.session,
            who.host,
            if act == Act::Renew {
                "renews it"
            } else {
                "gives it back"
            }
        ))),
    }
}

#[cfg(test)]
#[path = "tests/lead.rs"]
mod tests;
