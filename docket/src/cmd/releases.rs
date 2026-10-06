//! `docket releases`: a project's releases, listed, added, their open items moved, or shipped.

use docket_core::api::{ReleasesDone, ReleasesRequest};
use docket_core::metrics::ReleaseRow;
use docket_core::release::{Release, Row};
use serde_json::{Value, json};

use crate::ctx::Ctx;
use crate::fail::{Fail, Result};

/// What `docket releases` was asked to do.
pub struct Releases<'a> {
    pub action: Option<&'a str>,
    pub name: Option<&'a str>,
    pub to: Option<&'a str>,
    pub move_open_to: Option<&'a str>,
    pub target: Option<&'a str>,
    pub note: Option<&'a str>,
    pub all: bool,
}

/// One release a line: its name, the target date, when it shipped and its note.
#[must_use]
pub fn line(r: &Release, current: bool) -> String {
    let mut parts = vec![r.name.clone()];
    if current {
        parts.push("current".to_string());
    }
    if let Some(d) = &r.target_date {
        parts.push(format!("target {d}"));
    }
    if let Some(s) = &r.shipped_at {
        parts.push(format!(
            "shipped {}",
            s.chars().take(10).collect::<String>()
        ));
    }
    parts.extend(r.note.clone());
    parts.join("  ")
}

/// The counts of a release's row: what is open under each word, closed, held by a later release,
/// and the pace.
#[must_use]
pub fn counts(r: &ReleaseRow) -> String {
    let words: Vec<String> = [
        ("ready", r.ready),
        ("in progress", r.in_progress),
        ("plans under way", r.under_way),
        ("owner", r.waiting_owner),
        ("blocked", r.blocked),
    ]
    .iter()
    .map(|(w, n)| format!("{w} {n}"))
    .collect();
    format!(
        "{} open ({}), {} closed, {} held later, pace {:+.1}/day",
        r.open,
        words.join(", "),
        r.closed,
        r.held_later,
        r.pace
    )
}

/// The text of the list: a line per release, an unshipped one followed by its counts.
#[must_use]
pub fn render(releases: &[Release], rows: &[ReleaseRow]) -> Vec<String> {
    let current = releases.iter().find(|r| r.shipped_at.is_none());
    let mut out = Vec::new();
    if releases.is_empty() {
        out.push("no releases: docket releases add 1.0.0".to_string());
    }
    for r in releases {
        let mut text = line(r, current.is_some_and(|c| c.name == r.name));
        if let Some(row) = rows.iter().find(|row| row.name == r.name) {
            text = format!("{text}\n    {}", counts(row));
        }
        out.push(text);
    }
    out
}

/// The releases as `--json` gives them: each with its row, null for a shipped one.
#[must_use]
pub fn rows_json(releases: &[Release], rows: &[ReleaseRow]) -> Value {
    Value::Array(
        releases
            .iter()
            .map(|r| {
                let mut out = json!(r);
                out["progress"] = json!(rows.iter().find(|row| row.name == r.name));
                out
            })
            .collect(),
    )
}

fn print(releases: &[Release]) {
    let current = releases
        .iter()
        .find(|r| r.shipped_at.is_none())
        .map(|r| r.name.clone());
    if releases.is_empty() {
        println!("no releases: docket releases add 1.0.0");
    }
    for r in releases {
        println!("{}", line(r, Some(&r.name) == current.as_ref()));
    }
}

/// # Errors
/// A write without a name, or the server refuses.
pub fn releases(ctx: &mut Ctx, a: &Releases) -> Result<i32> {
    let action = a.action.unwrap_or("list");
    if action == "list" {
        let slug = ctx.project()?;
        let found = ctx.api.get(
            "/releases",
            &[("project", slug.clone()), ("all", a.all.to_string())],
        )?;
        let rows: Vec<Row> = serde_json::from_value(found).unwrap_or_default();
        let all: Vec<Release> = rows.into_iter().map(|r| r.release).collect();
        let mut body = ctx
            .api
            .get(
                "/metrics",
                &[("project", slug), ("scope", "releases".into())],
            )
            .unwrap_or(Value::Null);
        let progress: Vec<ReleaseRow> =
            serde_json::from_value(body["releases"].take()).unwrap_or_default();
        if ctx.json {
            let out = rows_json(&all, &progress);
            println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        } else {
            for text in render(&all, &progress) {
                println!("{text}");
            }
        }
        return Ok(0);
    }
    let job = std::env::var("DOCKET_JOB").ok();
    if let Some(why) = crate::job::owner_plan_refusal("releases", action, job.as_deref()) {
        return Err(Fail::refused(why));
    }
    let Some(name) = a.name else {
        return Err(Fail::refused(format!(
            "docket releases {action} NAME: name the release"
        )));
    };
    let req = ReleasesRequest {
        common: ctx.common(false)?,
        action: action.to_string(),
        name: name.to_string(),
        to: a.to.or(a.move_open_to).map(str::to_string),
        target_date: a.target.map(str::to_string),
        note: a.note.map(str::to_string),
    };
    let out: ReleasesDone = ctx.api.post("releases", &req)?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return Ok(0);
    }
    if !out.moved.is_empty() {
        println!("moved {}", out.moved.join(", "));
    }
    print(&out.releases);
    Ok(0)
}

#[cfg(test)]
#[path = "../tests/releases.rs"]
mod tests;
