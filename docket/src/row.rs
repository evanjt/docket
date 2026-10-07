//! An item as the server sends it, printed as a row: one line and its tail, or JSON.

use std::fmt::Write;

use serde_json::Value;

use docket_core::rows::Row;

use crate::jsonout::{Json, cut, or_none};

/// The stored columns in the order the database holds them, the order `--json` prints them in.
pub const COLUMNS: [&str; 28] = [
    "project",
    "key",
    "num",
    "id",
    "title",
    "state",
    "turn",
    "turn_note",
    "asked_at",
    "claim_branch",
    "claim_host",
    "claim_since",
    "wait_on",
    "wait_ref",
    "wait_since",
    "decision",
    "decided_at",
    "resolution",
    "complexity",
    "release",
    "area",
    "labels",
    "body",
    "opened_at",
    "updated_at",
    "claim_runner",
    "claim_job",
    "claim_on",
];

/// What `--json` adds after the stored columns and any a query selected.
pub const DERIVED: [&str; 5] = ["group", "repo", "word", "superseded_by", "priority"];

/// One item in the `--json` shape: the columns, then `extra` columns a query selected, the derived
/// fields, and `tail` fields set after.
#[must_use]
pub fn item_json(v: &Value, extra: &[&str], tail: &[&str]) -> Json {
    let keys: Vec<&str> = COLUMNS
        .iter()
        .chain(extra)
        .chain(DERIVED.iter())
        .chain(tail)
        .copied()
        .collect();
    Json::pick(v, &keys)
}

/// A list of items in the `--json` shape.
#[must_use]
pub fn items_json(rows: &Value, extra: &[&str], tail: &[&str]) -> Json {
    Json::List(
        rows.as_array()
            .into_iter()
            .flatten()
            .map(|r| item_json(r, extra, tail))
            .collect(),
    )
}

/// One item from the server, or the default when it is not one.
#[must_use]
pub fn row_of(v: &Value) -> Row {
    serde_json::from_value(v.clone()).unwrap_or_default()
}

/// The items of a list from the server.
#[must_use]
pub fn rows_of(v: &Value) -> Vec<Row> {
    v.as_array().into_iter().flatten().map(row_of).collect()
}

/// `B14    ready    high [low] Title`, then a line for each claim, wait, ask, decision, end and group.
#[must_use]
pub fn fmt_row(r: &Row, flag: Option<&str>) -> String {
    let cx = r
        .complexity
        .as_deref()
        .filter(|c| !c.is_empty())
        .map_or(String::new(), |c| format!(" [{c}]"));
    let pri = if r.priority == "normal" {
        String::new()
    } else {
        format!(" {}", r.priority)
    };
    let mut line = format!("{:<6} {:<8}{pri}{cx} {}", r.id, r.word, cut(&r.title, 90));
    let tail = tail_lines(r, flag);
    for t in tail {
        line.push_str("\n       ");
        line.push_str(&t);
    }
    line
}

fn tail_lines(r: &Row, flag: Option<&str>) -> Vec<String> {
    let mut tail = Vec::new();
    if let Some(branch) = r.claim_branch.as_deref().filter(|b| !b.is_empty()) {
        let mut held = format!(
            "held by {branch} on {} since {}",
            or_none(r.claim_host.as_deref()),
            or_none(r.claim_since.as_deref())
        );
        if let Some(runner) = r.claim_runner.as_deref().filter(|x| !x.is_empty()) {
            match r.claim_job.as_deref().filter(|x| !x.is_empty()) {
                Some(job) => {
                    let _ = write!(held, ", {runner} job {job}");
                }
                None => {
                    let _ = write!(held, ", run by {runner}");
                }
            }
        }
        if let Some(f) = flag {
            let _ = write!(held, "  [{f}]");
        }
        tail.push(held);
    }
    let (wref, wsince) = (
        or_none(r.wait_ref.as_deref()),
        or_none(r.wait_since.as_deref()),
    );
    match r.wait_on.as_deref() {
        Some("item") => tail.push(format!("waits on {wref} since {wsince}")),
        Some("condition") => tail.push(format!(
            "waits on {wref}, a task for the owner (since {wsince})"
        )),
        _ => {}
    }
    if r.turn.as_deref() == Some("user")
        && let Some(note) = r.turn_note.as_deref().filter(|n| !n.is_empty())
    {
        tail.push(format!(
            "asked: {}",
            cut(note.lines().next().unwrap_or_default(), 120)
        ));
    }
    if r.state == "open"
        && let Some(d) = r.decision.as_deref().filter(|d| !d.is_empty())
    {
        tail.push(format!(
            "decided {}: {}",
            or_none(r.decided_at.as_deref()),
            cut(d, 120)
        ));
    }
    if r.state != "open" {
        tail.push(format!("{}: {}", r.state, or_none(r.resolution.as_deref())));
    }
    if let Some(g) = r.group.as_deref().filter(|g| !g.is_empty()) {
        tail.push(format!("group {g}"));
    }
    tail
}

#[cfg(test)]
#[path = "tests/row.rs"]
mod tests;
