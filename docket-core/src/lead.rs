//! A project's lead claim: at most one session leads a project, renewing its claim while it runs. A
//! claim not renewed within the lapse is free to take over.

use serde::{Deserialize, Serialize};

use crate::clock;
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
