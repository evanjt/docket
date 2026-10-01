//! The moves read from the event log, the pace of closes, and the durations a person reads.

/// Every verb a move row names, in the order a row lists them: closes first.
pub const VERBS: [&str; 12] = [
    "closed", "dropped", "opened", "reopened", "claimed", "decided", "parked", "replied",
    "released", "lost", "blocked", "resumed",
];

/// Events that move an item without changing the open count, and the verb a row uses for each.
const ACTIVITY: [(&str, &str); 8] = [
    ("claimed", "claimed"),
    ("released", "released"),
    ("claim_lost", "lost"),
    ("asked", "parked"),
    ("replied", "replied"),
    ("decided", "decided"),
    ("waited", "blocked"),
    ("resumed", "resumed"),
];

/// One event as a move reads it.
#[derive(Clone, Copy, Debug)]
pub struct Logged<'a> {
    pub at: i64,
    pub id: &'a str,
    pub kind: &'a str,
    pub note: Option<&'a str>,
}

/// A move: when, its change to the open count, the item and the verb.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Move {
    pub at: i64,
    pub delta: i64,
    pub id: String,
    pub verb: &'static str,
}

fn verb_of(e: &Logged, logged_close: bool) -> Option<(i64, &'static str)> {
    let legacy = e.kind == "legacy";
    match (e.kind, e.note) {
        ("dropped", _) => Some((-1, "dropped")),
        ("closed", _) => Some((-1, "closed")),
        ("reopened", _) => Some((1, "reopened")),
        ("opened", _) => Some((1, "opened")),
        (_, Some("closed")) if legacy => Some((-1, "closed")),
        (_, Some("opened")) if legacy => Some((1, "opened")),
        (_, Some("DONE" | "BUILT")) if legacy && !logged_close => Some((-1, "closed")),
        (kind, _) => ACTIVITY
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, v)| (0, *v)),
    }
}

/// The events that moved an item, oldest first. A legacy item carries a close from the old log and one
/// from its status at import, so its status close counts only when the log has none.
#[must_use]
pub fn moves(events: &[Logged]) -> Vec<Move> {
    let logged: Vec<&str> = events
        .iter()
        .filter(|e| e.kind == "legacy" && e.note == Some("closed"))
        .map(|e| e.id)
        .collect();
    events
        .iter()
        .filter_map(|e| {
            let (delta, verb) = verb_of(e, logged.contains(&e.id))?;
            Some(Move {
                at: e.at,
                delta,
                id: e.id.to_string(),
                verb,
            })
        })
        .collect()
}

/// Closes, opens and the working seconds over the last `recent` closes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pace {
    pub closed: u64,
    pub opened: u64,
    pub working: i64,
}

impl Pace {
    /// Closes an hour of working time, rounded half to even as Python's `round` does; none when nothing
    /// closed.
    #[must_use]
    pub fn per_hour(&self) -> Option<u64> {
        if self.closed == 0 || self.working <= 0 {
            return None;
        }
        let working = u64::try_from(self.working).ok()?;
        let (q, r) = (self.closed * 3600 / working, self.closed * 3600 % working);
        let up = 2 * r > working || (2 * r == working && q % 2 == 1);
        Some(q + u64::from(up)).filter(|n| *n > 0)
    }
}

/// The pace over the slice holding the last `recent` closes. Each gap counts up to a cap of six median
/// gaps, floored at half an hour, so an overnight stop adds half an hour and a slow item counts in full.
#[must_use]
pub fn pace(moves: &[Move], recent: u64, now: i64) -> Pace {
    let counted: Vec<&Move> = moves.iter().filter(|m| m.delta != 0).collect();
    let (mut seen, mut start) = (0, counted.len());
    for (i, m) in counted.iter().enumerate().rev() {
        if m.delta < 0 {
            seen += 1;
            start = i;
            if seen >= recent {
                break;
            }
        } else if seen > 0 {
            start = i;
        }
    }
    let slice = &counted[start..];
    let mut stamps: Vec<i64> = slice.iter().map(|m| m.at).collect();
    stamps.push(now);
    let gaps: Vec<i64> = stamps.windows(2).map(|w| w[1] - w[0]).collect();
    let working = if gaps.is_empty() {
        0
    } else {
        let mut sorted = gaps.clone();
        sorted.sort_unstable();
        let cap = six_medians(&sorted).max(1800);
        gaps.iter().map(|g| (*g).min(cap)).sum()
    };
    Pace {
        closed: slice.iter().filter(|m| m.delta < 0).count() as u64,
        opened: slice.iter().filter(|m| m.delta > 0).count() as u64,
        working,
    }
}

/// Six times the median, exact: an even count's median is the mean of its middle two.
fn six_medians(sorted: &[i64]) -> i64 {
    match sorted.len() {
        0 => 0,
        n if n % 2 == 1 => sorted[n / 2] * 6,
        n => (sorted[n / 2 - 1] + sorted[n / 2]) * 3,
    }
}

/// The moves of one minute: the open count before and after, and the ids under each verb.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Minute {
    pub at: i64,
    pub before: i64,
    pub after: i64,
    pub verbs: Vec<(&'static str, Vec<String>)>,
}

/// The moves grouped by minute, newest first. Counts are walked back from the open count now, so the
/// newest row ends where the queue stands.
#[must_use]
pub fn minutes(moves: &[Move], open_now: i64) -> Vec<Minute> {
    let mut groups: Vec<(i64, Vec<&Move>)> = Vec::new();
    for m in moves {
        let at = m.at - m.at.rem_euclid(60);
        match groups.last_mut() {
            Some((t, ms)) if *t == at => ms.push(m),
            _ => groups.push((at, vec![m])),
        }
    }
    let mut after = open_now;
    let mut out = Vec::with_capacity(groups.len());
    for (at, ms) in groups.into_iter().rev() {
        let before = after - ms.iter().map(|m| m.delta).sum::<i64>();
        let verbs = VERBS
            .iter()
            .filter_map(|v| {
                let ids: Vec<String> = ms
                    .iter()
                    .filter(|m| m.verb == *v)
                    .map(|m| m.id.clone())
                    .collect();
                (!ids.is_empty()).then_some((*v, ids))
            })
            .collect();
        out.push(Minute {
            at,
            before,
            after,
            verbs,
        });
        after = before;
    }
    out
}

/// `45s`, `12m`, `3h 05m`, `2d 4h`.
#[must_use]
pub fn duration(seconds: i64) -> String {
    let s = seconds.max(0);
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m", s / 60),
        3600..86400 => format!("{}h {:02}m", s / 3600, (s % 3600) / 60),
        _ => format!("{}d {}h", s / 86400, (s % 86400) / 3600),
    }
}

/// Epoch seconds from a stamp as docket writes them, `2026-10-01T09:29:41Z`; `None` for anything else.
#[must_use]
pub fn epoch(stamp: &str) -> Option<i64> {
    let bytes = stamp.as_bytes();
    if bytes.len() < 19 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    let num = |from: usize, to: usize| stamp.get(from..to)?.parse::<i64>().ok();
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86400 + hour * 3600 + minute * 60 + second)
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
#[path = "tests/pace.rs"]
mod tests;
