//! `docket admin migrate`: what moving each project's rows onto the core changes, read from the full
//! dump the server serves in the shape the plan reads (`OldPage`) and planned by
//! `docket_core::migrate`, with every risky case.

use std::io::Write;
use std::process::{Command, Stdio};

use docket_core::api::{RemapRequest, Remapped};
use docket_core::dump::DumpPage;
use docket_core::migrate::{Changes, OldPage, Rows, Rules, Themes, plan};
use docket_core::remap::Map;

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
        AdminCmd::Remap { mapfile, dry_run } => remap(ctx, mapfile, *dry_run),
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

/// How many rows a dry run prints.
const SHOWN: usize = 10;

fn remap(ctx: &mut Ctx, mapfile: &str, dry_run: bool) -> Result<i32> {
    let text = std::fs::read_to_string(mapfile)
        .map_err(|e| Fail::refused(format!("cannot read {mapfile}: {e}")))?;
    let map = Map::parse(&text).map_err(|e| Fail::refused(format!("{mapfile}: {e}")))?;
    let missing = unresolved(&map)?;
    if !missing.is_empty() {
        return Err(Fail::refused(format!(
            "{} of the map's new shas are no commit in this checkout, so nothing is changed. First: {}",
            missing.len(),
            missing[0]
        )));
    }
    let req = RemapRequest {
        common: ctx.common(false)?,
        map: map
            .pairs()
            .map(|(o, n)| (o.to_string(), n.to_string()))
            .collect(),
        dry_run,
    };
    let out: Remapped = ctx.api.post("remap", &req)?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return Ok(0);
    }
    let verb = if dry_run { "would remap" } else { "remapped" };
    println!("{verb} {} items", out.rows.len());
    if dry_run {
        for r in out.rows.iter().take(SHOWN) {
            println!("{}  {} -> {}", r.id, r.old, r.new);
        }
    }
    Ok(0)
}

/// The new shas of the map that no commit in the working directory's repository answers to.
fn unresolved(map: &Map) -> Result<Vec<String>> {
    let mut child = Command::new("git")
        .args(["cat-file", "--batch-check"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Fail::refused(format!("git: {e}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Fail::refused("git: no stdin"))?;
    let news: Vec<String> = map.pairs().map(|(_, n)| n.to_string()).collect();
    let feeder = std::thread::spawn(move || {
        for n in &news {
            if writeln!(stdin, "{n}").is_err() {
                break;
            }
        }
    });
    let out = child
        .wait_with_output()
        .map_err(|e| Fail::refused(format!("git: {e}")))?;
    let _ = feeder.join();
    if !out.status.success() {
        return Err(Fail::refused(
            "git cat-file failed: run this inside the project's checkout",
        ));
    }
    Ok(not_commits(&String::from_utf8_lossy(&out.stdout)))
}

/// The shas a `git cat-file --batch-check` answer reports missing or as anything but a commit.
#[must_use]
pub fn not_commits(answer: &str) -> Vec<String> {
    answer
        .lines()
        .filter_map(|l| {
            let mut p = l.split_whitespace();
            let sha = p.next()?;
            (p.next() != Some("commit")).then(|| sha.to_string())
        })
        .collect()
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
