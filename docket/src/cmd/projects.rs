//! `docket projects rename OLD NEW`: a project moved to a new slug on the server, this machine's
//! roots rewritten to follow it, and what the server cannot reach printed as the steps left.

use docket_core::api::{Common, ProjectRenameRequest, ProjectRenamed};

use crate::args::ProjectsCmd;
use crate::ctx::Ctx;
use crate::fail::{Fail, Result};

/// Bare, the listing; with a verb, that verb.
///
/// # Errors
/// The server refuses, or the roots file cannot be written.
pub fn projects(ctx: &mut Ctx, what: Option<&ProjectsCmd>) -> Result<i32> {
    match what {
        None => super::show::projects(ctx),
        Some(ProjectsCmd::Rename {
            old,
            new,
            force,
            dry_run,
        }) => rename(ctx, old, new, *force, *dry_run),
    }
}

fn rename(ctx: &mut Ctx, old: &str, new: &str, force: bool, dry_run: bool) -> Result<i32> {
    let job = std::env::var("DOCKET_JOB").ok();
    if let Some(why) = crate::job::owner_plan_refusal("projects", "rename", job.as_deref()) {
        return Err(Fail::refused(why));
    }
    let slugs = ctx.api.slugs()?;
    let known = |slug: &str| slugs.iter().any(|s| s == slug);
    if !known(old) && known(new) {
        let moved = rewrite_roots(ctx, old, new)?;
        println!(
            "{new} is already the slug on the server; {moved} {} on this machine now name it",
            roots_word(moved)
        );
        return Ok(0);
    }
    let req = ProjectRenameRequest {
        common: Common {
            project: old.to_string(),
            branch: Some(ctx.branch()),
            force,
        },
        new: new.to_string(),
        dry_run,
    };
    let out: ProjectRenamed = ctx.api.post("projects-rename", &req)?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        print_moved(&out);
    }
    if dry_run {
        println!("dry run: nothing changed");
        return Ok(0);
    }
    let moved = rewrite_roots(ctx, old, new)?;
    if !ctx.json {
        println!(
            "{old} -> {new}: {moved} {} on this machine rewritten",
            roots_word(moved)
        );
        for line in left_to_do(old, new, &crate::job::state_root()) {
            println!("{line}");
        }
    }
    Ok(0)
}

/// This machine's roots of the old slug moved to the new one.
fn rewrite_roots(ctx: &mut Ctx, old: &str, new: &str) -> Result<usize> {
    ctx.roots.rename(old, new).map_err(|e| {
        Fail::refused(format!(
            "docket: cannot write {}: {e}",
            ctx.roots.file().display()
        ))
    })
}

fn roots_word(n: usize) -> &'static str {
    if n == 1 { "root" } else { "roots" }
}

fn print_moved(out: &ProjectRenamed) {
    let verb = if out.dry_run { "would move" } else { "moved" };
    let counts: Vec<String> = out
        .moved
        .iter()
        .filter(|(_, n)| **n > 0)
        .map(|(table, n)| format!("{table} {n}"))
        .collect();
    println!("{verb} {} -> {}: {}", out.old, out.new, counts.join(", "));
    for l in &out.labels {
        println!("  {}: {} -> {}", l.project, l.old, l.new);
    }
}

/// What the server cannot reach after a rename, for the person to do by hand.
#[must_use]
pub fn left_to_do(old: &str, new: &str, state: &std::path::Path) -> Vec<String> {
    let mut out = vec!["Left to do by hand:".to_string()];
    let jobs = crate::job::project_dir(state, old);
    if jobs.is_dir() {
        out.push(format!(
            "  mv {} {}   # the job state directory",
            jobs.display(),
            crate::job::project_dir(state, new).display()
        ));
    }
    out.push(format!(
        "  in the dump checkout: git mv {old} {new}, or a full dump pass, then remove {old}/"
    ));
    out.push(format!(
        "  on every other machine: docket projects rename {old} {new}   # rewrites its roots"
    ));
    out
}

#[cfg(test)]
#[path = "../tests/projects.rs"]
mod tests;
