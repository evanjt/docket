//! `docket dispatch`, `jobs` and `collect`: a lead's claims, the code moved by git over ssh from the
//! lead's machine, and every machine's jobs read where they run.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use docket_core::api::{Common, ReleaseRequest, StartRequest, Started};
use docket_core::fact::{self, Model, Role};
use docket_core::machine::Machine;

use crate::ctx::Ctx;
use crate::dispatch::{Via, branch_for, choose, git, nonce, role_of};
use crate::fail::{Fail, Result};
use crate::job;

/// What `docket dispatch` was asked for.
pub struct Ask<'a> {
    pub id: &'a str,
    pub on: Option<&'a str>,
    pub runner: Option<&'a str>,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub role: Option<&'a str>,
}

/// Claim an item on a fresh branch, push the base to a machine and start its job there.
///
/// # Errors
/// The item cannot be claimed, no machine can take it, or the push or the start fails, in which
/// case the claim is given back.
pub fn dispatch(ctx: &mut Ctx, ask: &Ask) -> Result<i32> {
    let slug = ctx.project()?;
    let here = ctx.host()?;
    let id = crate::ctx::id(ask.id)?;
    let item: Value = ctx
        .api
        .get(&format!("/show/{id}"), &[("project", slug.clone())])?;
    if let Some(branch) = item["claim_branch"].as_str() {
        return Err(Fail::refused(format!(
            "{id} is held by {branch} on {}: give it back or take another",
            item["claim_host"].as_str().unwrap_or("?")
        )));
    }
    let key = item["key"].as_str().unwrap_or_default();
    let role = match ask.role {
        Some(r) => r.to_string(),
        None => role_of(&kind_of(&ctx.project_row(&slug)?, key)).to_string(),
    };
    let skills = ctx.api.facts(&slug)?.skills;
    let model = model(ask, &skills, &role, item["complexity"].as_str())?;
    let machines = machines(ctx)?;
    let running = running(&machines, &here);
    let m = pick(&machines, &running, ask.on, &model.runner, &here)?;
    let via = Via::of(m, &here);
    let checkout = via
        .docket(&strings(&["-p", &slug, "job", "where"]))
        .map_err(Fail::refused)?;
    let repo = repo()?;
    let base = git(&repo, &["rev-parse", "HEAD"]).map_err(Fail::refused)?;
    let branch = branch_for(&id, nonce());
    let name = job::name_of(&branch);
    let common = Common {
        project: slug.clone(),
        branch: Some(branch.clone()),
        force: false,
    };
    let _: Started = ctx.api.post(
        "start",
        &StartRequest {
            common: common.clone(),
            id: id.clone(),
            runner: Some(model.runner.clone()),
            job: Some(name.clone()),
            model: Some(model.model.clone()),
            on: Some(m.name.clone()),
            role: Some(role.clone()),
        },
    )?;
    let url = via.git_url(&checkout);
    let started = start(
        &via,
        &repo,
        &url,
        &base,
        &Job {
            slug: &slug,
            id: &id,
            branch: &branch,
            role: &role,
            model: &model,
        },
    );
    if let Err(why) = started {
        let _ = git(&repo, &["push", &url, &format!(":refs/heads/{branch}")]);
        let _: Value = ctx.api.post(
            "release",
            &ReleaseRequest {
                common,
                id: id.clone(),
                note: Some(format!("dispatch to {} failed: {why}", m.name)),
                ..ReleaseRequest::default()
            },
        )?;
        return Err(Fail::refused(format!(
            "dispatch of {id} to {} failed, the claim given back: {why}",
            m.name
        )));
    }
    if ctx.json {
        let out = serde_json::json!({
            "id": id, "machine": m.name, "branch": branch, "job": name, "base": base,
            "runner": model.runner, "model": model.model, "effort": model.effort, "role": role,
        });
        println!("{out}");
    } else {
        println!(
            "dispatched {id} to {}: {role} on {branch} from {}, {} {}{}, job {name}",
            m.name,
            &base[..base.len().min(10)],
            model.runner,
            model.model,
            model
                .effort
                .as_deref()
                .map(|e| format!(" {e}"))
                .unwrap_or_default()
        );
    }
    Ok(0)
}

/// The project's jobs on every machine, or every project's; with `wait`, read again until one of
/// the jobs running at the start ends, or `timeout` seconds pass when it is not 0.
///
/// # Errors
/// The server cannot be reached for the machines.
pub fn jobs(ctx: &mut Ctx, wait: bool, every: u64, timeout: u64, all: bool) -> Result<i32> {
    let slug = if all { None } else { Some(ctx.project()?) };
    let here = ctx.host()?;
    let machines = machines(ctx)?;
    let mut rows = read_all(&machines, &here, slug.as_deref());
    let live: BTreeSet<(String, String)> = running_of(&rows);
    if wait && !live.is_empty() {
        let until = (timeout > 0).then(|| Instant::now() + Duration::from_secs(timeout));
        loop {
            if until.is_some_and(|u| Instant::now() >= u) {
                break;
            }
            std::thread::sleep(Duration::from_secs(every.max(1)));
            rows = read_all(&machines, &here, slug.as_deref());
            let now = running_of(&rows);
            if !live.is_subset(&now) {
                break;
            }
        }
    }
    if ctx.json {
        println!("{}", Value::Array(rows));
    } else if rows.is_empty() {
        println!("no jobs");
    } else {
        for r in &rows {
            println!("{}", line(r));
        }
    }
    Ok(0)
}

/// Fetch the branch a dispatched item's job worked on into this repository, print its commits and
/// the job's report, and with `remove` clear the job from its machine.
///
/// # Errors
/// The item has no job's claim, its machine is unknown or unreachable, or the fetch fails.
pub fn collect(ctx: &mut Ctx, id: &str, remove: bool) -> Result<i32> {
    let slug = ctx.project()?;
    let here = ctx.host()?;
    let id = crate::ctx::id(id)?;
    let item: Value = ctx
        .api
        .get(&format!("/show/{id}"), &[("project", slug.clone())])?;
    let (Some(branch), Some(on), Some(name)) = (
        item["claim_branch"].as_str(),
        item["claim_on"].as_str(),
        item["claim_job"].as_str(),
    ) else {
        return Err(Fail::refused(format!(
            "{id} holds no job's claim: docket dispatch {id} starts one"
        )));
    };
    let machines = machines(ctx)?;
    let Some(m) = machines.iter().find(|m| m.name == on) else {
        return Err(Fail::refused(format!(
            "{id}'s job ran on {on}, which is not a machine: docket machines"
        )));
    };
    let via = Via::of(m, &here);
    let checkout = via
        .docket(&strings(&["-p", &slug, "job", "where"]))
        .map_err(Fail::refused)?;
    let repo = repo()?;
    let same = via == Via::Here && same_repo(&repo, Path::new(&checkout));
    if !same {
        git(
            &repo,
            &[
                "fetch",
                &via.git_url(&checkout),
                &format!("+refs/heads/{branch}:refs/heads/{branch}"),
            ],
        )
        .map_err(Fail::refused)?;
    }
    let commits =
        git(&repo, &["log", "--oneline", &format!("HEAD..{branch}")]).map_err(Fail::refused)?;
    let report = via
        .docket(&strings(&["-p", &slug, "job", "status", name, "--json"]))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()));
    if remove {
        let mut args = strings(&["-p", &slug, "job", "remove", name]);
        if same {
            args.push("--keep-branch".into());
        }
        via.docket(&args).map_err(Fail::refused)?;
    }
    if ctx.json {
        let out = serde_json::json!({
            "id": id, "branch": branch, "machine": on, "job": name,
            "commits": commits.lines().collect::<Vec<_>>(), "status": report, "removed": remove,
        });
        println!("{out}");
        return Ok(0);
    }
    println!("{id}: {branch} from {on}, job {name}");
    if commits.is_empty() {
        println!("no commits ahead of HEAD");
    } else {
        println!("{commits}");
    }
    if let Some(r) = report {
        println!(
            "{} {}",
            r["state"].as_str().unwrap_or("?"),
            r["report"].as_str().unwrap_or("no report")
        );
        if let Some(note) = r["note"].as_str() {
            println!("{note}");
        }
    }
    if remove {
        println!("removed {name} from {on}");
    }
    Ok(0)
}

/// What a job is started with on its machine.
struct Job<'a> {
    slug: &'a str,
    id: &'a str,
    branch: &'a str,
    role: &'a str,
    model: &'a Model,
}

/// The base pushed to the machine's checkout as the job's branch, then the job started there.
fn start(
    via: &Via,
    repo: &Path,
    url: &str,
    base: &str,
    job: &Job,
) -> std::result::Result<String, String> {
    git(
        repo,
        &["push", url, &format!("{base}:refs/heads/{}", job.branch)],
    )?;
    let mut args = strings(&[
        "-p", job.slug, "--branch", job.branch, "job", "run", "--id", job.id, "--runner",
    ]);
    args.extend(strings(&[
        &job.model.runner,
        "--model",
        &job.model.model,
        "--role",
        job.role,
    ]));
    if let Some(e) = &job.model.effort {
        args.extend(strings(&["--effort", e]));
    }
    via.docket(&args)
}

/// The model the job runs on: the one asked for, else the `models` fact's for its role and
/// complexity.
fn model(
    ask: &Ask,
    skills: &BTreeMap<String, String>,
    role: &str,
    complexity: Option<&str>,
) -> Result<Model> {
    if let (Some(runner), Some(model)) = (ask.runner, ask.model) {
        return Ok(Model {
            runner: runner.to_string(),
            model: model.to_string(),
            effort: ask.effort.map(str::to_string),
        });
    }
    let role_of = match role {
        "audit" => Role::Audit,
        "plan" => Role::Plan,
        _ => Role::Build,
    };
    let Some(mut m) = fact::model_for(skills, role_of, complexity) else {
        return Err(Fail::refused(
            "no model for this job: set the models fact (docket skills set models \"high=claude:MODEL:high ...\") \
             or pass --runner and --model",
        ));
    };
    if let Some(runner) = ask.runner.filter(|r| *r != m.runner) {
        return Err(Fail::refused(format!(
            "the models fact runs this on {}, not {runner}: pass --model with --runner",
            m.runner
        )));
    }
    if let Some(e) = ask.effort {
        m.effort = Some(e.to_string());
    }
    Ok(m)
}

fn machines(ctx: &mut Ctx) -> Result<Vec<Machine>> {
    let listed: docket_core::api::Machines = serde_json::from_value(ctx.api.get("/machines", &[])?)
        .map_err(|e| Fail::refused(format!("/machines: {e}")))?;
    let machines = listed.machines;
    if machines.is_empty() {
        return Err(Fail::refused(
            "no machines: the owner adds one with docket machine set NAME --ssh ADDR --slots N --runners claude,codex",
        ));
    }
    Ok(machines)
}

/// The machine asked for, when it has the runner and a free slot, or the one `choose` picks.
fn pick<'a>(
    machines: &'a [Machine],
    running: &BTreeMap<String, usize>,
    on: Option<&str>,
    runner: &str,
    here: &str,
) -> Result<&'a Machine> {
    let Some(name) = on else {
        return choose(machines, running, runner, here).ok_or_else(|| {
            Fail::refused(format!(
                "no machine has {runner} and a free slot: docket jobs shows what runs"
            ))
        });
    };
    let Some(m) = machines.iter().find(|m| m.name == name) else {
        return Err(Fail::refused(format!("no machine {name}: docket machines")));
    };
    if !m.runners.iter().any(|r| r == runner) {
        return Err(Fail::refused(format!("{name} has no {runner}")));
    }
    if crate::dispatch::free(m, running) == 0 {
        return Err(Fail::refused(format!(
            "{name} runs {} jobs, its slots: docket jobs",
            m.slots
        )));
    }
    Ok(m)
}

/// The jobs running on each machine, every project's; a machine that cannot be read counts as full.
fn running(machines: &[Machine], here: &str) -> BTreeMap<String, usize> {
    machines
        .iter()
        .map(|m| {
            let n = read(m, here, None).map_or(usize::MAX, |rows| {
                rows.iter().filter(|r| r["state"] == "running").count()
            });
            (m.name.clone(), n)
        })
        .collect()
}

/// One machine's jobs, each with the machine's name added.
fn read(m: &Machine, here: &str, slug: Option<&str>) -> std::result::Result<Vec<Value>, String> {
    let mut args = Vec::new();
    if let Some(s) = slug {
        args.extend(strings(&["-p", s]));
    }
    args.extend(strings(&["job", "status", "--json"]));
    let text = Via::of(m, here).docket(&args)?;
    let rows: Vec<Value> = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", m.name))?;
    Ok(rows
        .into_iter()
        .map(|mut r| {
            r["machine"] = Value::String(m.name.clone());
            r
        })
        .collect())
}

fn read_all(machines: &[Machine], here: &str, slug: Option<&str>) -> Vec<Value> {
    let mut out = Vec::new();
    for m in machines {
        match read(m, here, slug) {
            Ok(rows) => out.extend(rows),
            Err(why) => eprintln!("docket jobs: {} cannot be read: {why}", m.name),
        }
    }
    out
}

fn running_of(rows: &[Value]) -> BTreeSet<(String, String)> {
    rows.iter()
        .filter(|r| r["state"] == "running")
        .map(|r| {
            (
                r["machine"].as_str().unwrap_or_default().to_string(),
                r["name"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

/// One job as a line: machine, name, state, item, role, runner and model, minutes and report.
#[must_use]
pub fn line(r: &Value) -> String {
    let s = |k: &str| r[k].as_str().unwrap_or("").to_string();
    format!(
        "{:<12} {:<24} {:<8} {:<6} {:<6} {:<6} {:<22} {:>4}m  {}",
        s("machine"),
        s("name"),
        s("state"),
        s("id"),
        s("role"),
        s("runner"),
        s("model"),
        r["minutes"].as_u64().unwrap_or(0),
        s("report")
    )
}

/// The kind a key holds in a project's matrix.
fn kind_of(project: &Value, key: &str) -> String {
    project["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|k| k["key"] == key)
        .and_then(|k| k["kind"].as_str())
        .unwrap_or("work")
        .to_string()
}

/// The repository this command runs in, the one the lead merges into.
fn repo() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| Fail::refused(e.to_string()))?;
    git(&cwd, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .map_err(|_| {
            Fail::refused("docket dispatch and collect run inside the lead's git checkout")
        })
}

fn same_repo(a: &Path, b: &Path) -> bool {
    let common = |p: &Path| {
        git(
            p,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .ok()
        .and_then(|c| std::fs::canonicalize(c).ok())
    };
    common(a).is_some() && common(a) == common(b)
}

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_string()).collect()
}
