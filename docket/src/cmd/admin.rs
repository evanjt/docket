//! `docket admin migrate`: what moving each project's rows onto the core changes, read from the full
//! dump the server serves in the shape the plan reads (`OldPage`) and planned by
//! `docket_core::migrate`, with every risky case.

use docket_core::dump::DumpPage;
use docket_core::migrate::{Changes, OldPage, Rows, Rules, Themes, plan};

use crate::args::AdminCmd;
use crate::ctx::Ctx;
use crate::fail::{Fail, Result};
use crate::py::Py;

/// # Errors
/// The server cannot be reached, the project is not in the dump, or a write is asked for.
pub fn admin(ctx: &mut Ctx, what: &AdminCmd) -> Result<i32> {
    match what {
        AdminCmd::Migrate {
            dry_run,
            all,
            themes,
        } => migrate(ctx, *dry_run, *all, rules(themes.as_deref())),
    }
}

/// The rules the flags decide; a flag not given leaves the rule at its default.
#[must_use]
pub fn rules(themes: Option<&str>) -> Rules {
    Rules {
        themes: match themes {
            Some("backlog") => Themes::Backlog,
            _ => Themes::Plan,
        },
    }
}

fn migrate(ctx: &mut Ctx, dry_run: bool, all: bool, rules: Rules) -> Result<i32> {
    let only = if all { None } else { Some(ctx.project()?) };
    let v = ctx.api.get("/dump", &[("since", "0".to_string())])?;
    let page: DumpPage =
        serde_json::from_value(v).map_err(|e| Fail::refused(format!("/dump: {e}")))?;
    let page = OldPage::of(&page);
    let planned: Vec<Changes> = Rows::of(&page)
        .iter()
        .filter(|r| only.as_ref().is_none_or(|s| *s == r.project.slug))
        .map(|r| plan(r, rules))
        .collect();
    if let Some(slug) = only.filter(|_| planned.is_empty()) {
        return Err(Fail::refused(format!("{slug} is not in the server's dump")));
    }
    if ctx.json {
        ctx.emit(&Py::from_value(
            &serde_json::to_value(&planned).unwrap_or_default(),
        ));
    } else {
        for c in &planned {
            for line in c.lines() {
                println!("{line}");
            }
        }
    }
    if dry_run {
        return Ok(0);
    }
    Err(Fail::refused(refusal(&planned)))
}

/// Why nothing is written: each project's waiting cases, or, with none waiting, that the writes are
/// each step's own.
#[must_use]
pub fn refusal(planned: &[Changes]) -> String {
    let waiting: Vec<String> = planned
        .iter()
        .filter_map(|c| c.writable().err().map(|r| r.0))
        .collect();
    if waiting.is_empty() {
        "nothing is written here: each step of the move applies its own slice of the plan; \
         --dry-run prints it"
            .to_string()
    } else {
        waiting.join("\n")
    }
}

#[cfg(test)]
#[path = "../tests/admin.rs"]
mod tests;
