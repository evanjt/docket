//! `docket job`: run, list, stop and read the jobs a lead starts on this machine.

use std::path::PathBuf;

use crate::args::JobCmd;
use crate::ctx::Ctx;
use crate::fail::{Fail, Result};
use crate::job::{self, Row, Spec};
use crate::local;

/// The flags `docket job` reads from the command line around it.
pub struct Flags {
    pub json: bool,
    pub project: Option<String>,
    pub branch: Option<String>,
}

/// # Errors
/// The job cannot be started, found or stopped.
pub fn job(flags: &Flags, what: &JobCmd) -> Result<i32> {
    let root = job::state_root();
    match what {
        JobCmd::Run {
            id,
            runner,
            model,
            effort,
            role,
        } => run(flags, &root, id, runner, model, effort.as_deref(), role),
        JobCmd::Status { job: name } => {
            let rows: Vec<Row> = job::rows(&root, job::now())
                .into_iter()
                .map(|(r, _)| r)
                .filter(|r| name.as_ref().is_none_or(|n| &r.name == n))
                .filter(|r| flags.project.as_ref().is_none_or(|p| &r.project == p))
                .collect();
            if flags.json {
                println!("{}", serde_json::to_string(&rows).unwrap_or_default());
            } else if rows.is_empty() {
                println!("no jobs on this machine");
            } else {
                for r in &rows {
                    println!("{}", line(r));
                }
            }
            Ok(0)
        }
        JobCmd::Where => {
            if flags.project.as_deref().is_none_or(str::is_empty) {
                return Err(Fail::refused("docket job where needs -p SLUG"));
            }
            let mut ctx = Ctx::new(flags.json, flags.project.clone(), None)?;
            let slug = ctx.project()?;
            println!("{}", checkout(&mut ctx, &slug)?.display());
            Ok(0)
        }
        JobCmd::Remove {
            job: name,
            keep_branch,
        } => {
            let dir = job::find(&root, name, flags.project.as_deref()).map_err(Fail::refused)?;
            job::remove(&dir, *keep_branch).map_err(Fail::refused)?;
            println!("removed {name}");
            Ok(0)
        }
        JobCmd::Diff { job: name } => {
            let dir = job::find(&root, name, flags.project.as_deref()).map_err(Fail::refused)?;
            let change = job::diff(&dir).map_err(Fail::refused)?;
            if flags.json {
                println!("{}", serde_json::to_string(&change).unwrap_or_default());
            } else {
                print!("{}", change.text());
            }
            Ok(0)
        }
        JobCmd::Kill { job: name } => {
            let dir = job::find(&root, name, flags.project.as_deref()).map_err(Fail::refused)?;
            job::kill(&dir).map_err(Fail::refused)?;
            println!("killed {name}");
            Ok(0)
        }
        JobCmd::Log { job: name, n } => {
            let dir = job::find(&root, name, flags.project.as_deref()).map_err(Fail::refused)?;
            println!("{}", job::tail(&dir, *n));
            Ok(0)
        }
    }
}

/// `docket job run`: the checkout found, the job started, its name and where it runs printed.
fn run(
    flags: &Flags,
    root: &std::path::Path,
    id: &str,
    runner: &str,
    model: &str,
    effort: Option<&str>,
    role: &str,
) -> Result<i32> {
    let Some(branch) = flags.branch.clone().filter(|b| !b.is_empty()) else {
        return Err(Fail::refused(
            "docket job run needs --branch NAME, the branch the lead pushed here",
        ));
    };
    if flags.project.as_deref().is_none_or(str::is_empty) {
        return Err(Fail::refused("docket job run needs -p SLUG"));
    }
    let mut ctx = Ctx::new(flags.json, flags.project.clone(), Some(branch.clone()))?;
    let slug = ctx.project()?;
    let checkout = checkout(&mut ctx, &slug)?;
    let provision = ctx.api.facts(&slug)?.skills.get("provision").cloned();
    let spec = Spec {
        project: slug.clone(),
        id: crate::ctx::id(id)?,
        branch,
        runner: runner.to_string(),
        model: model.to_string(),
        effort: effort.filter(|e| !e.is_empty()).map(str::to_string),
        role: role.to_string(),
        provision,
    };
    let name = job::name_of(&spec.branch);
    let env = [
        ("DOCKET_PROJECT", slug.as_str()),
        ("DOCKET_JOB", name.as_str()),
    ];
    let started =
        job::run(&spec, &checkout, root, &env, &job::program(runner)).map_err(Fail::refused)?;
    if flags.json {
        let out = serde_json::json!({
            "name": started.name,
            "dir": started.dir.display().to_string(),
            "worktree": started.worktree.display().to_string(),
            "pid": started.child.id(),
        });
        println!("{out}");
    } else {
        println!(
            "started {} (pid {}): {} on {}, {} {}{}, in {}",
            started.name,
            started.child.id(),
            spec.id,
            spec.branch,
            spec.runner,
            spec.model,
            spec.effort.map(|e| format!(" {e}")).unwrap_or_default(),
            started.worktree.display()
        );
    }
    Ok(0)
}

/// One job as a line: name, state, item, runner and model, minutes, tokens and report.
#[must_use]
pub fn line(r: &Row) -> String {
    let tokens = r.tokens.map(|t| format!("{t} out")).unwrap_or_default();
    format!(
        "{:<24} {:<8} {:<6} {:<7} {:<6} {:<22} {:>4}m {:>10}  {}",
        r.name,
        r.state.word(),
        r.id,
        r.role,
        r.runner,
        r.model,
        r.minutes,
        tokens,
        r.report.as_deref().unwrap_or("")
    )
}

/// The project's checkout on this machine: its first root that exists, then the `checkout` fact.
fn checkout(ctx: &mut Ctx, slug: &str) -> Result<PathBuf> {
    let roots = ctx.roots.of(slug);
    let Some(root) = roots
        .iter()
        .find(|r| std::path::Path::new(r.as_str()).is_dir())
    else {
        return Err(Fail::refused(format!(
            "{slug} is not bound on this machine: clone it and run docket bind {slug} from its root"
        )));
    };
    let facts = ctx.api.get("/facts", &[("project", slug.to_string())])?;
    let rel = facts["skills"]["checkout"]
        .as_str()
        .filter(|c| !c.is_empty())
        .unwrap_or(".");
    Ok(local::expand(root, rel))
}
