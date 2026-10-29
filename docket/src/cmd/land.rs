//! `docket land`: finished builds queued to land, and one gate run per checkout that takes them in
//! batches until the queue is empty: each batch merged onto the work branch's head in a worktree,
//! gated with the `gate` fact's command, fast-forwarded and its items closed. It never pushes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use docket_core::api::{CloseRequest, Closed, Common, LandRequest, MovedMany};
use docket_core::assignment::Stage;
use docket_core::fact;
use docket_core::rows::Row;

use crate::cmd::write::refuse_in_job;
use crate::ctx::Ctx;
use crate::dispatch::git;
use crate::fail::{Fail, Result};
use crate::job;
use crate::row::rows_of;

/// The lines of a failed gate's log its items carry.
const TAIL: usize = 20;

/// What `docket land` was asked for.
pub struct Ask<'a> {
    pub ids: &'a [String],
    /// The build sent back out of a failed gate run, and why.
    pub back: Option<(&'a str, &'a str)>,
    pub log: bool,
    /// Run the gate loop itself on this work branch, as `docket land` starts it.
    pub run: Option<&'a str>,
}

/// Queue builds and see a gate run going, send one back, print the log, or be the gate run.
///
/// # Errors
/// Inside a job, outside a checkout on a branch, with no `gate` fact, or the server refuses.
pub fn land(ctx: &mut Ctx, ask: &Ask) -> Result<i32> {
    refuse_in_job("land")?;
    let slug = ctx.project()?;
    let here = checkout(ctx, &slug, ask.run)?;
    if ask.log {
        return print_log(&here);
    }
    if let Some(work) = ask.run {
        return lander(ctx, &slug, &here, work);
    }
    gate_of(ctx, &slug)?;
    let (queued, refused) = match ask.back {
        Some((id, why)) => (send_back(ctx, &slug, id, why)?, Vec::new()),
        None => queue(ctx, &slug, &here, ask.ids)?,
    };
    let mut queued = queued;
    let free = lock(&here.dir)?.is_some();
    let rows = rows_of(&ctx.read("/wip", &[])?);
    for id in stranded(&rows, free, |b| has_branch(&here.repo, b)) {
        let branch = rows
            .iter()
            .find(|r| r.id == id)
            .and_then(|r| r.claim_branch.clone())
            .unwrap_or_default();
        step(ctx, &slug, &branch, "queue", id, |_| {})?;
        queued.push(id.to_string());
    }
    let rows = rows_of(&ctx.read("/wip", &[])?);
    let waiting = batch(&rows, usize::MAX, |b| has_branch(&here.repo, b));
    let started = if run_wanted(&queued, &waiting) {
        Some(keep_running(&slug, &here)?)
    } else {
        None
    };
    if ctx.json {
        let out = serde_json::json!({
            "queued": queued, "refused": refused, "started": started,
            "queue": waiting.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            "log": here.dir.join("gate.log"),
        });
        println!("{out}");
    } else {
        say(&queued, &refused, started, &waiting, &here);
    }
    Ok(i32::from(!refused.is_empty()))
}

/// What a `docket land` that queued did, in lines.
fn say(queued: &[String], refused: &[String], started: Option<bool>, waiting: &[Row], here: &Here) {
    if !queued.is_empty() {
        println!("queued {}", queued.join(", "));
    }
    for why in refused {
        eprintln!("{why}");
    }
    match started {
        Some(true) => println!("a gate run started in {}", here.repo.display()),
        Some(false) => println!("a gate run is going in {}", here.repo.display()),
        None => {}
    }
    if !waiting.is_empty() {
        let ids: Vec<&str> = waiting.iter().map(|r| r.id.as_str()).collect();
        println!("to gate: {}", ids.join(", "));
    }
    println!(
        "log: docket land --log, or {}",
        here.dir.join("gate.log").display()
    );
}

/// The checkout a gate run lands in: its repository and its gate run's directory.
struct Here {
    repo: PathBuf,
    dir: PathBuf,
}

/// The repository this runs in, on a branch; a gate run is given its work branch instead.
fn checkout(ctx: &Ctx, slug: &str, work: Option<&str>) -> Result<Here> {
    let repo = git(&ctx.cwd, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .map_err(|_| {
            Fail::refused(format!(
                "{} is not inside a checkout: docket land runs in the project checkout",
                ctx.cwd.display()
            ))
        })?;
    if work.is_none() && git(&repo, &["branch", "--show-current"]).is_ok_and(|b| b.is_empty()) {
        return Err(Fail::refused(format!(
            "{} is on no branch: docket land runs on the branch the work lands on",
            repo.display()
        )));
    }
    let dir = run_dir(&job::state_root(), slug, &repo);
    Ok(Here { repo, dir })
}

/// The `gate` fact, which a landing needs.
fn gate_of(ctx: &Ctx, slug: &str) -> Result<String> {
    let facts = ctx.api.facts(slug)?;
    fact::effective(&facts.skills, "gate").ok_or_else(|| {
        Fail::refused(
            "no gate fact: the command whose exit 0 lets a batch land, docket skills set gate \"...\"; build it from the gates fact (docket skills gates)",
        )
    })
}

/// Each item named, or every claim reading to land and not yet queued, queued: an ended job's
/// change collected first. Returns the ids queued and why the others were not.
fn queue(
    ctx: &mut Ctx,
    slug: &str,
    here: &Here,
    ids: &[String],
) -> Result<(Vec<String>, Vec<String>)> {
    let ids: Vec<String> = if ids.is_empty() {
        unqueued(&rows_of(&ctx.read("/wip", &[])?))
            .into_iter()
            .map(str::to_string)
            .collect()
    } else {
        ids.iter()
            .map(|i| crate::ctx::id(i))
            .collect::<Result<_>>()?
    };
    let mut queued = Vec::new();
    let mut refused = Vec::new();
    for id in ids {
        match queue_one(ctx, slug, here, &id) {
            Ok(()) => queued.push(id),
            Err(Fail::Refused(why)) => refused.push(why),
        }
    }
    Ok((queued, refused))
}

fn queue_one(ctx: &mut Ctx, slug: &str, here: &Here, id: &str) -> Result<()> {
    let item = ctx.read(&format!("/show/{id}"), &[])?;
    let Some(branch) = item["claim_branch"].as_str().map(str::to_string) else {
        return Err(Fail::refused(format!(
            "{id} holds no claim: nothing to land"
        )));
    };
    match crate::cmd::dispatch::job_waits(ctx, &item)? {
        Some(false) => {
            return Err(Fail::refused(format!(
                "{id}'s job is still running: docket jobs --wait"
            )));
        }
        Some(true) => {
            crate::cmd::dispatch::collect(ctx, id, false, None, None)?;
        }
        None => {}
    }
    if !has_branch(&here.repo, &branch) {
        return Err(Fail::refused(format!(
            "{id}'s branch {branch} is not in {}: docket land runs in the repository it changes",
            here.repo.display()
        )));
    }
    step(ctx, slug, &branch, "queue", id, |_| {})
}

/// One build of a failed gate run given back, and the rest of its batch queued again.
fn send_back(ctx: &mut Ctx, slug: &str, id: &str, why: &str) -> Result<Vec<String>> {
    let id = crate::ctx::id(id)?;
    let row: Row = serde_json::from_value(ctx.read(&format!("/show/{id}"), &[])?)
        .map_err(|e| Fail::refused(format!("/show/{id}: {e}")))?;
    let failed = row
        .claim_land
        .as_ref()
        .filter(|l| l.stage == Stage::GateFailed)
        .and_then(|l| l.batch.clone());
    let (Some(branch), Some(name)) = (row.claim_branch.as_deref(), failed) else {
        return Err(Fail::refused(format!(
            "{id} is not in a failed gate run: docket land --back sends one of its builds back"
        )));
    };
    step(ctx, slug, branch, "back", &id, |r| {
        r.reason = Some(why.to_string());
    })?;
    let rows = rows_of(&ctx.read("/wip", &[])?);
    let mut queued = Vec::new();
    for other in rest_of_batch(&rows, &id, &name) {
        let branch = rows
            .iter()
            .find(|r| r.id == other)
            .and_then(|r| r.claim_branch.clone())
            .unwrap_or_default();
        step(ctx, slug, &branch, "queue", other, |_| {})?;
        queued.push(other.to_string());
    }
    Ok(queued)
}

/// A landing step recorded on one item, under its claim's branch.
fn step(
    ctx: &Ctx,
    slug: &str,
    branch: &str,
    action: &str,
    id: &str,
    fill: impl FnOnce(&mut LandRequest),
) -> Result<()> {
    let mut req = LandRequest {
        common: Common {
            project: slug.to_string(),
            branch: Some(branch.to_string()),
            force: false,
        },
        action: action.to_string(),
        ids: vec![id.to_string()],
        ..LandRequest::default()
    };
    fill(&mut req);
    let _: MovedMany = ctx.api.post("land", &req)?;
    Ok(())
}

/// A gate run started in the background unless one already holds this checkout's lock; whether one
/// was started.
fn keep_running(slug: &str, here: &Here) -> Result<bool> {
    if lock(&here.dir)?.is_none() {
        return Ok(false);
    }
    let exe = std::env::current_exe()
        .map_err(|e| Fail::refused(format!("docket land: the running docket: {e}")))?;
    let work = git(&here.repo, &["branch", "--show-current"]).map_err(Fail::refused)?;
    for file in ["exit", "pid", "pgid", "stderr", "events.jsonl"] {
        let _ = fs::remove_file(here.dir.join(file));
    }
    let argv: Vec<String> = ["-p", slug, "land", "--run", &work]
        .iter()
        .map(|w| (*w).to_string())
        .collect();
    job::detach(&here.dir, &here.repo, &exe.display().to_string(), &argv)
        .map_err(|e| Fail::refused(format!("docket land: cannot start the gate run: {e}")))?;
    Ok(true)
}

/// This checkout's gate run lock, taken; `None` while another process holds it. Released when the
/// file is closed, so a gate run that dies leaves it free.
fn lock(dir: &Path) -> Result<Option<fs::File>> {
    use std::os::fd::AsRawFd;
    fs::create_dir_all(dir).map_err(|e| Fail::refused(format!("{}: {e}", dir.display())))?;
    let path = dir.join("lock");
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| Fail::refused(format!("{}: {e}", path.display())))?;
    // SAFETY: flock on a descriptor this function owns, which nothing else closes.
    let taken = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    Ok(taken.then_some(file))
}

/// The current or last gate run's log.
fn print_log(here: &Here) -> Result<i32> {
    match fs::read_to_string(here.dir.join("gate.log")) {
        Ok(text) => {
            print!("{text}");
            Ok(0)
        }
        Err(_) => Err(Fail::refused(format!(
            "no gate run has logged in {} yet",
            here.repo.display()
        ))),
    }
}

/// The gate run: batches taken oldest first until none is queued, holding the checkout's lock. The
/// lock is let go before the last look at the queue, so a build queued while it ends either is
/// seen here or finds the lock free and starts the next run.
fn lander(ctx: &mut Ctx, slug: &str, here: &Here, work: &str) -> Result<i32> {
    let Some(mut held) = lock(&here.dir)? else {
        eprintln!("another gate run holds {}", here.dir.display());
        return Ok(0);
    };
    loop {
        let gate = gate_of(ctx, slug)?;
        let n = fact::land_batch(&ctx.api.facts(slug)?.skills);
        let rows = rows_of(&ctx.read("/wip", &[])?);
        let picked = batch(&rows, n, |b| has_branch(&here.repo, b));
        if picked.is_empty() {
            drop(held);
            let rows = rows_of(&ctx.read("/wip", &[])?);
            if batch(&rows, 1, |b| has_branch(&here.repo, b)).is_empty() {
                return Ok(0);
            }
            let Some(again) = lock(&here.dir)? else {
                return Ok(0);
            };
            held = again;
            continue;
        }
        let bisect = fact::land_bisect(&ctx.api.facts(slug)?.skills);
        let run = Run {
            slug,
            here,
            work,
            gate: &gate,
            bisect,
        };
        if let Err(e) = run.batch(ctx, &picked) {
            run.requeue(ctx, &picked);
            return Err(e);
        }
    }
}

/// One batch's gate run in this checkout.
struct Run<'a> {
    slug: &'a str,
    here: &'a Here,
    work: &'a str,
    gate: &'a str,
    bisect: bool,
}

/// How a set of builds fared at the gate.
enum Gated {
    /// Every build conflicted, so nothing was gated.
    Empty,
    /// The gate passed on `base`, the work branch's head; `clean` when nothing conflicted or moved.
    Passed {
        merged: Vec<Merged>,
        base: String,
        clean: bool,
    },
    Failed {
        merged: Vec<Merged>,
        log: String,
    },
    /// The set could not be gated to a verdict, as the work branch kept moving.
    Stopped {
        merged: Vec<Merged>,
        why: String,
    },
}

fn rows_of_merged(merged: Vec<Merged>) -> Vec<Row> {
    merged.into_iter().map(|m| m.row).collect()
}

/// A build merged into the batch, and the merge commit that brought it in.
struct Merged {
    row: Row,
    sha: String,
}

impl Run<'_> {
    fn tree(&self) -> PathBuf {
        self.here.dir.join("tree")
    }

    fn head(&self) -> Result<String> {
        git(
            &self.here.repo,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/heads/{}", self.work),
            ],
        )
        .map_err(Fail::refused)
    }

    /// The batch gated and, when the gate passes, landed and closed. A failed gate splits the batch
    /// when `land_bisect` is on, and keeps its builds out as failed when it is off.
    fn batch(&self, ctx: &mut Ctx, picked: &[Row]) -> Result<()> {
        let name = format!("{}-{}", job::name_of(self.work), job::now());
        match self.gate_set(ctx, picked, &name)? {
            Gated::Empty => {}
            Gated::Passed { merged, base, .. } => self.land(ctx, &merged, &base)?,
            Gated::Stopped { merged, why } => self.fail(ctx, &merged, &why)?,
            Gated::Failed { merged, log } => {
                if self.bisect {
                    self.settle(ctx, rows_of_merged(merged), &log, &name)?;
                } else {
                    self.fail(ctx, &merged, &log)?;
                }
            }
        }
        self.clear();
        Ok(())
    }

    /// Builds that fail the gate on the work branch's head, split in halves and gated again until
    /// the build that fails is found. The first half is gated: a passing half lands and the rest,
    /// which fails on the new head by what was already gated, is split in turn; a failing half is
    /// settled itself before the rest is gated. A build that fails alone goes back to its item with
    /// the log's tail.
    fn settle(&self, ctx: &mut Ctx, mut rows: Vec<Row>, log: &str, name: &str) -> Result<()> {
        let mut log = log.to_string();
        let mut failing = true;
        let mut splits = 0;
        while !rows.is_empty() {
            if !failing {
                splits += 1;
                match self.gate_set(ctx, &rows, &format!("{name}.{splits}"))? {
                    Gated::Empty => break,
                    Gated::Passed { merged, base, .. } => {
                        self.land(ctx, &merged, &base)?;
                        break;
                    }
                    Gated::Stopped { merged, why } => {
                        self.fail(ctx, &merged, &why)?;
                        break;
                    }
                    Gated::Failed { merged, log: tail } => {
                        rows = rows_of_merged(merged);
                        log = tail;
                    }
                }
            }
            if rows.len() == 1 {
                let why = format!(
                    "the gate fails with this build merged onto {}:\n{log}",
                    self.work
                );
                self.step(ctx, &rows[0], "back", |r| r.reason = Some(why))?;
                eprintln!("{} fails the gate and goes back", rows[0].id);
                break;
            }
            let rest = rows.split_off(rows.len() / 2);
            let first = std::mem::replace(&mut rows, rest);
            splits += 1;
            let sub = format!("{name}.{splits}");
            match self.gate_set(ctx, &first, &sub)? {
                Gated::Empty => failing = false,
                Gated::Passed {
                    merged,
                    base,
                    clean,
                } => {
                    self.land(ctx, &merged, &base)?;
                    failing = clean && merged.len() == first.len();
                }
                Gated::Stopped { merged, why } => {
                    self.fail(ctx, &merged, &why)?;
                    failing = false;
                }
                Gated::Failed { merged, log: tail } => {
                    self.settle(ctx, rows_of_merged(merged), &tail, &sub)?;
                    failing = false;
                }
            }
        }
        self.clear();
        Ok(())
    }

    /// The builds merged in a fresh worktree on the work branch's head and gated under a batch
    /// name. A work branch that moved during the gate run is merged onto again, once.
    fn gate_set(&self, ctx: &mut Ctx, picked: &[Row], name: &str) -> Result<Gated> {
        let tree = self.tree();
        self.clear();
        let mut base = self.head()?;
        git(
            &self.here.repo,
            &[
                "worktree",
                "add",
                "--detach",
                &tree.display().to_string(),
                &base,
            ],
        )
        .map_err(Fail::refused)?;
        let mut merged = self.merge_all(ctx, picked)?;
        let mut moved = false;
        while !merged.is_empty() {
            let short =
                git(&self.here.repo, &["rev-parse", "--short", &base]).map_err(Fail::refused)?;
            let members: Vec<String> = merged.iter().map(|m| m.row.id.clone()).collect();
            eprintln!("gating {} on {short} as {name}", members.join(", "));
            let _ = fs::write(self.here.dir.join("members"), members.join(" "));
            for m in &merged {
                self.step(ctx, &m.row, "gating", |r| {
                    r.batch = Some(name.to_string());
                    r.base = Some(short.clone());
                    r.members.clone_from(&members);
                })?;
            }
            if let Err(log) = self.gate() {
                eprintln!("the gate failed for {}", members.join(", "));
                return Ok(Gated::Failed { merged, log });
            }
            let now = self.head()?;
            if now == base {
                let clean = !moved && merged.len() == picked.len();
                return Ok(Gated::Passed {
                    merged,
                    base,
                    clean,
                });
            }
            if moved {
                let why = format!("{} moved twice during the gate run", self.work);
                return Ok(Gated::Stopped { merged, why });
            }
            moved = true;
            base = now;
            eprintln!(
                "{} moved during the gate run: merging onto it again",
                self.work
            );
            git(&tree, &["checkout", "--quiet", "--detach", &base]).map_err(Fail::refused)?;
            let rows: Vec<Row> = merged.into_iter().map(|m| m.row).collect();
            merged = self.merge_all(ctx, &rows)?;
        }
        Ok(Gated::Empty)
    }

    /// A gated set fast-forwarded onto the work branch and closed.
    fn land(&self, ctx: &mut Ctx, merged: &[Merged], base: &str) -> Result<()> {
        let tip = merged.last().map(|m| m.sha.clone()).unwrap_or_default();
        if let Err(why) = self.fast_forward(base, &tip) {
            return self.fail(ctx, merged, &why);
        }
        self.close(ctx, merged)
    }

    /// Each build merged `--no-ff` under its commit's subject; one that conflicts is left out with
    /// its files named on its item.
    fn merge_all(&self, ctx: &mut Ctx, rows: &[Row]) -> Result<Vec<Merged>> {
        let tree = self.tree();
        let mut out = Vec::new();
        for row in rows {
            let branch = row.claim_branch.clone().unwrap_or_default();
            let reference = format!("refs/heads/{branch}");
            let subject =
                git(&tree, &["log", "-1", "--format=%s", &reference]).map_err(Fail::refused)?;
            let message = merge_message(&subject);
            match git(
                &tree,
                &["merge", "--no-ff", "--quiet", "-m", &message, &reference],
            ) {
                Ok(_) => {
                    let sha = git(&tree, &["rev-parse", "HEAD"]).map_err(Fail::refused)?;
                    out.push(Merged {
                        row: row.clone(),
                        sha,
                    });
                }
                Err(why) => {
                    let listed =
                        git(&tree, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default();
                    let mut files: Vec<String> = listed.lines().map(str::to_string).collect();
                    if files.is_empty() {
                        files.push(why.lines().next().unwrap_or_default().to_string());
                    }
                    let _ = git(&tree, &["merge", "--abort"]);
                    eprintln!("{} conflicts in {}", row.id, files.join(", "));
                    self.step(ctx, row, "conflict", |r| r.files = files)?;
                }
            }
        }
        Ok(out)
    }

    /// The `gate` command run in the batch's worktree, its output the log; the log's tail when it
    /// does not exit 0.
    fn gate(&self) -> std::result::Result<(), String> {
        let path = self.here.dir.join("gate.log");
        let log = fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let err = log.try_clone().map_err(|e| e.to_string())?;
        let status = Command::new("sh")
            .arg("-c")
            .arg(self.gate)
            .current_dir(self.tree())
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(err)
            .status()
            .map_err(|e| format!("the gate could not start: {e}"))?;
        if status.success() {
            return Ok(());
        }
        let text = fs::read_to_string(&path).unwrap_or_default();
        Err(format!("{status}\n{}", log_tail(&text, TAIL)))
    }

    /// The checkout's work branch moved to the batch's tip, which holds its head.
    fn fast_forward(&self, base: &str, tip: &str) -> std::result::Result<(), String> {
        let on = git(&self.here.repo, &["branch", "--show-current"]).unwrap_or_default();
        if on == self.work {
            git(&self.here.repo, &["merge", "--ff-only", "--quiet", tip]).map(|_| ())
        } else {
            let reference = format!("refs/heads/{}", self.work);
            git(&self.here.repo, &["update-ref", &reference, tip, base]).map(|_| ())
        }
        .map_err(|why| format!("{} could not be fast-forwarded: {why}", self.work))
    }

    /// Each build closed with the sha that brought it in, under its claim's branch, and the branch
    /// and the worktree removed.
    fn close(&self, ctx: &mut Ctx, merged: &[Merged]) -> Result<()> {
        for m in merged {
            let branch = m.row.claim_branch.clone().unwrap_or_default();
            let short =
                git(&self.here.repo, &["rev-parse", "--short", &m.sha]).map_err(Fail::refused)?;
            let req = CloseRequest {
                common: self.common(&branch),
                id: m.row.id.clone(),
                resolution: Some(short.clone()),
                gates: Some("passed".to_string()),
                runner: None,
                model: None,
            };
            let _: Closed = ctx.api.post("close", &req)?;
            eprintln!("landed {} as {short}", m.row.id);
            if let Err(why) = git(&self.here.repo, &["branch", "-D", &branch]) {
                eprintln!("{branch} is kept: {why}");
            }
        }
        self.clear();
        Ok(())
    }

    /// Each build of a batch that did not land recorded as failed with why, and the worktree removed.
    fn fail(&self, ctx: &mut Ctx, merged: &[Merged], why: &str) -> Result<()> {
        for m in merged {
            self.step(ctx, &m.row, "failed", |r| r.log = Some(why.to_string()))?;
        }
        self.clear();
        Ok(())
    }

    fn step(
        &self,
        ctx: &Ctx,
        row: &Row,
        action: &str,
        fill: impl FnOnce(&mut LandRequest),
    ) -> Result<()> {
        let branch = row.claim_branch.clone().unwrap_or_default();
        step(ctx, self.slug, &branch, action, &row.id, fill)
    }

    fn common(&self, branch: &str) -> Common {
        Common {
            project: self.slug.to_string(),
            branch: Some(branch.to_string()),
            force: false,
        }
    }

    /// The builds of a batch that a failed run left gating queued again, so the server sees no gate
    /// run going and the next run takes them. A post that fails too leaves them to `docket land`.
    fn requeue(&self, ctx: &mut Ctx, picked: &[Row]) {
        self.clear();
        let Ok(read) = ctx.read("/wip", &[]) else {
            return;
        };
        let ids: Vec<String> = picked.iter().map(|r| r.id.clone()).collect();
        let rows = rows_of(&read);
        for id in still_gating(&rows, &ids) {
            if let Some(row) = rows.iter().find(|r| r.id == id) {
                let _ = self.step(ctx, row, "queue", |_| {});
            }
        }
    }

    /// The batch's worktree removed, whatever it holds.
    fn clear(&self) {
        let tree = self.tree();
        if tree.exists() {
            let _ = git(
                &self.here.repo,
                &["worktree", "remove", "--force", &tree.display().to_string()],
            );
            let _ = fs::remove_dir_all(&tree);
        }
        let _ = git(&self.here.repo, &["worktree", "prune"]);
    }
}

fn has_branch(repo: &Path, branch: &str) -> bool {
    !branch.is_empty()
        && git(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
        )
        .is_ok()
}

/// The claims a bare `docket land` queues: each reading to land with no landing step yet.
#[must_use]
pub fn unqueued(rows: &[Row]) -> Vec<&str> {
    rows.iter()
        .filter(|r| r.word == "to land" && r.claim_land.is_none())
        .map(|r| r.id.as_str())
        .collect()
}

/// The builds whose gate run is gone: in the `gating` stage, with their branch in this checkout,
/// while the checkout's lock is free. They go back in the queue.
#[must_use]
pub fn stranded(rows: &[Row], lock_free: bool, here: impl Fn(&str) -> bool) -> Vec<&str> {
    if !lock_free {
        return Vec::new();
    }
    rows.iter()
        .filter(|r| {
            r.claim_land
                .as_ref()
                .is_some_and(|l| l.stage == Stage::Gating)
        })
        .filter(|r| r.claim_branch.as_deref().is_some_and(&here))
        .map(|r| r.id.as_str())
        .collect()
}

/// Which of a batch's builds are still in the `gating` stage.
#[must_use]
pub fn still_gating<'a>(rows: &'a [Row], picked: &[String]) -> Vec<&'a str> {
    rows.iter()
        .filter(|r| picked.contains(&r.id))
        .filter(|r| {
            r.claim_land
                .as_ref()
                .is_some_and(|l| l.stage == Stage::Gating)
        })
        .map(|r| r.id.as_str())
        .collect()
}

/// Whether `docket land` makes sure a gate run is going: it queued something, or the queue holds
/// builds.
#[must_use]
pub fn run_wanted(queued: &[String], waiting: &[Row]) -> bool {
    !queued.is_empty() || !waiting.is_empty()
}

/// The next batch: up to `n` queued builds whose branch is in this checkout, oldest queued first.
#[must_use]
pub fn batch(rows: &[Row], n: usize, here: impl Fn(&str) -> bool) -> Vec<Row> {
    let mut queued: Vec<&Row> = rows
        .iter()
        .filter(|r| {
            r.claim_land
                .as_ref()
                .is_some_and(|l| l.stage == Stage::Queued)
        })
        .filter(|r| r.claim_branch.as_deref().is_some_and(&here))
        .collect();
    queued.sort_by(|a, b| {
        let at = |r: &Row| {
            r.claim_land
                .as_ref()
                .map(|l| l.at.clone())
                .unwrap_or_default()
        };
        at(a).cmp(&at(b))
    });
    queued.into_iter().take(n).cloned().collect()
}

/// The other builds of a failed gate run, which go back in the queue when one is sent back.
#[must_use]
pub fn rest_of_batch<'a>(rows: &'a [Row], id: &str, name: &str) -> Vec<&'a str> {
    rows.iter()
        .filter(|r| r.id != id)
        .filter(|r| {
            r.claim_land
                .as_ref()
                .is_some_and(|l| l.stage == Stage::GateFailed && l.batch.as_deref() == Some(name))
        })
        .map(|r| r.id.as_str())
        .collect()
}

/// A build's merge message: its commit's subject after `Merge`.
#[must_use]
pub fn merge_message(subject: &str) -> String {
    format!("Merge {subject}")
}

/// A checkout's gate run directory under the job state root, beside the project's jobs: `.land-`
/// and the checkout's path. It holds no `meta.json`, so it is never read as a job.
#[must_use]
pub fn run_dir(root: &Path, slug: &str, checkout: &Path) -> PathBuf {
    job::project_dir(root, slug).join(format!(
        ".land-{}",
        job::name_of(&checkout.display().to_string())
    ))
}

/// The last `n` lines of a log.
#[must_use]
pub fn log_tail(log: &str, n: usize) -> String {
    let lines: Vec<&str> = log.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
#[path = "../tests/land.rs"]
mod tests;
