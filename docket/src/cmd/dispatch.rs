//! `docket dispatch`, `jobs` and `collect`: a lead's claims, the code moved by git over ssh from the
//! lead's machine, and every machine's jobs read where they run.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use docket_core::api::Facts;
use docket_core::api::{
    Common, EditRequest, JobReportRequest, ReleaseRequest, StartRequest, Started,
};
use docket_core::clock;
use docket_core::fact::{self, Model, Role};
use docket_core::machine::{Limit, Machine};

use crate::ctx::Ctx;
use crate::dispatch::{
    Here, Via, branch_for, commit_change, ended_in, git, gone, limited_runners, nonce,
    push_to_machine, ranked, read_machines_now, role_for, runner_counts,
};
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
    refuse_outside_run(ctx, &slug)?;
    let item: Value = ctx
        .api
        .get(&format!("/show/{id}"), &[("project", slug.clone())])?;
    if let Some(branch) = item["claim_branch"].as_str() {
        return Err(Fail::refused(format!(
            "{id} is held by {branch} on {}: give it back or take another",
            item["claim_host"].as_str().unwrap_or("?")
        )));
    }
    let ready = ready_row(ctx, &id)?;
    let role = role_for(ready.as_ref(), ask.role, &id).map_err(Fail::refused)?;
    let named = repos_named(&item);
    let (model, m, pushes) = placed(ctx, ask, &item, &role, ready.as_ref(), &named)?;
    let via = Via::of(&m, &here);
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
            effort: model.effort.clone(),
        },
    )?;
    let started = start(
        &via,
        &pushes,
        &Job {
            slug: &slug,
            id: &id,
            branch: &branch,
            role: &role,
            model: &model,
            repos: &named,
        },
    );
    if let Err(why) = started {
        for p in &pushes {
            let _ = push_to_machine(&p.repo, &p.url, &format!(":refs/heads/{branch}"));
        }
        give_back(
            ctx,
            common,
            &id,
            format!("dispatch to {} failed: {why}", m.name),
        )?;
        return Err(Fail::refused(format!(
            "dispatch of {id} to {} failed, the claim given back: {why}",
            m.name
        )));
    }
    let base = pushes.first().map(|p| p.base.clone()).unwrap_or_default();
    let from = from_line(&named, &pushes);
    if ctx.json {
        let out = serde_json::json!({
            "id": id, "machine": m.name, "branch": branch, "job": name, "base": base,
            "runner": model.runner, "model": model.model, "effort": model.effort, "role": role,
        });
        println!("{out}");
    } else {
        println!(
            "dispatched {id} to {}: {role} on {branch} from {from}, {} {}{}, job {name}",
            m.name,
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

/// The model a job runs on, the machine it is placed on, and the base of each repository it names,
/// taken from the lead's repository of it, to push to that machine's checkout.
fn placed(
    ctx: &mut Ctx,
    ask: &Ask,
    item: &Value,
    role: &str,
    ready: Option<&Value>,
    named: &[Option<String>],
) -> Result<(Model, Machine, Vec<Push>)> {
    let slug = ctx.project()?;
    let here = ctx.host()?;
    let id = item["id"].as_str().unwrap_or_default();
    let mut bases = Vec::new();
    for r in named {
        let repo = lead_repo(ctx, &slug, r.as_deref())?;
        let base = git(&repo, &["rev-parse", "HEAD"]).map_err(Fail::refused)?;
        bases.push((repo, base));
    }
    let (model, machines) = place(ctx, ask, item, role, &runner_counts(ready), &here)?;
    let (m, checkouts) = resolving(&machines, &here, &slug, id, named)?;
    let via = Via::of(&m, &here);
    let pushes = bases
        .into_iter()
        .zip(&checkouts)
        .map(|((repo, base), checkout)| Push {
            url: via.git_url(checkout),
            repo,
            base,
        })
        .collect();
    Ok((model, m, pushes))
}

/// The base a dispatch names: the first ten characters of its sha, or `REPO@SHA` for each
/// repository of an item naming several.
fn from_line(named: &[Option<String>], pushes: &[Push]) -> String {
    let short = |sha: &str| sha[..sha.len().min(10)].to_string();
    if pushes.len() == 1 {
        return short(&pushes[0].base);
    }
    let each: Vec<String> = named
        .iter()
        .zip(pushes)
        .map(|(r, p)| format!("{}@{}", r.as_deref().unwrap_or("."), short(&p.base)))
        .collect();
    each.join(" ")
}

/// The repositories an item changes, as the server reads them; one `None`, the `checkout` fact's,
/// for an item naming none.
fn repos_named(item: &Value) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = item["repos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|r| Some(r.to_string()))
        .collect();
    if out.is_empty() {
        out.push(item["repo"].as_str().map(str::to_string));
    }
    out
}

/// The first of `machines` on which every repository of `named` resolves, with its checkout of
/// each, so a job is placed only where it can run.
fn resolving(
    machines: &[Machine],
    here: &str,
    slug: &str,
    id: &str,
    named: &[Option<String>],
) -> Result<(Machine, Vec<String>)> {
    let mut why = Vec::new();
    for m in machines {
        let via = Via::of(m, here);
        let found: std::result::Result<Vec<String>, String> = named
            .iter()
            .map(|r| via.docket(&where_args(slug, r.as_deref())))
            .collect();
        match found {
            Ok(checkouts) => return Ok((m.clone(), checkouts)),
            Err(e) => why.push(format!("{}: {}", m.name, first_line(&e))),
        }
    }
    Err(Fail::refused(format!(
        "no machine that can take {id} has every repository it names: {}",
        why.join("; ")
    )))
}

/// Refuses a dispatch while the project's mode is drain or pause.
fn refuse_outside_run(ctx: &mut Ctx, slug: &str) -> Result<()> {
    let facts = ctx.api.facts(slug)?;
    match fact::dispatch_refused(&facts.skills, &facts.owner) {
        Some(why) => Err(Fail::refused(why)),
        None => Ok(()),
    }
}

/// The model the job runs on and the machine that takes it: the models fact's entry on a runner not
/// under a usage limit, and a machine with that runner and a free slot.
fn place(
    ctx: &mut Ctx,
    ask: &Ask,
    item: &Value,
    role: &str,
    runs: &BTreeMap<String, usize>,
    here: &str,
) -> Result<(Model, Vec<Machine>)> {
    let slug = ctx.project()?;
    let facts = ctx.api.facts(&slug)?;
    let machines = machines(ctx)?;
    let now = clock::now();
    let model = model(
        ask,
        &facts,
        role,
        item["complexity"].as_str(),
        runs,
        &limited_runners(&machines, &now),
    )?;
    refuse_a_running_group(ctx, item, ask.force)?;
    let running = running(&live_claims(ctx, true)?);
    let picked = pick(
        &machines,
        &Pick {
            running: &running,
            on: ask.on,
            runner: &model.runner,
            here,
            now: &now,
        },
    )?;
    Ok((model, picked.into_iter().cloned().collect()))
}

/// The claim of a job that never started given back, its attempt ended as failed.
fn give_back(ctx: &mut Ctx, common: Common, id: &str, note: String) -> Result<()> {
    let _: Value = ctx.api.post(
        "release",
        &ReleaseRequest {
            common,
            id: id.to_string(),
            note: Some(note),
            outcome: Some("failed".to_string()),
            ..ReleaseRequest::default()
        },
    )?;
    Ok(())
}

/// How long a wait goes without a report before it reads the machines: `DOCKET_JOBS_FALLBACK`
/// seconds, else five minutes.
fn fallback() -> Duration {
    let secs = std::env::var("DOCKET_JOBS_FALLBACK")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    Duration::from_secs(secs)
}

/// The project's jobs on every machine, or every project's; with `wait`, until one of the jobs
/// holding a claim at the start reports its end to the server, or `timeout` seconds pass when it is
/// not 0. The server's change stream says when to look, `every` seconds being the longest between
/// looks. A machine is read over ssh only when no report has come for a while.
///
/// # Errors
/// The server cannot be reached for the machines.
pub fn jobs(ctx: &mut Ctx, wait: bool, every: u64, timeout: u64, all: bool) -> Result<i32> {
    let slug = if all { None } else { Some(ctx.project()?) };
    let here = ctx.host()?;
    let machines = machines(ctx)?;
    if wait {
        let held = live_claims(ctx, all)?;
        if !held.is_empty() {
            wait_for_an_end(
                ctx,
                &held,
                &machines,
                &here,
                slug.as_deref(),
                every,
                timeout,
            );
        }
    }
    let rows = read_all(&machines, &here, slug.as_deref());
    record_limits(ctx, &rows);
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

/// Block until one of `held` reports its end, the timeout passes, or a machine read after the
/// fallback shows one of them no longer running.
fn wait_for_an_end(
    ctx: &mut Ctx,
    held: &[Claim],
    machines: &[Machine],
    here: &str,
    slug: Option<&str>,
    every: u64,
    timeout: u64,
) {
    let (tick, news) = std::sync::mpsc::channel::<()>();
    if let Ok(stream) = ctx.api.changes() {
        std::thread::spawn(move || {
            for _ in stream {
                if tick.send(()).is_err() {
                    break;
                }
            }
        });
    }
    let waited: Vec<(String, String)> = held
        .iter()
        .map(|c| (c.machine.clone(), c.job.clone()))
        .collect();
    let every = Duration::from_secs(every.max(1));
    let until = (timeout > 0).then(|| Instant::now() + Duration::from_secs(timeout));
    let mut read_at = Instant::now();
    loop {
        if until.is_some_and(|u| Instant::now() >= u) {
            return;
        }
        match news.recv_timeout(every) {
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(every),
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        if held
            .iter()
            .any(|c| reported_end(ctx, &c.project, &c.id, &c.branch).is_some())
        {
            return;
        }
        if read_machines_now(read_at.elapsed(), fallback()) {
            read_at = Instant::now();
            if gone(&waited, &read_all(machines, here, slug)) {
                return;
            }
        }
    }
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
    let several: Vec<String> = row["repos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let ran = Ran {
        via: &via,
        slug: &slug,
        name,
        on,
        branch,
    };
    let (committed, commits) = if several.is_empty() {
        land_one(ctx, &ran, &item, &row, discard)?
    } else {
        land_several(ctx, &ran, &several, &row, discard)?
    };
    append_observations(ctx, &id, name, &row)?;
    report_usage(ctx, &slug, &id, branch, &row);
    record_limits(ctx, &[with_machine(&row, on)]);
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

/// A job in one repository landed: its change committed here on the job's branch, the job
/// cleared. Returns the short sha, `None` for an empty change, and the commits the branch holds
/// past this repository's HEAD.
fn land_one(
    ctx: &mut Ctx,
    ran: &Ran,
    item: &Value,
    row: &Value,
    discard: bool,
) -> Result<(Option<String>, String)> {
    let (via, slug, name, branch) = (ran.via, ran.slug, ran.name, ran.branch);
    // The repository the job recorded it ran for; a machine whose docket records none sends no
    // field, and the item's says.
    let named = match row.get("repo") {
        Some(r) => r.as_str(),
        None => item["repo"].as_str(),
    };
    let checkout = via
        .docket(&where_args(slug, named))
        .map_err(Fail::refused)?;
    let repo = lead_repo(ctx, slug, named)?;
    let same = *via == Via::Here && same_repo(&repo, Path::new(&checkout));
    let committed = if discard {
        remove(via, slug, name, same)?;
        None
    } else if let Some(base) = row["base"].as_str() {
        take(ctx, ran, &repo, same, row, base)?
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
        remove(via, slug, name, same)?;
        None
    };
    Ok((committed, commits_past_head(&repo, branch)?))
}

/// The commits `branch` holds past the repository's HEAD, one line each; none when it is absent.
fn commits_past_head(repo: &Path, branch: &str) -> Result<String> {
    let reference = format!("refs/heads/{branch}");
    if git(repo, &["rev-parse", "--verify", "--quiet", &reference]).is_err() {
        return Ok(String::new());
    }
    git(repo, &["log", "--oneline", &format!("HEAD..{branch}")]).map_err(Fail::refused)
}

/// One repository of a job whose item names several, as a collect reaches it here.
struct Side {
    repo: String,
    /// The lead's repository of it, where the commit is made.
    local: PathBuf,
    /// Whether the job's worktree of it belongs to that repository.
    same: bool,
}

/// One repository's change committed here, on no branch yet: the commit made, `None` for an empty
/// change, which leaves the branch at `base`.
struct Landing<'a> {
    side: &'a Side,
    base: String,
    made: Option<crate::dispatch::Made>,
}

impl Landing<'_> {
    /// The job's branch moved to the commit made, or to the base for an empty change.
    fn keep(&self, branch: &str) -> std::result::Result<(), String> {
        match &self.made {
            Some(m) => m.keep(&self.side.local, branch),
            None => git(&self.side.local, &["branch", "-f", branch, &self.base]).map(|_| ()),
        }
    }
}

/// A job in several repositories landed in all of them or none: each repository's change is
/// committed here first, no branch moving, then every branch is moved, then the job cleared. A
/// change that cannot be committed, or a branch that cannot be moved, leaves every branch where it
/// was and the job where it ran. Returns `REPO@SHA` for each repository committed in.
fn land_several(
    ctx: &mut Ctx,
    ran: &Ran,
    repos: &[String],
    row: &Value,
    discard: bool,
) -> Result<(Option<String>, String)> {
    let (via, slug, name, branch) = (ran.via, ran.slug, ran.name, ran.branch);
    let sides = sides_of(ctx, ran, repos)?;
    let same = sides.iter().any(|s| s.same);
    if discard {
        remove(via, slug, name, same)?;
        return Ok((None, String::new()));
    }
    let landings = commit_all(ctx, ran, &sides, row)?;
    move_all(ran, &landings)?;
    remove(via, slug, name, same)?;
    // A branch checked out in the job's own worktree moves once that worktree is gone.
    for l in landings.iter().filter(|l| l.side.same) {
        l.keep(branch).map_err(|why| {
            let sha = l.made.as_ref().map_or(l.base.as_str(), |m| m.sha.as_str());
            Fail::refused(format!(
                "{why}: the change in {} is {sha}, git branch -f {branch} {sha} there",
                l.side.repo
            ))
        })?;
    }
    let mut landed = Vec::new();
    let mut commits = Vec::new();
    for l in &landings {
        if let Some(m) = &l.made {
            let short =
                git(&l.side.local, &["rev-parse", "--short", &m.sha]).map_err(Fail::refused)?;
            landed.push(format!("{}@{short}", l.side.repo));
        }
        for line in commits_past_head(&l.side.local, branch)?.lines() {
            commits.push(format!("{}: {line}", l.side.repo));
        }
    }
    Ok((
        (!landed.is_empty()).then(|| landed.join(" ")),
        commits.join("\n"),
    ))
}

/// Each repository of the job as this collect reaches it: the lead's repository of it, and whether
/// the job's worktree belongs to that repository.
fn sides_of(ctx: &mut Ctx, ran: &Ran, repos: &[String]) -> Result<Vec<Side>> {
    let mut sides = Vec::new();
    for r in repos {
        let checkout = ran
            .via
            .docket(&where_args(ran.slug, Some(r)))
            .map_err(Fail::refused)?;
        let local = lead_repo(ctx, ran.slug, Some(r))?;
        let same = *ran.via == Via::Here && same_repo(&local, Path::new(&checkout));
        sides.push(Side {
            repo: r.clone(),
            local,
            same,
        });
    }
    Ok(sides)
}

/// Each repository's change committed here on the commit the job started from there, no branch
/// moving, after the message and the private names are checked across all of them.
fn commit_all<'a>(
    ctx: &mut Ctx,
    ran: &Ran,
    sides: &'a [Side],
    row: &Value,
) -> Result<Vec<Landing<'a>>> {
    let (name, on) = (ran.name, ran.on);
    let said = ran
        .via
        .docket(&strings(&["-p", ran.slug, "job", "diff", name, "--json"]))
        .map_err(Fail::refused)?;
    let Ok(job::Diff::Several(changes)) = serde_json::from_str::<job::Diff>(&said) else {
        return Err(Fail::refused(format!(
            "{name} on {on} did not send a change for each of its repositories: update docket there"
        )));
    };
    let message = proposed(ran, row, changes.iter().map(|c| &c.change))?;
    let look = crate::cmd::private::read(ctx)?;
    for c in changes.iter().filter(|c| !c.change.is_empty()) {
        let found = look.change_hits(&c.change, message);
        if !found.is_empty() {
            return Err(Fail::refused(format!(
                "{name}'s change in {} carries private names and is kept on {on}: {}",
                c.repo,
                found.join("; ")
            )));
        }
    }
    let mut out = Vec::new();
    for side in sides {
        let Some(c) = changes.iter().find(|c| c.repo == side.repo) else {
            return Err(Fail::refused(format!(
                "{name} on {on} sent no change for {}: the job is kept",
                side.repo
            )));
        };
        let made = (!c.change.is_empty())
            .then(|| commit_change(&side.local, &c.base, &c.change, message))
            .transpose()
            .map_err(|why| {
                Fail::refused(format!(
                    "{name}'s change in {} was not committed here, so none of its repositories \
                     moved, and the job is kept on {on}: {why}",
                    side.repo
                ))
            })?;
        out.push(Landing {
            side,
            base: c.base.clone(),
            made,
        });
    }
    Ok(out)
}

/// The job's branch moved in each repository whose worktree is not the job's own; when one cannot
/// move, those moved go back to where they were.
fn move_all(ran: &Ran, landings: &[Landing]) -> Result<()> {
    let branch = ran.branch;
    let reference = format!("refs/heads/{branch}");
    let mut moved: Vec<(&Landing, Option<String>)> = Vec::new();
    for l in landings.iter().filter(|l| !l.side.same) {
        let was = git(
            &l.side.local,
            &["rev-parse", "--verify", "--quiet", &reference],
        )
        .ok();
        if let Err(why) = l.keep(branch) {
            for (back, was) in moved {
                let _ = match was.as_deref() {
                    Some(sha) => git(&back.side.local, &["branch", "-f", branch, sha]),
                    None => git(&back.side.local, &["branch", "-D", branch]),
                };
            }
            return Err(Fail::refused(format!(
                "{why}: no branch moved, the job is kept on {}, collect it again",
                ran.on
            )));
        }
        moved.push((l, was));
    }
    Ok(())
}

/// The message a job proposed for its change, which a change needs to be committed.
fn proposed<'a, 'b>(
    ran: &Ran,
    row: &'a Value,
    mut changes: impl Iterator<Item = &'b Change>,
) -> Result<&'a str> {
    let message = row["message"]
        .as_str()
        .or_else(|| row["note"].as_str())
        .filter(|m| !m.is_empty());
    if message.is_none() && changes.any(|c| !c.is_empty()) {
        return Err(Fail::refused(format!(
            "{} left a change but proposed no MESSAGE line to commit it with: docket job log {} on {}",
            ran.name, ran.name, ran.on
        )));
    }
    Ok(message.unwrap_or_default())
}

/// What a job's end posts to its claim: its times, exit, final report word, tokens and reported
/// cost, from the job's row.
#[must_use]
pub fn report_request(common: Common, id: &str, row: &Value) -> JobReportRequest {
    let stamp = |k: &str| row[k].as_u64().map(clock::stamp);
    JobReportRequest {
        common,
        id: id.to_string(),
        start: stamp("started"),
        end: stamp("ended"),
        exit: row["exit"].as_i64().and_then(|e| i32::try_from(e).ok()),
        tokens_in: row["tokens_in"].as_i64(),
        tokens_out: row["tokens"].as_i64(),
        cost_reported: row["cost"].as_f64(),
        report: row["report"].as_str().map(str::to_string),
    }
}

/// The job's end posted to the item's open claim when the job's own report did not get there: one
/// writer for the fact, the job, and this only where it is missing. A server that cannot take it is
/// said on standard error: the change is collected all the same.
fn report_usage(ctx: &mut Ctx, slug: &str, id: &str, branch: &str, row: &Value) {
    if reported_end(ctx, slug, id, branch).is_some() {
        return;
    }
    let sent = ctx.common(false).and_then(|common| {
        let _: Value = ctx
            .api
            .post("job-report", &report_request(common, id, row))?;
        Ok(())
    });
    if let Err(why) = sent {
        eprintln!("docket: the usage of {id}'s job was not recorded: {why}");
    }
}

/// The end a job reported to the server for the claim on `branch`: the data of its newest
/// `job_reported` event that carries one. None when the server holds none, or cannot be read.
fn reported_end(ctx: &mut Ctx, slug: &str, id: &str, branch: &str) -> Option<Value> {
    let log: Value = ctx
        .api
        .get(&format!("/log/{id}"), &[("project", slug.to_string())])
        .ok()?;
    ended_in(&log, branch)
}

/// A job's row with the machine it ran on added, as `jobs` reads it.
fn with_machine(row: &Value, on: &str) -> Value {
    let mut row = row.clone();
    row["machine"] = Value::String(on.to_string());
    row
}

/// The usage limits the jobs in `rows` ended on and that are still in force, reported to the server
/// so every machine and lead reads them. A server that cannot take one is said on standard error:
/// the limit only lasts until the next job reports it.
fn record_limits(ctx: &Ctx, rows: &[Value]) {
    let now = clock::now();
    let found: BTreeSet<(&str, &str, &str)> = rows
        .iter()
        .filter_map(|r| {
            let until = r["limit"].as_str().filter(|u| *u > now.as_str())?;
            Some((r["machine"].as_str()?, r["runner"].as_str()?, until))
        })
        .collect();
    for (machine, runner, until) in found {
        let limit = Limit {
            machine: machine.to_string(),
            runner: runner.to_string(),
            until: until.to_string(),
        };
        let sent: Result<docket_core::api::Machines> = ctx.api.post("limit", &limit);
        if let Err(why) = sent {
            eprintln!(
                "docket: {runner} on {machine} reported a limit until {until}, not recorded: {why}"
            );
        }
    }
}

/// A job that ended, as a collect reaches it.
struct Ran<'a> {
    via: &'a Via,
    slug: &'a str,
    name: &'a str,
    on: &'a str,
    branch: &'a str,
}

/// The job's change committed here in `repo` on `base`, then the job cleared, so a change that
/// cannot be committed leaves the job where it ran. `same` says the job's worktree belongs to
/// `repo`. Returns the short sha, `None` for an empty change.
fn take(
    ctx: &mut Ctx,
    ran: &Ran,
    repo: &Path,
    same: bool,
    row: &Value,
    base: &str,
) -> Result<Option<String>> {
    let (name, on, branch) = (ran.name, ran.on, ran.branch);
    let said = ran
        .via
        .docket(&strings(&["-p", ran.slug, "job", "diff", name, "--json"]))
        .map_err(Fail::refused)?;
    let change = read_change(&said);
    let message = proposed(ran, row, std::iter::once(&change))?;
    if !change.is_empty() {
        let look = crate::cmd::private::read(ctx)?;
        let found = look.change_hits(&change, message);
        if !found.is_empty() {
            return Err(Fail::refused(format!(
                "{name}'s change carries private names and is kept on {on}: {}",
                found.join("; ")
            )));
        }
    }
    let made = (!change.is_empty())
        .then(|| commit_change(repo, base, &change, message))
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
    if !same {
        keep().map_err(|why| {
            Fail::refused(format!("{why}: the job is kept on {on}, collect it again"))
        })?;
    }
    remove(ran.via, ran.slug, name, same)?;
    if same {
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
        if let Some(until) = row["limit"].as_str() {
            println!(
                "usage limit: {} on {on} until {until}, not counted as a failed job",
                row["runner"].as_str().unwrap_or("?")
            );
        }
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
    /// The repositories the item names; a `None` is the `checkout` fact's.
    repos: &'a [Option<String>],
}

/// One repository's base, pushed from the lead's repository to the machine's checkout at `url`.
struct Push {
    repo: PathBuf,
    base: String,
    url: String,
}

/// Each repository's base pushed to the machine's checkout as the job's branch, then the job
/// started there.
fn start(via: &Via, pushes: &[Push], job: &Job) -> std::result::Result<String, String> {
    for p in pushes {
        push_to_machine(
            &p.repo,
            &p.url,
            &format!("{}:refs/heads/{}", p.base, job.branch),
        )?;
    }
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
    for r in job.repos.iter().flatten() {
        args.extend(strings(&["--repo", r]));
    }
    via.docket(&args)
}

/// `docket job where` for the repository an item names, or the `checkout` fact's when it names
/// none, which a machine whose docket predates `--repo` still answers.
fn where_args(slug: &str, repo: Option<&str>) -> Vec<String> {
    let mut args = strings(&["-p", slug, "job", "where"]);
    if let Some(r) = repo {
        args.extend(strings(&["--repo", r]));
    }
    args
}

/// The model the job runs on: the one asked for, else the `models` fact's for its role and
/// complexity.
fn model(
    ask: &Ask,
    facts: &Facts,
    role: &str,
    complexity: Option<&str>,
    runs: &BTreeMap<String, usize>,
    limited: &BTreeMap<String, String>,
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
    let unavailable: Vec<&str> = limited.keys().map(String::as_str).collect();
    let chosen = if role_of == Role::Audit {
        fact::audit_model(&facts.skills, &facts.owner, runs, &unavailable)
    } else {
        fact::model_without(
            &facts.skills,
            &facts.owner,
            role_of,
            complexity,
            &unavailable,
        )
    };
    let Some(mut m) = chosen else {
        if fact::model_for(&facts.skills, &facts.owner, role_of, complexity).is_some() {
            let resets: Vec<String> = limited
                .iter()
                .map(|(r, u)| format!("{r} until {u}"))
                .collect();
            return Err(Fail::refused(format!(
                "every runner the models fact names for this job reported a usage limit: {}",
                resets.join(", ")
            )));
        }
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

/// What `pick` chooses by.
struct Pick<'a> {
    running: &'a BTreeMap<String, usize>,
    on: Option<&'a str>,
    runner: &'a str,
    here: &'a str,
    now: &'a str,
}

/// The machine asked for, when it has the runner free of a usage limit and a free slot, or those
/// `ranked` orders.
fn pick<'a>(machines: &'a [Machine], by: &Pick) -> Result<Vec<&'a Machine>> {
    let Pick {
        running,
        on,
        runner,
        here,
        now,
    } = *by;
    let Some(name) = on else {
        let all = ranked(machines, running, runner, here, now);
        if all.is_empty() {
            return Err(Fail::refused(format!(
                "no machine has {runner} and a free slot: docket jobs shows what runs{}",
                limit_note(machines, runner, now)
            )));
        }
        return Ok(all);
    };
    let Some(m) = machines.iter().find(|m| m.name == name) else {
        return Err(Fail::refused(format!("no machine {name}: docket machines")));
    };
    if !m.runners.iter().any(|r| r == runner) {
        return Err(Fail::refused(format!("{name} has no {runner}")));
    }
    if let Some(until) = m.limited(runner, now) {
        return Err(Fail::refused(format!(
            "{runner} on {name} reported a usage limit until {until}: docket machines"
        )));
    }
    if crate::dispatch::free(m, running) == 0 {
        return Err(Fail::refused(format!(
            "{name} runs {} jobs, its slots: docket jobs",
            m.slots
        )));
    }
    Ok(vec![m])
}

/// One line per machine whose `runner` is under a usage limit at `now`, to follow a refusal.
fn limit_note(machines: &[Machine], runner: &str, now: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for m in machines {
        if let Some(until) = m.limited(runner, now) {
            let _ = write!(
                out,
                "\n{runner} on {} reported a usage limit until {until}",
                m.name
            );
        }
    }
    out
}

/// A claim held by a job a lead started on a machine.
struct Claim {
    project: String,
    id: String,
    machine: String,
    job: String,
    branch: String,
}

/// The claims of jobs on the server, this project's or every project's. The server holds each one
/// with its job and machine, so no machine is asked.
fn claims(ctx: &mut Ctx, all: bool) -> Result<Vec<Claim>> {
    let slugs = if all {
        ctx.api.slugs()?
    } else {
        vec![ctx.project()?]
    };
    let mut out = Vec::new();
    for slug in slugs {
        let rows: Value = ctx.api.get("/wip", &[("project", slug.clone())])?;
        for r in rows.as_array().into_iter().flatten() {
            if let (Some(id), Some(machine), Some(job), Some(branch)) = (
                r["id"].as_str(),
                r["claim_on"].as_str(),
                r["claim_job"].as_str(),
                r["claim_branch"].as_str(),
            ) {
                out.push(Claim {
                    project: slug.clone(),
                    id: id.to_string(),
                    machine: machine.to_string(),
                    job: job.to_string(),
                    branch: branch.to_string(),
                });
            }
        }
    }
    Ok(out)
}

/// The claims whose job has not reported its end.
fn live_claims(ctx: &mut Ctx, all: bool) -> Result<Vec<Claim>> {
    let all = claims(ctx, all)?;
    Ok(all
        .into_iter()
        .filter(|c| reported_end(ctx, &c.project, &c.id, &c.branch).is_none())
        .collect())
}

/// The jobs running on each machine, counted from their claims.
fn running(claims: &[Claim]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for c in claims {
        *counts.entry(c.machine.clone()).or_insert(0) += 1;
    }
    counts
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
fn refuse_a_running_group(ctx: &mut Ctx, item: &Value, force: bool) -> Result<()> {
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
    let held = live_claims(ctx, false)?;
    if let Some(job) = held.iter().find(|c| others.contains(c.id.as_str())) {
        return Err(Fail::refused(format!(
            "{id} is in group {group}, whose member {} runs as {} on {}: give the group to that job, or --force",
            job.id, job.job, job.machine
        )));
    }
    Ok(())
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

/// The item's row in the ready queue, which carries the role it is taken in; none when the queue
/// does not offer it.
fn ready_row(ctx: &mut Ctx, id: &str) -> Result<Option<Value>> {
    let slug = ctx.project()?;
    let rows: Value = ctx.api.get(
        "/next",
        &[("project", slug), ("id", id.to_string()), ("n", "1".into())],
    )?;
    Ok(rows.as_array().and_then(|a| a.first()).cloned())
}

/// The lead's repository for an item, the one it merges into: the repository the item names, else
/// the `checkout` fact's, under the project root the lead stands in, by [`Here::repo`].
fn lead_repo(ctx: &mut Ctx, slug: &str, named: Option<&str>) -> Result<PathBuf> {
    if let Some(rel) = named.filter(|r| docket_core::label::foreign(r).is_some()) {
        let dir = crate::local::resolve_repo(&ctx.roots.roots, "", rel).map_err(Fail::refused)?;
        return top_of(&dir, rel);
    }
    let facts = ctx.api.facts(slug)?;
    let default = facts
        .skills
        .get("checkout")
        .map(String::as_str)
        .filter(|c| !c.is_empty())
        .unwrap_or(".");
    let cwd = crate::local::realpath(&ctx.cwd);
    let (top, _) = crate::local::outermost_repo(&ctx.cwd);
    let top = top.map(|t| t.display().to_string());
    let own = git(&ctx.cwd, &["rev-parse", "--show-toplevel"])
        .ok()
        .map(PathBuf::from);
    let here = Here {
        roots: &ctx.roots.roots,
        slug,
        cwd: &cwd,
        top: top.as_deref(),
        own: own.as_deref(),
    };
    let Some(dir) = here.repo(named, default) else {
        return Err(Fail::refused(format!(
            "docket dispatch and collect run inside the lead's git checkout or under a root of {slug}"
        )));
    };
    top_of(&dir, named.unwrap_or(default))
}

/// `dir` when it is the top of a git repository, else a refusal naming `wanted`.
fn top_of(dir: &Path, wanted: &str) -> Result<PathBuf> {
    let real = |p: &Path| std::fs::canonicalize(p).ok();
    match git(dir, &["rev-parse", "--show-toplevel"]) {
        Ok(top) if real(Path::new(&top)).is_some() && real(Path::new(&top)) == real(dir) => {
            Ok(PathBuf::from(top))
        }
        _ => Err(Fail::refused(format!(
            "{} is no repository: clone {wanted} there, or name the repository the item changes with \
             docket edit ID --set repo=PATH",
            dir.display()
        ))),
    }
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
