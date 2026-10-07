//! `docket job`: run, list, stop and read the jobs a lead starts on this machine.

use std::path::PathBuf;

use crate::args::JobCmd;
use crate::ctx::Ctx;
use crate::fail::{Fail, Result};
use crate::job::{self, Cap, Launch, Row, Spec};
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
            repo,
        } => run(
            flags,
            &root,
            &Asked {
                id,
                runner,
                model,
                effort: effort.as_deref(),
                role,
                repo: repo.as_deref(),
            },
        ),
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
        JobCmd::Where { repo } => {
            if flags.project.as_deref().is_none_or(str::is_empty) {
                return Err(Fail::refused("docket job where needs -p SLUG"));
            }
            let mut ctx = Ctx::new(flags.json, flags.project.clone(), None)?;
            let slug = ctx.project()?;
            println!("{}", checkout(&mut ctx, &slug, repo.as_deref())?.display());
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
        JobCmd::Report { dir } => report(dir),
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

/// What `docket job run` was asked for.
struct Asked<'a> {
    id: &'a str,
    runner: &'a str,
    model: &'a str,
    effort: Option<&'a str>,
    role: &'a str,
    repo: Option<&'a str>,
}

/// `docket job run`: the checkout found, the job started, its name and where it runs printed.
fn run(flags: &Flags, root: &std::path::Path, asked: &Asked) -> Result<i32> {
    let Asked {
        id,
        runner,
        model,
        effort,
        role,
        repo,
    } = *asked;
    let Some(branch) = flags.branch.clone().filter(|b| !b.is_empty()) else {
        return Err(Fail::refused(
            "docket job run needs --branch NAME, the branch the lead pushed here",
        ));
    };
    if flags.project.as_deref().is_none_or(str::is_empty) {
        return Err(Fail::refused("docket job run needs -p SLUG"));
    }
    let mut ctx = Ctx::new(flags.json, flags.project.clone(), Some(branch.clone()))?;
    let job_key = agent_key()?;
    let slug = ctx.project()?;
    let repo = repo
        .map(|r| docket_core::label::repo_path(r).map_err(|e| Fail::refused(e.0)))
        .transpose()?
        .flatten();
    let checkout = checkout(&mut ctx, &slug, repo.as_deref())?;
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
        repo,
        launch: Launch {
            reporter: std::env::current_exe().ok(),
            cap: Cap::detect(),
        },
    };
    let name = job::name_of(&spec.branch);
    let env = [
        ("DOCKET_PROJECT", slug.as_str()),
        ("DOCKET_JOB", name.as_str()),
        ("DOCKET_KEY", job_key.as_str()),
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

/// The key a job presents: the `job_key` line of the machine's client file, which the server must
/// know as an agent's. A job holding an owner key could answer as the owner, so none starts with
/// that key or without one.
fn agent_key() -> Result<String> {
    let config =
        docket_client::Config::load().map_err(|e| Fail::refused(format!("docket: {e}")))?;
    let file = docket_client::config::path().and_then(|p| std::fs::read_to_string(p).ok());
    let shown = docket_client::config::path().map_or_else(
        || "~/.config/docket/client".to_string(),
        |p| p.display().to_string(),
    );
    let Some(key) = file
        .as_deref()
        .and_then(|t| docket_client::config::field(t, "job_key"))
    else {
        return Err(Fail::refused(format!(
            "docket job run needs `job_key = ...` in {shown}: the agent key this machine's jobs present, so that no job holds an owner key"
        )));
    };
    let who: serde_json::Value = docket_client::Api::new(&docket_client::Config {
        server: config.server,
        key: key.clone(),
    })
    .and_then(|api| api.get("/whoami", &[]))
    .map_err(|e| {
        Fail::refused(format!(
            "docket job run: the job_key in {shown} is refused: {e}"
        ))
    })?;
    if who["owner"].as_bool() != Some(false) {
        return Err(Fail::refused(format!(
            "docket job run: the job_key in {shown} is an owner key; a job holds an agent key"
        )));
    }
    Ok(key)
}

/// `docket job report DIR`: the end of the job in `dir` posted to the claim of its item. The
/// reporter the job's wrapper runs, so it is the one place a job's end leaves its machine.
fn report(dir: &std::path::Path) -> Result<i32> {
    let Some(row) = job::row(dir, job::now()) else {
        return Err(Fail::refused(format!("{}: no job here", dir.display())));
    };
    if row.state == job::State::Running {
        return Err(Fail::refused(format!("{} is still running", row.name)));
    }
    let mut ctx = Ctx::new(false, Some(row.project.clone()), Some(row.branch.clone()))?;
    let common = ctx.common(false)?;
    let value = serde_json::to_value(&row).map_err(|e| Fail::refused(e.to_string()))?;
    let _: serde_json::Value = ctx.api.post(
        "job-report",
        &crate::cmd::dispatch::report_request(common, &row.id, &value),
    )?;
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

/// The project's checkout on this machine: its first root that exists, then the repository `repo`
/// names under it, else the `checkout` fact's. Each machine resolves the path against its own root,
/// so roots bound at different depths on different machines name the same repository.
fn checkout(ctx: &mut Ctx, slug: &str, repo: Option<&str>) -> Result<PathBuf> {
    let roots = ctx.roots.of(slug);
    let Some(root) = roots
        .iter()
        .find(|r| std::path::Path::new(r.as_str()).is_dir())
    else {
        return Err(Fail::refused(format!(
            "{slug} is not bound on this machine: clone it and run docket bind {slug} from its root"
        )));
    };
    if let Some(rel) = repo {
        return Ok(local::expand(root, rel));
    }
    let facts = ctx.api.get("/facts", &[("project", slug.to_string())])?;
    let rel = facts["skills"]["checkout"]
        .as_str()
        .filter(|c| !c.is_empty())
        .unwrap_or(".");
    Ok(local::expand(root, rel))
}
