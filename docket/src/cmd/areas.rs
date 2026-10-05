//! `docket areas`: a project's areas, listed, added, edited, moved to another place, or removed.

use docket_core::api::{AreasDone, AreasRequest};
use docket_core::area::{Area, Row};

use crate::ctx::Ctx;
use crate::fail::{Fail, Result};

/// What `docket areas` was asked to do.
pub struct Areas<'a> {
    pub action: Option<&'a str>,
    pub name: Option<&'a str>,
    pub about: Option<&'a str>,
    pub rename: Option<&'a str>,
    pub priority: Option<&'a str>,
    pub to: Option<i64>,
}

/// One area a line: its place counted from one, its name, its priority and what it holds.
#[must_use]
pub fn line(a: &Area) -> String {
    let mut parts = vec![format!("{:>2}", a.position + 1), a.name.clone()];
    parts.extend(a.priority.clone());
    parts.extend(a.description.clone());
    if a.history {
        parts.push("(closed items only)".to_string());
    }
    parts.join("  ")
}

fn print(areas: &[Area]) {
    if areas.is_empty() {
        println!("no areas: docket areas add NAME --about \"what it holds\"");
    }
    for a in areas {
        println!("{}", line(a));
    }
}

/// # Errors
/// A write without a name, inside a job, or the server refuses.
pub fn areas(ctx: &mut Ctx, a: &Areas) -> Result<i32> {
    let action = a.action.unwrap_or("list");
    if action == "list" {
        let slug = ctx.project()?;
        let found = ctx.api.get("/areas", &[("project", slug)])?;
        let rows: Vec<Row> = serde_json::from_value(found).unwrap_or_default();
        let all: Vec<Area> = rows.into_iter().map(|r| r.area).collect();
        if ctx.json {
            println!("{}", serde_json::to_string_pretty(&all).unwrap_or_default());
        } else {
            print(&all);
        }
        return Ok(0);
    }
    let job = std::env::var("DOCKET_JOB").ok();
    if let Some(why) = crate::job::owner_plan_refusal("areas", action, job.as_deref()) {
        return Err(Fail::refused(why));
    }
    let Some(name) = a.name else {
        return Err(Fail::refused(format!(
            "docket areas {action} NAME: name the area"
        )));
    };
    let req = AreasRequest {
        common: ctx.common(false)?,
        action: action.to_string(),
        name: name.to_string(),
        rename: a.rename.map(str::to_string),
        about: a.about.map(str::to_string),
        priority: a.priority.map(str::to_string),
        to: a.to,
    };
    let out: AreasDone = ctx.api.post("areas", &req)?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return Ok(0);
    }
    print(&out.areas);
    Ok(0)
}
