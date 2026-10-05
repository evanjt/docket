//! `docket dispatch`, `jobs` and `collect`: a lead's claims, the code moved by git over ssh from the
//! lead's machine, and every machine's jobs read where they run.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use docket_core::api::Facts;
use docket_core::api::{Common, EditRequest, ReleaseRequest, StartRequest, Started};
use docket_core::fact::{self, Model, Role};
use docket_core::machine::Machine;

use crate::ctx::Ctx;
use crate::dispatch::{Via, branch_for, choose, commit_change, git, nonce, role_of};
use crate::fail::{Fail, Result};
use crate::job::{self, Change};

/// What `docket dispatch` was asked for.
pub struct Ask<'a> {
    pub id: &'a str,
    pub on: Option<&'a str>,
    pub runner: Option<&'a str>,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub role: Option<&'a str>,
    pub force: bool,
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
    let facts = ctx.api.facts(&slug)?;
    let model = model(ask, &facts, &role, item["complexity"].as_str())?;
    let machines = machines(ctx)?;
    refuse_a_running_group(ctx, &item, &machines, &here, ask.force)?;
    let (running, unread) = running(&machines, &here);
    let m = pick(&machines, &running, &unread, ask.on, &model.runner, &here)?;
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

/// Bring a finished job's change to this machine and commit it here, on the job's branch, with the
/// message the job proposed; then clear the job from its machine. Commits are only ever made on the
/// lead's machine: a job leaves its change in its worktree. A change inside a submodule is committed
/// in this checkout's repository of that submodule, on a branch of the job's name, and the job's
/// commit points at it. A job from before that rule, which committed on its own branch, has that
/// branch fetched instead. With `discard`, the job is cleared and nothing is committed. A change
/// that cannot be committed here leaves the job where it ran, to collect again.
///
/// # Errors
/// The item has no job's claim, its job is still running or cannot be read, it proposed no message,
/// or git fails.
pub fn collect(ctx: &mut Ctx, id: &str, discard: bool) -> Result<i32> {
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
    let row = via
        .docket(&strings(&["-p", &slug, "job", "status", name, "--json"]))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .ok_or_else(|| Fail::refused(format!("no job {name} on {on}: docket jobs")))?;
    if row["state"] == "running" {
        return Err(Fail::refused(format!(
            "{name} is still running on {on}: docket jobs --wait"
        )));
    }
    let checkout = via
        .docket(&strings(&["-p", &slug, "job", "where"]))
        .map_err(Fail::refused)?;
    let repo = repo()?;
    let same = via == Via::Here && same_repo(&repo, Path::new(&checkout));
    let committed = if discard {
        remove(&via, &slug, name, same)?;
        None
    } else if let Some(base) = row["base"].as_str() {
        let ran = Ran {
            via: &via,
            slug: &slug,
            name,
            on,
            branch,
            repo: &repo,
            same,
        };
        take(&ran, &row, base)?
    } else {
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
        remove(&via, &slug, name, same)?;
        None
    };
    append_observations(ctx, &id, name, &row)?;
    let reference = format!("refs/heads/{branch}");
    let commits = if git(&repo, &["rev-parse", "--verify", "--quiet", &reference]).is_ok() {
        git(&repo, &["log", "--oneline", &format!("HEAD..{branch}")]).map_err(Fail::refused)?
    } else {
        String::new()
    };
    let said = Collected {
        id: &id,
        branch,
        on,
        name,
        committed: committed.as_deref(),
        commits: &commits,
        row: &row,
        discard,
    };
    said.print(ctx.json);
    Ok(0)
}

/// A job that ended, as a collect reaches it.
struct Ran<'a> {
    via: &'a Via,
    slug: &'a str,
    name: &'a str,
    on: &'a str,
    branch: &'a str,
    repo: &'a Path,
    /// Whether the job's worktree belongs to this repository.
    same: bool,
}

/// The job's change committed here on `base`, then the job cleared, so a change that cannot be
/// committed leaves the job where it ran. Returns the short sha, `None` for an empty change.
fn take(ran: &Ran, row: &Value, base: &str) -> Result<Option<String>> {
    let (name, on, branch, repo) = (ran.name, ran.on, ran.branch, ran.repo);
    let message = row["message"]
        .as_str()
        .or_else(|| row["note"].as_str())
        .filter(|m| !m.is_empty());
    let said = ran
        .via
        .docket(&strings(&["-p", ran.slug, "job", "diff", name, "--json"]))
        .map_err(Fail::refused)?;
    let change = read_change(&said);
    if !change.is_empty() && message.is_none() {
        return Err(Fail::refused(format!(
            "{name} left a change but proposed no MESSAGE line to commit it with: docket job log {name} on {on}"
        )));
    }
    let made = (!change.is_empty())
        .then(|| commit_change(repo, base, &change, message.unwrap_or_default()))
        .transpose()
        .map_err(|why| {
            Fail::refused(format!(
                "{name}'s change was not committed here, and the job is kept on {on}: {why}"
            ))
        })?;
    let keep = || match &made {
        Some(m) => m.keep(repo, branch),
        None => git(repo, &["branch", "-f", branch, base]).map(|_| ()),
    };
    // A branch checked out in the job's own worktree cannot be moved until that worktree is gone.
    if !ran.same {
        keep().map_err(|why| {
            Fail::refused(format!("{why}: the job is kept on {on}, collect it again"))
        })?;
    }
    remove(ran.via, ran.slug, name, ran.same)?;
    if ran.same {
        keep().map_err(|why| {
            let sha = made.as_ref().map_or(base, |m| m.sha.as_str());
            Fail::refused(format!(
                "{why}: the change is {sha}, git branch -f {branch} {sha}"
            ))
        })?;
    }
    made.map(|m| git(repo, &["rev-parse", "--short", &m.sha]).map_err(Fail::refused))
        .transpose()
}

/// A job's `OBSERVE` lines, appended to its item, so what it saw below the bar for an item of its own
/// is kept.
fn append_observations(ctx: &mut Ctx, id: &str, name: &str, row: &Value) -> Result<()> {
    let seen: Vec<String> = row["observations"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if seen.is_empty() {
        return Ok(());
    }
    let req = EditRequest {
        common: ctx.common(false)?,
        id: id.to_string(),
        append: Some(crate::job::observed(name, &seen)),
        ..EditRequest::default()
    };
    let _: Value = ctx.api.post("edit", &req)?;
    Ok(())
}

/// What `docket collect` did, as it prints it.
struct Collected<'a> {
    id: &'a str,
    branch: &'a str,
    on: &'a str,
    name: &'a str,
    committed: Option<&'a str>,
    commits: &'a str,
    row: &'a Value,
    discard: bool,
}

impl Collected<'_> {
    fn print(&self, json: bool) {
        let (id, branch, on, name, row) = (self.id, self.branch, self.on, self.name, self.row);
        if json {
            let out = serde_json::json!({
                "id": id, "branch": branch, "machine": on, "job": name, "committed": self.committed,
                "commits": self.commits.lines().collect::<Vec<_>>(), "status": row, "discarded": self.discard,
            });
            println!("{out}");
            return;
        }
        println!("{id}: {branch} from {on}, job {name}");
        match self.committed {
            Some(sha) => println!("committed here as {sha}"),
            None if self.discard => println!("discarded: nothing committed"),
            None if row["base"].is_string() => println!("no change to commit"),
            None => {}
        }
        if !self.commits.is_empty() {
            println!("{}", self.commits);
        }
        println!(
            "{} {}",
            row["state"].as_str().unwrap_or("?"),
            row["report"].as_str().unwrap_or("no report")
        );
        if let Some(note) = row["note"].as_str() {
            println!("{note}");
        }
    }
}

/// Clear a job from its machine: its worktree, its record, and its branch there unless the branch
/// lives in this repository.
fn remove(via: &Via, slug: &str, name: &str, same: bool) -> Result<()> {
    let mut args = strings(&["-p", slug, "job", "remove", name]);
    if same {
        args.push("--keep-branch".into());
    }
    via.docket(&args).map(|_| ()).map_err(Fail::refused)
}

/// A job's change as `docket job diff --json` sends it, or as a plain patch from a machine whose
/// `docket` sends only that.
fn read_change(said: &str) -> Change {
    serde_json::from_str(said).unwrap_or_else(|_| Change {
        patch: said.to_string(),
        submodules: Vec::new(),
    })
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
fn model(ask: &Ask, facts: &Facts, role: &str, complexity: Option<&str>) -> Result<Model> {
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
    let Some(mut m) = fact::model_for(&facts.skills, &facts.owner, role_of, complexity) else {
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
    unread: &BTreeMap<String, String>,
    on: Option<&str>,
    runner: &str,
    here: &str,
) -> Result<&'a Machine> {
    let Some(name) = on else {
        return choose(machines, running, runner, here).ok_or_else(|| {
            Fail::refused(format!(
                "no machine has {runner} and a free slot: docket jobs shows what runs{}",
                unread_note(unread)
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
        if let Some(why) = unread.get(name) {
            return Err(Fail::refused(format!(
                "{name} cannot be read: {}{}",
                first_line(why),
                update_hint(why)
            )));
        }
        return Err(Fail::refused(format!(
            "{name} runs {} jobs, its slots: docket jobs",
            m.slots
        )));
    }
    Ok(m)
}

/// The jobs running on each machine, every project's, and the reason for each machine that could
/// not be read, which counts as full.
fn running(
    machines: &[Machine],
    here: &str,
) -> (BTreeMap<String, usize>, BTreeMap<String, String>) {
    let mut counts = BTreeMap::new();
    let mut unread = BTreeMap::new();
    for m in machines {
        match read(m, here, None) {
            Ok(rows) => {
                counts.insert(
                    m.name.clone(),
                    rows.iter().filter(|r| r["state"] == "running").count(),
                );
            }
            Err(why) => {
                counts.insert(m.name.clone(), usize::MAX);
                unread.insert(m.name.clone(), why);
            }
        }
    }
    (counts, unread)
}

fn first_line(why: &str) -> &str {
    why.lines().next().unwrap_or_default()
}

/// A remote whose docket has no `job` command answers with a usage error naming it.
fn update_hint(why: &str) -> &'static str {
    let lower = why.to_lowercase();
    if lower.contains("job") && (lower.contains("unrecognized") || lower.contains("unknown")) {
        " (its docket has no job command: update its client)"
    } else {
        ""
    }
}

/// One line per machine that could not be read, to follow a refusal.
fn unread_note(unread: &BTreeMap<String, String>) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for (name, why) in unread {
        let _ = write!(
            out,
            "\n{name} cannot be read: {}{}",
            first_line(why),
            update_hint(why)
        );
    }
    out
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
            Err(why) => eprintln!(
                "docket jobs: {} cannot be read: {}{}",
                m.name,
                first_line(&why),
                update_hint(&why)
            ),
        }
    }
    out
}

/// Refuse an item whose group has another member with a job running, naming that job.
fn refuse_a_running_group(
    ctx: &mut Ctx,
    item: &Value,
    machines: &[Machine],
    here: &str,
    force: bool,
) -> Result<()> {
    let Some(group) = item["group"].as_str().filter(|g| !g.is_empty() && !force) else {
        return Ok(());
    };
    let slug = ctx.project()?;
    let id = item["id"].as_str().unwrap_or_default();
    let listed: Value = ctx.api.get(
        "/groups",
        &[("project", slug.clone()), ("name", group.to_string())],
    )?;
    let others: BTreeSet<&str> = listed
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["id"].as_str())
        .filter(|m| *m != id)
        .collect();
    let rows = read_all(machines, here, Some(&slug));
    if let Some(job) = running_member(&rows, &others) {
        return Err(Fail::refused(format!(
            "{id} is in group {group}, whose member {} runs as {} on {}: give the group to that job, or --force",
            job["id"].as_str().unwrap_or("?"),
            job["name"].as_str().unwrap_or("?"),
            job["machine"].as_str().unwrap_or("?")
        )));
    }
    Ok(())
}

/// The running job of one of `members`, if any.
fn running_member<'a>(rows: &'a [Value], members: &BTreeSet<&str>) -> Option<&'a Value> {
    rows.iter()
        .find(|r| r["state"] == "running" && r["id"].as_str().is_some_and(|i| members.contains(i)))
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
