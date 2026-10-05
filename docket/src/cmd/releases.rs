//! `docket releases`: a project's releases, listed, added, their open items moved, or shipped.

use docket_core::api::{ReleasesDone, ReleasesRequest};
use docket_core::release::{Release, Row};

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
            &[("project", slug), ("all", a.all.to_string())],
        )?;
        let rows: Vec<Row> = serde_json::from_value(found).unwrap_or_default();
        let all: Vec<Release> = rows.into_iter().map(|r| r.release).collect();
        if ctx.json {
            println!("{}", serde_json::to_string_pretty(&all).unwrap_or_default());
        } else {
            print(&all);
        }
        return Ok(0);
    }
    let job = std::env::var("DOCKET_JOB").ok();
    if let Some(why) = crate::job::releases_refusal(action, job.as_deref()) {
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
