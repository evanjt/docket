//! `docket list` and the groups of `docket wip`: items under the status words the server derives,
//! in the order the work flows.

use serde_json::Value;

use docket_core::pace::epoch;
use docket_core::rows::Row;
use docket_core::word::PRIORITIES;

use crate::args::ListSort;
use crate::ctx::Ctx;
use crate::fail::Result;
use crate::jsonout::Json;
use crate::row::{fmt_row, item_json, rows_of};

/// The words of open items in the order the work flows through them.
pub const WORDS: [&str; 9] = [
    "ready",
    "in progress",
    "to land",
    "job failed",
    "blocked",
    "waiting on owner",
    "parked",
    "under way",
    "audit due",
];

/// The words a claim shows under, in the order `docket wip` groups them.
pub const CLAIM_WORDS: [&str; 3] = ["in progress", "to land", "job failed"];

/// The group plans are listed under, beside the words and not counted under them.
pub const PLANS: &str = "plans";

/// A group of rows under the word that names it.
pub type Group = (String, Vec<Row>);

/// A claim row with the minutes it has gone.
pub type Held = (Row, Option<i64>);

/// Whole minutes between a stamp and `now`, none for a stamp that is not one.
fn minutes_since(stamp: Option<&str>, now: i64) -> Option<i64> {
    Some((now - epoch(stamp?)?).max(0) / 60)
}

/// The minutes a claim has gone: since the claim for a running job, since the job's end for one
/// that ended.
#[must_use]
pub fn claim_minutes(r: &Row, now: i64) -> Option<i64> {
    let from = if r.word == "in progress" {
        r.claim_since.as_deref()
    } else {
        r.claim_ended_at.as_deref().or(r.claim_since.as_deref())
    };
    minutes_since(from, now)
}

/// Claimed rows under `in progress`, `to land` and `job failed`, a word with no rows left out, each
/// row with its minutes. Rows keep the order they came in within a group.
#[must_use]
pub fn wip_groups(rows: &[Row], now: i64) -> Vec<(String, Vec<Held>)> {
    let mut words: Vec<String> = CLAIM_WORDS.iter().map(|w| (*w).to_string()).collect();
    for r in rows {
        if !words.contains(&r.word) {
            words.push(r.word.clone());
        }
    }
    words
        .into_iter()
        .filter_map(|w| {
            let held: Vec<Held> = rows
                .iter()
                .filter(|r| r.word == w)
                .map(|r| (r.clone(), claim_minutes(r, now)))
                .collect();
            (!held.is_empty()).then_some((w, held))
        })
        .collect()
}

/// The minutes a claim row shows, worded by what they count from.
#[must_use]
pub fn minutes_text(word: &str, minutes: Option<i64>) -> String {
    let Some(m) = minutes else {
        return String::new();
    };
    if word == "in progress" {
        format!("  ({m} min since the claim)")
    } else {
        format!("  ({m} min since the job ended)")
    }
}

fn tier(r: &Row) -> usize {
    PRIORITIES
        .iter()
        .position(|p| *p == r.priority)
        .unwrap_or(PRIORITIES.len())
}

fn sorted(rows: &mut [Row], by: ListSort) {
    match by {
        ListSort::Queue => {}
        ListSort::Id => rows.sort_by(|a, b| (&a.key, a.num).cmp(&(&b.key, b.num))),
        ListSort::Priority => {
            rows.sort_by(|a, b| (tier(a), &a.key, a.num).cmp(&(tier(b), &b.key, b.num)));
        }
        ListSort::Updated => rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
    }
}

/// Open rows grouped by word in flow order, plans in a group of their own after the words, a group
/// with no rows left out. `only` keeps one group; the rows of each are ordered by `by`.
#[must_use]
pub fn groups(rows: Vec<Row>, only: Option<&str>, by: ListSort) -> Vec<Group> {
    let (plan_rows, work): (Vec<Row>, Vec<Row>) =
        rows.into_iter().partition(|r| r.item_type == "plan");
    let mut names: Vec<String> = WORDS.iter().map(|w| (*w).to_string()).collect();
    for r in &work {
        if !names.contains(&r.word) {
            names.push(r.word.clone());
        }
    }
    let mut out: Vec<Group> = names
        .into_iter()
        .map(|w| {
            (
                w.clone(),
                work.iter().filter(|r| r.word == w).cloned().collect(),
            )
        })
        .collect();
    out.push((PLANS.to_string(), plan_rows));
    out.retain(|(name, rows)| !rows.is_empty() && only.is_none_or(|w| w == name));
    for (_, rows) in &mut out {
        sorted(rows, by);
    }
    out
}

/// # Errors
/// The project cannot be resolved, or the server refuses.
pub fn list(ctx: &mut Ctx, word: Option<&String>, by: ListSort) -> Result<i32> {
    let found = ctx.read("/open", &[])?;
    let found = groups(rows_of(&found), word.map(String::as_str), by);
    if ctx.json {
        let all: Vec<Json> = found
            .iter()
            .flat_map(|(_, rows)| rows)
            .map(|r| {
                item_json(
                    &serde_json::to_value(r).unwrap_or(Value::Null),
                    &[],
                    &["claim_land"],
                )
            })
            .collect();
        ctx.emit(&Json::List(all));
        return Ok(0);
    }
    if found.is_empty() {
        println!("Nothing open.");
        return Ok(0);
    }
    let now = crate::cmd::status::now();
    for (i, (name, rows)) in found.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!("{name} ({})", rows.len());
        for r in rows {
            println!("{}", list_row(r, now));
        }
    }
    Ok(0)
}

/// A row with `note` after the end of its first line.
fn with_note(mut text: String, note: &str) -> String {
    if !note.is_empty() {
        let first = text.find('\n').unwrap_or(text.len());
        text.insert_str(first, note);
    }
    text
}

/// The landing stage of a claim's build with its age, after two spaces, or nothing before it is queued.
fn land_note(r: &Row, now: i64) -> String {
    r.claim_land
        .as_ref()
        .map_or_else(String::new, |l| format!("  [{}]", l.words(now)))
}

/// The text of one row of `list`: the row, then the landing stage of a build that waits to land.
#[must_use]
pub fn list_row(r: &Row, now: i64) -> String {
    with_note(fmt_row(r, None), &land_note(r, now))
}

/// The text of one claim row under its group: the row, then how long it has been and where its
/// build stands on its way to landing.
#[must_use]
pub fn wip_row(word: &str, r: &Row, minutes: Option<i64>, flag: Option<&str>, now: i64) -> String {
    let note = minutes_text(word, minutes) + &land_note(r, now);
    with_note(fmt_row(r, flag), &note)
}

#[cfg(test)]
#[path = "../tests/list.rs"]
mod tests;
