//! The numbers a project is read by: progress, throughput, cycle and lead time, time spent, cost and
//! the forecast of a release. Each is one function over plain rows; a surface prints what they return
//! and works nothing out itself.

// Counts of items and days are far below the widths these casts could lose.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::assignment::{Kind, Outcome};
use crate::clock::stamp;
use crate::fact::Prices;

const DAY: i64 = 86_400;
/// Calendar days the throughput and the forecast read by default.
pub const WINDOW: u32 = 28;
/// Resampled futures the forecast draws.
pub const TRIALS: usize = 1000;
/// Days a resampled future may run before it counts as never finishing.
pub const HORIZON: u32 = 730;

/// One item as the metrics read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subject {
    pub id: String,
    /// The status word.
    pub word: String,
    /// The position of its release in the project's order; none is the backlog.
    pub release: Option<usize>,
    pub opened_at: i64,
    /// When it was closed or dropped, for an item that is.
    pub closed_at: Option<i64>,
    /// When its first assignment started.
    pub started_at: Option<i64>,
}

impl Subject {
    fn done(&self) -> bool {
        self.word == "done"
    }

    fn open(&self) -> bool {
        self.word != "done" && self.word != "dropped"
    }
}

/// Done of everything not dropped, and how many items stand under each open status word.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub done: u64,
    pub dropped: u64,
    /// Every item, dropped ones included.
    pub total: u64,
    pub open: BTreeMap<String, u64>,
}

#[must_use]
pub fn progress(items: &[Subject]) -> Progress {
    let mut out = Progress::default();
    for i in items {
        out.total += 1;
        match i.word.as_str() {
            "done" => out.done += 1,
            "dropped" => out.dropped += 1,
            word => *out.open.entry(word.to_string()).or_default() += 1,
        }
    }
    out
}

/// Items done on one calendar day, UTC.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Day {
    pub date: String,
    pub done: u64,
}

fn date_of(day: i64) -> String {
    stamp(u64::try_from(day.max(0) * DAY).unwrap_or(0))[..10].to_string()
}

/// Items done per calendar day over the `days` ending with the day of `now`, oldest first; a day with
/// none reads zero.
#[must_use]
pub fn throughput(items: &[Subject], now: i64, days: u32) -> Vec<Day> {
    let today = now.div_euclid(DAY);
    let first = today - i64::from(days.max(1)) + 1;
    (first..=today)
        .map(|d| Day {
            date: date_of(d),
            done: items
                .iter()
                .filter(|i| i.done() && i.closed_at.is_some_and(|c| c.div_euclid(DAY) == d))
                .count() as u64,
        })
        .collect()
}

/// A span of seconds over the items counted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Spread {
    pub n: u64,
    pub median: i64,
    pub mean: i64,
}

fn spread(mut seconds: Vec<i64>) -> Option<Spread> {
    if seconds.is_empty() {
        return None;
    }
    seconds.sort_unstable();
    let n = seconds.len();
    let median = if n % 2 == 1 {
        seconds[n / 2]
    } else {
        (seconds[n / 2 - 1] + seconds[n / 2]) / 2
    };
    Some(Spread {
        n: n as u64,
        median,
        mean: seconds.iter().sum::<i64>() / n as i64,
    })
}

/// First assignment start to done, over the done items that had an assignment.
#[must_use]
pub fn cycle_time(items: &[Subject]) -> Option<Spread> {
    spread(
        items
            .iter()
            .filter(|i| i.done())
            .filter_map(|i| Some(i.closed_at? - i.started_at?))
            .filter(|s| *s >= 0)
            .collect(),
    )
}

/// Opened to closed, over the done items.
#[must_use]
pub fn lead_time(items: &[Subject]) -> Option<Spread> {
    spread(
        items
            .iter()
            .filter(|i| i.done())
            .filter_map(|i| Some(i.closed_at? - i.opened_at))
            .filter(|s| *s >= 0)
            .collect(),
    )
}

/// One assignment as the metrics read it.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub item: String,
    pub kind: Kind,
    /// The runner of a claim; none is a claim made by hand.
    pub runner: Option<String>,
    pub start: i64,
    /// None is under way now.
    pub end: Option<i64>,
    pub outcome: Option<Outcome>,
    /// When the item was closed, for the time a blocked claim stays blocked.
    pub settled: Option<i64>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub cost_reported: Option<f64>,
    pub model: Option<String>,
}

/// Seconds worked by an agent and by a person, apart from the seconds an item waited on the owner
/// and the seconds it sat blocked after a claim ended on a block.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TimeSpent {
    pub agent: i64,
    pub person: i64,
    pub waiting_owner: i64,
    pub blocked: i64,
}

#[must_use]
pub fn time_spent(spans: &[Span], now: i64) -> TimeSpent {
    let mut out = TimeSpent::default();
    for s in spans {
        let len = (s.end.unwrap_or(now) - s.start).max(0);
        match (s.kind, &s.runner) {
            (Kind::Ask, _) => out.waiting_owner += len,
            (Kind::Claim, Some(_)) => out.agent += len,
            (Kind::Claim, None) => out.person += len,
        }
        if let (Kind::Claim, Some(Outcome::Blocked), Some(end)) = (s.kind, s.outcome, s.end) {
            let next = spans
                .iter()
                .filter(|o| o.item == s.item && o.start >= end)
                .map(|o| o.start)
                .min();
            out.blocked += (next.or(s.settled).unwrap_or(now) - end).max(0);
        }
    }
    out
}

/// What the agent attempts used: tokens, seconds, the cost the runners reported, and the money the
/// project's `prices` fact puts on their tokens. An attempt whose runner reported no cost adds none
/// to the reported sum; one on a model with no price adds none to the money.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Cost {
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub agent_seconds: i64,
    /// The sum over the attempts that reported one; none when none did.
    pub cost_reported: Option<f64>,
    pub attempts: u64,
    /// Attempts that reported a cost; below `attempts` the sum covers only part of them.
    pub attempts_reported: u64,
    /// Tokens at the `prices` fact's rates, summed over the attempts on a priced model; none when none was.
    pub money: Option<f64>,
    /// Attempts on a priced model; below `attempts` the money covers only part of them.
    pub attempts_priced: u64,
}

#[must_use]
pub fn cost(spans: &[Span], now: i64, prices: &Prices) -> Cost {
    let mut out = Cost::default();
    for s in spans
        .iter()
        .filter(|s| s.kind == Kind::Claim && s.runner.is_some())
    {
        out.attempts += 1;
        out.tokens_in += s.tokens_in.unwrap_or(0);
        out.tokens_out += s.tokens_out.unwrap_or(0);
        out.agent_seconds += (s.end.unwrap_or(now) - s.start).max(0);
        if let Some(c) = s.cost_reported {
            *out.cost_reported.get_or_insert(0.0) += c;
            out.attempts_reported += 1;
        }
        if let Some(&(input, output)) = s.model.as_ref().and_then(|m| prices.get(m)) {
            let tokens = |n: Option<i64>| n.unwrap_or(0) as f64 / 1_000_000.0;
            *out.money.get_or_insert(0.0) +=
                tokens(s.tokens_in) * input + tokens(s.tokens_out) * output;
            out.attempts_priced += 1;
        }
    }
    out
}

/// When a release is expected to be clear.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Forecast {
    /// Items still open up to and including the release.
    pub open: u64,
    /// Mean closes less opens a day over the window.
    pub burn: f64,
    /// False when the burn is not above zero: the set is not shrinking.
    pub converging: bool,
    pub p50: Option<String>,
    pub p85: Option<String>,
    pub target: Option<String>,
    /// The P85 date falls after the target, or the set is not converging; none without a target.
    pub late: Option<bool>,
}

impl Forecast {
    /// The line a person reads: when the release clears, or that it is not converging.
    #[must_use]
    pub fn line(&self) -> String {
        if self.open == 0 {
            return "nothing open".into();
        }
        let dates = match (&self.p50, &self.p85) {
            (Some(p50), Some(p85)) if self.converging => format!("clear by P50 {p50}, P85 {p85}"),
            _ => "not converging: closes do not outrun opens".to_string(),
        };
        let target = match (&self.target, self.late) {
            (Some(t), Some(true)) => format!("; late for the target {t}"),
            (Some(t), Some(false)) => format!("; on course for the target {t}"),
            _ => String::new(),
        };
        format!("{} open, {dates}{target}", self.open)
    }
}

/// The next draw of a splitmix64 stream.
fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The days to clear `open` items when each day takes a burn drawn with replacement from `burn`; none
/// when it takes longer than the horizon.
fn days_to_clear(open: i64, burn: &[i64], state: &mut u64) -> Option<u32> {
    let mut left = open;
    for day in 1..=HORIZON {
        left -= burn[(next(state) % burn.len() as u64) as usize];
        if left <= 0 {
            return Some(day);
        }
    }
    None
}

/// The forecast of the items up to and including a release (`upto` is its position): open items, the
/// daily burn over the last `days`, and P50 and P85 dates from `TRIALS` seeded resamples of that burn,
/// the same for the same seed. `target` is the release's target date, `YYYY-MM-DD`.
#[must_use]
pub fn forecast(
    items: &[Subject],
    upto: usize,
    now: i64,
    days: u32,
    seed: u64,
    target: Option<&str>,
) -> Forecast {
    let set: Vec<&Subject> = items
        .iter()
        .filter(|i| i.release.is_some_and(|r| r <= upto))
        .collect();
    let open = set.iter().filter(|i| i.open()).count();
    let today = now.div_euclid(DAY);
    let first = today - i64::from(days.max(1)) + 1;
    let burn: Vec<i64> = (first..=today)
        .map(|d| {
            let on = |at: i64| at.div_euclid(DAY) == d;
            let closed = set.iter().filter(|i| i.closed_at.is_some_and(on)).count();
            let opened = set.iter().filter(|i| on(i.opened_at)).count();
            closed as i64 - opened as i64
        })
        .collect();
    let mean = burn.iter().sum::<i64>() as f64 / burn.len() as f64;
    let mut out = Forecast {
        open: open as u64,
        burn: mean,
        converging: open == 0 || mean > 0.0,
        target: target.map(str::to_string),
        ..Forecast::default()
    };
    if open == 0 {
        out.p50 = Some(date_of(today));
        out.p85 = out.p50.clone();
    } else if out.converging {
        let mut state = seed;
        let mut runs: Vec<Option<u32>> = (0..TRIALS)
            .map(|_| days_to_clear(open as i64, &burn, &mut state))
            .collect();
        runs.sort_by_key(|r| r.unwrap_or(u32::MAX));
        let at =
            |p: usize| runs[(TRIALS * p).div_ceil(100) - 1].map(|d| date_of(today + i64::from(d)));
        out.p50 = at(50);
        out.p85 = at(85);
    }
    out.late = target.map(|t| match &out.p85 {
        Some(p) => p.as_str() > t,
        None => true,
    });
    out
}

/// A release not yet shipped, as its row is asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unshipped {
    /// The position of the release in the project's whole order, as `Subject::release` counts it.
    pub position: usize,
    pub name: String,
    pub target: Option<String>,
}

/// One unshipped release as every surface reads it: what is open under each word, what is closed,
/// the items a later release holds up, and the pace its forecast runs on.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReleaseRow {
    pub name: String,
    pub open: u64,
    pub ready: u64,
    pub building: u64,
    pub waiting_owner: u64,
    pub blocked: u64,
    /// Items closed as done; dropped ones are not counted.
    pub closed: u64,
    /// Open items of the release that an item of a later release holds.
    pub held_later: u64,
    /// Mean closes less opens a day over the window, the burn the forecast draws from.
    pub pace: f64,
    pub forecast: Forecast,
}

/// One row per unshipped release, in the order they ship. `held` holds the ids of the items a later
/// release holds up; `seed` fixes the forecast draws.
#[must_use]
pub fn release_rows<S: std::hash::BuildHasher>(
    items: &[Subject],
    releases: &[Unshipped],
    held: &HashSet<String, S>,
    now: i64,
    days: u32,
    seed: u64,
) -> Vec<ReleaseRow> {
    releases
        .iter()
        .map(|r| {
            let mine: Vec<&Subject> = items
                .iter()
                .filter(|i| i.release == Some(r.position))
                .collect();
            let words = |w: &[&str]| {
                mine.iter()
                    .filter(|i| i.open() && w.contains(&i.word.as_str()))
                    .count() as u64
            };
            let forecast = forecast(items, r.position, now, days, seed, r.target.as_deref());
            ReleaseRow {
                name: r.name.clone(),
                open: mine.iter().filter(|i| i.open()).count() as u64,
                ready: words(&["ready"]),
                building: words(&["building", "checking", "in progress"]),
                waiting_owner: words(&["parked", "waiting on owner"]),
                blocked: words(&["blocked"]),
                closed: mine.iter().filter(|i| i.done()).count() as u64,
                held_later: mine
                    .iter()
                    .filter(|i| i.open() && held.contains(&i.id))
                    .count() as u64,
                pace: forecast.burn,
                forecast,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/metrics.rs"]
mod tests;
