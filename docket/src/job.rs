//! A job under a lead: one item worked by a headless Claude Code or Codex session in a worktree of
//! its own on this machine. Its state is a directory the lead reads, here or over ssh: `meta.json`,
//! the brief, the session's event stream, the pid and process group, the exit code and the final
//! message. Nothing here names a machine: the lead says where a job runs.

use std::fs;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The runners a job can be started on.
pub const RUNNERS: [&str; 2] = ["claude", "codex"];

/// Each role's brief, with `{id}` and `{branch}` filled in when a job starts.
pub const BRIEFS: [(&str, &str); 3] = [
    ("build", include_str!("../../skills/job/build.md")),
    ("audit", include_str!("../../skills/job/audit.md")),
    ("plan", include_str!("../../skills/job/plan.md")),
];

/// The verbs only the lead runs: a job asking for one is refused.
pub const LEAD_VERBS: [&str; 5] = ["start", "release", "close", "drop", "reopen"];

/// The shell that runs the session and records its exit code once it ends. `$1` is the job's
/// directory, the rest the session's command.
const WRAP: &str = r#"d=$1; shift
"$@" </dev/null >"$d/events.jsonl" 2>"$d/stderr"
echo $? >"$d/exit.part" && mv "$d/exit.part" "$d/exit""#;

/// What a job is started with, recorded as `meta.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub name: String,
    pub project: String,
    pub id: String,
    pub branch: String,
    pub runner: String,
    pub model: String,
    pub effort: Option<String>,
    pub role: String,
    pub worktree: String,
    /// Seconds since the epoch.
    pub started: u64,
    /// The commit the job started from, which its diff is taken against.
    #[serde(default)]
    pub base: Option<String>,
}

/// The brief for a role, filled in for one item on one branch.
#[must_use]
pub fn brief(role: &str, id: &str, branch: &str) -> Option<String> {
    BRIEFS
        .iter()
        .find(|(r, _)| *r == role)
        .map(|(_, text)| text.replace("{id}", id).replace("{branch}", branch))
}

/// A job's name: its branch, with anything but letters, digits, `.`, `_` and `-` as `-`.
#[must_use]
pub fn name_of(branch: &str) -> String {
    let name: String = branch
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    name.trim_matches('-').to_string()
}

/// The directory a project's jobs live in under the state root: the slug with `/` as `-`.
#[must_use]
pub fn project_dir(root: &Path, slug: &str) -> PathBuf {
    root.join(slug.replace('/', "-"))
}

/// Where jobs are kept: `DOCKET_JOB_STATE`, else `$XDG_STATE_HOME/docket/jobs`, else
/// `~/.local/state/docket/jobs`.
#[must_use]
pub fn state_root() -> PathBuf {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    if let Some(dir) = var("DOCKET_JOB_STATE") {
        return PathBuf::from(dir);
    }
    let base = var("XDG_STATE_HOME").map_or_else(
        || PathBuf::from(var("HOME").unwrap_or_default()).join(".local/state"),
        PathBuf::from,
    );
    base.join("docket/jobs")
}

/// The program a runner starts: `DOCKET_JOB_RUNNER_CLAUDE` or `DOCKET_JOB_RUNNER_CODEX` when set,
/// else the runner's own name on the path.
#[must_use]
pub fn program(runner: &str) -> String {
    std::env::var(format!("DOCKET_JOB_RUNNER_{}", runner.to_uppercase()))
        .ok()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| runner.to_string())
}

/// The session's arguments for a runner: headless, every permission granted inside the worktree,
/// the events as JSON lines, and for Codex the final message written to `last`.
#[must_use]
pub fn args(
    runner: &str,
    model: &str,
    effort: Option<&str>,
    brief: &str,
    worktree: &Path,
    last: &Path,
) -> Vec<String> {
    let s = |v: &str| v.to_string();
    let mut out = Vec::new();
    if runner == "codex" {
        out.extend([s("exec"), s("--json"), s("-m"), s(model)]);
        if let Some(e) = effort {
            out.extend([s("-c"), format!("model_reasoning_effort=\"{e}\"")]);
        }
        out.extend([
            s("--dangerously-bypass-approvals-and-sandbox"),
            s("-C"),
            worktree.display().to_string(),
            s("-o"),
            last.display().to_string(),
            s(brief),
        ]);
    } else {
        out.extend([s("-p"), s(brief), s("--model"), s(model)]);
        if let Some(e) = effort {
            out.extend([s("--effort"), s(e)]);
        }
        out.extend([
            s("--dangerously-skip-permissions"),
            s("--output-format"),
            s("stream-json"),
            s("--verbose"),
        ]);
    }
    out
}

/// Why a verb is refused inside a job, when it is one only the lead runs.
#[must_use]
pub fn refusal(verb: &str, job: Option<&str>) -> Option<String> {
    let job = job.filter(|j| !j.is_empty())?;
    LEAD_VERBS.contains(&verb).then(|| {
        format!(
            "docket {verb} is refused inside the job {job}: the lead claims, merges and closes. \
             End with the report line and the lead reads it."
        )
    })
}

/// Why `new` or `add` is refused inside a job: every item a job files names its release.
#[must_use]
pub fn unreleased(verb: &str, job: Option<&str>, release: Option<&str>) -> Option<String> {
    let job = job.filter(|j| !j.is_empty())?;
    release.is_none_or(|r| r.trim().is_empty()).then(|| {
        format!(
            "docket {verb} inside the job {job} needs --release: current, a later release, or a \
             theme in use, chosen by what the item is, as the brief's triage says."
        )
    })
}

/// The report a job ends its final message with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "word", content = "what", rename_all = "UPPERCASE")]
pub enum Report {
    /// Done: its change is left in its worktree for the lead to commit. A sha, from a job that
    /// committed, is kept.
    Done(String),
    /// Waiting on this question.
    Waiting(String),
    /// Not done, for this reason.
    Failed(String),
}

impl Report {
    #[must_use]
    pub fn line(&self) -> String {
        match self {
            Report::Done(s) if s.is_empty() => "DONE".into(),
            Report::Done(s) => format!("DONE {s}"),
            Report::Waiting(q) => format!("WAITING {q}"),
            Report::Failed(r) => format!("FAILED {r}"),
        }
    }
}

/// One line read as a report: `DONE`, `WAITING Q<n>` or `FAILED <reason>`; `DONE <sha>` from a
/// job that committed is read too.
fn report_line(line: &str) -> Option<Report> {
    let line = line.trim().trim_matches('`').trim();
    if line == "DONE" {
        return Some(Report::Done(String::new()));
    }
    let (word, rest) = line.split_once(char::is_whitespace)?;
    let rest = rest.trim();
    match word {
        "DONE" if rest.len() >= 7 && rest.chars().all(|c| c.is_ascii_hexdigit()) => {
            Some(Report::Done(rest.to_string()))
        }
        "WAITING"
            if rest.len() > 1
                && rest.starts_with('Q')
                && rest[1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            Some(Report::Waiting(rest.to_string()))
        }
        "FAILED" if !rest.is_empty() => Some(Report::Failed(rest.to_string())),
        _ => None,
    }
}

/// The report and note of a final message: the report is its last line that says something (a
/// code fence aside), and the note the last line starting `NOTE `.
#[must_use]
pub fn report(text: &str) -> (Option<Report>, Option<String>) {
    let last = text
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty() && !l.chars().all(|c| c == '`'));
    let note = text
        .lines()
        .map(str::trim)
        .rfind(|l| l.starts_with("NOTE "))
        .map(|l| l["NOTE ".len()..].trim().to_string());
    (last.and_then(report_line), note)
}

/// What a job saw on the way that is below the bar for an item of its own: every line of its final
/// message starting `OBSERVE `, in order.
#[must_use]
pub fn observations(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("OBSERVE "))
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .map(str::to_string)
        .collect()
}

/// The observations as appended to the job's item.
#[must_use]
pub fn observed(job: &str, observations: &[String]) -> String {
    let list: Vec<String> = observations.iter().map(|o| format!("- {o}")).collect();
    format!(
        "**Observations, from the job {job}.**\n\n{}",
        list.join("\n")
    )
}

/// The commit message a job proposes: the last line of its final message starting `MESSAGE `.
#[must_use]
pub fn message(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .rfind(|l| l.starts_with("MESSAGE "))
        .map(|l| l["MESSAGE ".len()..].trim().to_string())
        .filter(|m| !m.is_empty())
}

/// What a session's event stream says: its final message and the output tokens it spent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Read {
    pub last: Option<String>,
    pub tokens: Option<u64>,
}

/// A Claude Code stream: the longest `result` is the final message, and the tokens are summed over
/// every result. A Codex stream: the tokens of every completed turn; its final message is a file.
#[must_use]
pub fn read_events(runner: &str, events: &str) -> Read {
    let mut out = Read::default();
    for line in events.lines() {
        let Ok(e) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let counted = match (runner, e["type"].as_str()) {
            ("codex", Some("turn.completed")) => true,
            ("claude", Some("result")) => {
                if let Some(r) = e["result"].as_str()
                    && out.last.as_ref().is_none_or(|l| r.len() > l.len())
                {
                    out.last = Some(r.to_string());
                }
                true
            }
            _ => false,
        };
        if counted && let Some(n) = e["usage"]["output_tokens"].as_u64() {
            out.tokens = Some(out.tokens.unwrap_or(0) + n);
        }
    }
    out
}

/// Where a job stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Running,
    Done,
    Failed,
    /// No exit was recorded and its process is gone.
    Lost,
}

impl State {
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            State::Running => "running",
            State::Done => "done",
            State::Failed => "failed",
            State::Lost => "lost",
        }
    }
}

/// A job's state from its exit code, whether it was killed, and whether its process lives.
#[must_use]
pub fn state(exit: Option<&str>, killed: bool, alive: bool) -> State {
    match exit.map(str::trim) {
        Some("0") => State::Done,
        Some(_) => State::Failed,
        None if killed => State::Failed,
        None if alive => State::Running,
        None => State::Lost,
    }
}

/// Whether a process runs, as opposed to having ended or waiting to be reaped.
#[must_use]
pub fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks that the process exists and may be signalled.
    if pid <= 0 || unsafe { libc::kill(pid, 0) } != 0 {
        return false;
    }
    stat(pid).is_none_or(|(st, _)| st != 'Z')
}

/// A process's state letter and process group, from `/proc` where there is one.
fn stat(pid: i32) -> Option<(char, i32)> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after = text.rsplit_once(')')?.1;
    let mut f = after.split_whitespace();
    let st = f.next()?.chars().next()?;
    let _ppid = f.next()?;
    let pgrp = f.next()?.parse().ok()?;
    Some((st, pgrp))
}

/// Whether any process of a group still runs.
#[must_use]
pub fn group_alive(pgid: i32) -> bool {
    if pgid <= 0 {
        return false;
    }
    if let Ok(entries) = fs::read_dir("/proc") {
        return entries
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.parse::<i32>().ok())
            .filter_map(stat)
            .any(|(st, g)| g == pgid && st != 'Z');
    }
    // SAFETY: signal 0 only checks that a member of the group exists.
    unsafe { libc::killpg(pgid, 0) == 0 }
}

/// Seconds since the epoch.
#[must_use]
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn read(dir: &Path, file: &str) -> Option<String> {
    fs::read_to_string(dir.join(file)).ok()
}

fn number(dir: &Path, file: &str) -> Option<i32> {
    read(dir, file)?.trim().parse().ok()
}

/// One job as `status` lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Row {
    pub name: String,
    pub project: String,
    pub id: String,
    pub branch: String,
    pub runner: String,
    pub model: String,
    pub effort: Option<String>,
    pub role: String,
    pub state: State,
    pub minutes: u64,
    pub tokens: Option<u64>,
    pub report: Option<String>,
    pub note: Option<String>,
    /// The one-line commit message the job proposes.
    pub message: Option<String>,
    /// What it saw on the way, below the bar for an item of its own.
    pub observations: Vec<String>,
    /// The commit the job started from.
    pub base: Option<String>,
    /// The final message in full: the result of a Claude session, the last message file of Codex.
    pub last: Option<String>,
    pub worktree: String,
    pub dir: String,
}

/// A job read from its directory; `None` when it holds no readable `meta.json`.
#[must_use]
pub fn row(dir: &Path, at: u64) -> Option<Row> {
    let meta: Meta = serde_json::from_str(&read(dir, "meta.json")?).ok()?;
    let exit = read(dir, "exit");
    let killed = dir.join("killed").exists();
    let live = number(dir, "pid").is_some_and(|pid| {
        alive(pid) && number(dir, "pgid").is_none_or(|g| stat(pid).is_none_or(|(_, pg)| pg == g))
    });
    let st = state(exit.as_deref(), killed, live);
    let events = read(dir, "events.jsonl").unwrap_or_default();
    let got = read_events(&meta.runner, &events);
    let last = read(dir, "final").or(got.last);
    let (said, note) = last.as_deref().map_or((None, None), report);
    let said = said.or_else(|| killed.then(|| Report::Failed("killed".into())));
    let ended = ["exit", "killed"]
        .iter()
        .filter_map(|f| fs::metadata(dir.join(f)).ok()?.modified().ok())
        .filter_map(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .min()
        .unwrap_or(at);
    Some(Row {
        name: meta.name,
        project: meta.project,
        id: meta.id,
        branch: meta.branch,
        runner: meta.runner,
        model: meta.model,
        effort: meta.effort,
        role: meta.role,
        state: st,
        minutes: ended.saturating_sub(meta.started) / 60,
        tokens: got.tokens,
        report: said.map(|r| r.line()),
        note,
        message: last.as_deref().and_then(message),
        observations: last.as_deref().map(observations).unwrap_or_default(),
        base: meta.base,
        last,
        worktree: meta.worktree,
        dir: dir.display().to_string(),
    })
}

/// Every job under the state root, by project and then start.
#[must_use]
pub fn rows(root: &Path, at: u64) -> Vec<(Row, u64)> {
    let mut out = Vec::new();
    let Ok(projects) = fs::read_dir(root) else {
        return out;
    };
    for p in projects.flatten() {
        let Ok(jobs) = fs::read_dir(p.path()) else {
            continue;
        };
        for j in jobs.flatten() {
            let dir = j.path();
            let started = read(&dir, "meta.json")
                .and_then(|m| serde_json::from_str::<Meta>(&m).ok())
                .map_or(0, |m| m.started);
            if let Some(r) = row(&dir, at) {
                out.push((r, started));
            }
        }
    }
    out.sort_by(|(a, sa), (b, sb)| (&a.project, sa, &a.name).cmp(&(&b.project, sb, &b.name)));
    out
}

/// The directory of the job with this name, within one project when one is given.
///
/// # Errors
/// No job has the name, or jobs of several projects do and none was given.
pub fn find(root: &Path, name: &str, project: Option<&str>) -> Result<PathBuf, String> {
    let found: Vec<PathBuf> = rows(root, now())
        .into_iter()
        .filter(|(r, _)| r.name == name && project.is_none_or(|p| r.project == p))
        .map(|(r, _)| PathBuf::from(r.dir))
        .collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(format!("no job {name} on this machine")),
        _ => Err(format!(
            "jobs named {name} in several projects here: pass -p SLUG"
        )),
    }
}

/// What a job is started from.
#[derive(Clone, Debug)]
pub struct Spec {
    pub project: String,
    pub id: String,
    pub branch: String,
    pub runner: String,
    pub model: String,
    pub effort: Option<String>,
    pub role: String,
}

/// A job started: its name, its directory, and the process the caller may wait on or leave.
pub struct Started {
    pub name: String,
    pub dir: PathBuf,
    pub worktree: PathBuf,
    pub child: Child,
}

fn git(checkout: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Start a job: a worktree beside the checkout on the branch the lead pushed, the brief, and the
/// session, `program` with the runner's arguments, detached in a process group of its own, its exit
/// code recorded when it ends.
///
/// # Errors
/// An unknown runner or role, the job or its worktree already there, the branch missing from the
/// checkout, or the worktree or the process failing to start.
pub fn run(
    spec: &Spec,
    checkout: &Path,
    root: &Path,
    env: &[(&str, &str)],
    program: &str,
) -> Result<Started, String> {
    if !RUNNERS.contains(&spec.runner.as_str()) {
        return Err(format!(
            "no runner {}: one of {}",
            spec.runner,
            RUNNERS.join(", ")
        ));
    }
    let Some(text) = brief(&spec.role, &spec.id, &spec.branch) else {
        let roles: Vec<&str> = BRIEFS.iter().map(|(r, _)| *r).collect();
        return Err(format!(
            "no role {}: one of {}",
            spec.role,
            roles.join(", ")
        ));
    };
    let name = name_of(&spec.branch);
    let dir = project_dir(root, &spec.project).join(&name);
    if dir.exists() {
        return Err(format!(
            "a job {name} is already here ({}): kill it or pick another branch",
            dir.display()
        ));
    }
    let reference = format!("refs/heads/{}", spec.branch);
    let Ok(base) = git(checkout, &["rev-parse", "--verify", "--quiet", &reference]) else {
        return Err(format!(
            "no branch {} in {}: the lead pushes it there before starting the job",
            spec.branch,
            checkout.display()
        ));
    };
    let stem = spec.project.rsplit('/').next().unwrap_or(&spec.project);
    let parent = checkout.parent().unwrap_or(checkout);
    let worktree = parent.join(format!("{stem}-{name}"));
    if worktree.exists() {
        return Err(format!("{} is already there", worktree.display()));
    }
    git(
        checkout,
        &[
            "worktree",
            "add",
            &worktree.display().to_string(),
            &spec.branch,
        ],
    )?;
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let meta = Meta {
        name: name.clone(),
        project: spec.project.clone(),
        id: spec.id.clone(),
        branch: spec.branch.clone(),
        runner: spec.runner.clone(),
        model: spec.model.clone(),
        effort: spec.effort.clone(),
        role: spec.role.clone(),
        worktree: worktree.display().to_string(),
        started: now(),
        base: Some(base),
    };
    let write = |file: &str, text: &str| {
        fs::write(dir.join(file), text).map_err(|e| format!("{}: {e}", dir.join(file).display()))
    };
    write(
        "meta.json",
        &serde_json::to_string_pretty(&meta).unwrap_or_default(),
    )?;
    write("brief.md", &text)?;
    let argv = args(
        &spec.runner,
        &spec.model,
        spec.effort.as_deref(),
        &text,
        &worktree,
        &dir.join("final"),
    );
    let child = spawn(&dir, &worktree, program, &argv, env)
        .map_err(|e| format!("cannot start {}: {e}", spec.runner))?;
    let pid = child.id().to_string();
    write("pid", &pid)?;
    write("pgid", &pid)?;
    Ok(Started {
        name,
        dir,
        worktree,
        child,
    })
}

/// The session under the recording shell, in a new session and process group, reading nothing.
fn spawn(
    dir: &Path,
    cwd: &Path,
    program: &str,
    argv: &[String],
    env: &[(&str, &str)],
) -> io::Result<Child> {
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(WRAP)
        .arg("sh")
        .arg(dir)
        .arg(program)
        .args(argv)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    // SAFETY: setsid is async-signal-safe and touches nothing the parent shares.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn()
}

/// A job's change, against the commit it started from, as a patch `git apply` takes: what it
/// committed and what it left in its worktree, new files and binary ones included. The worktree's
/// changes are staged to be read; nothing is committed.
///
/// # Errors
/// The job is still running, has no worktree, or git fails.
pub fn diff(dir: &Path) -> Result<String, String> {
    let r = row(dir, now()).ok_or_else(|| format!("{}: no job here", dir.display()))?;
    if r.state == State::Running {
        return Err(format!("{} is still running", r.name));
    }
    let worktree = Path::new(&r.worktree);
    if !worktree.is_dir() {
        return Err(format!("{} has no worktree at {}", r.name, r.worktree));
    }
    git(worktree, &["add", "-A"])?;
    let base = r.base.unwrap_or_else(|| "HEAD".into());
    let out = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args([
            "diff",
            "--cached",
            "--binary",
            "--no-color",
            "--no-ext-diff",
            &base,
        ])
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Stop a job's process group, the whole of it, and record that it was killed.
///
/// # Errors
/// The job has no process group recorded, or the group outlives a forced kill.
pub fn kill(dir: &Path) -> Result<(), String> {
    let pgid = number(dir, "pgid").ok_or_else(|| format!("{}: no pgid", dir.display()))?;
    if dir.join("exit").exists() {
        return Err("the job has already ended".into());
    }
    fs::write(dir.join("killed"), now().to_string()).map_err(|e| e.to_string())?;
    for (sig, wait) in [(libc::SIGTERM, 5), (libc::SIGKILL, 2)] {
        // SAFETY: signals only the job's own process group.
        unsafe {
            libc::killpg(pgid, sig);
        }
        let until = Instant::now() + Duration::from_secs(wait);
        while Instant::now() < until {
            if !group_alive(pgid) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    Err(format!("process group {pgid} outlived SIGKILL"))
}

/// Remove a job that has ended: its worktree, its branch in the checkout unless kept, and its
/// directory.
///
/// # Errors
/// The job is still running, or git refuses to remove the worktree or the branch.
pub fn remove(dir: &Path, keep_branch: bool) -> Result<(), String> {
    let at = now();
    let r = row(dir, at).ok_or_else(|| format!("{}: no job here", dir.display()))?;
    if r.state == State::Running {
        return Err(format!(
            "{} is still running: docket job kill {} first",
            r.name, r.name
        ));
    }
    let worktree = Path::new(&r.worktree);
    if worktree.is_dir() {
        let common = git(
            worktree,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        let checkout = Path::new(&common)
            .parent()
            .map_or_else(|| PathBuf::from(&common), Path::to_path_buf);
        git(&checkout, &["worktree", "remove", "--force", &r.worktree])?;
        if !keep_branch {
            git(&checkout, &["branch", "-D", &r.branch])?;
        }
    }
    fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// The last `n` lines of a job's event stream, then of its stderr when the stream is empty.
#[must_use]
pub fn tail(dir: &Path, n: usize) -> String {
    let events = read(dir, "events.jsonl").unwrap_or_default();
    let text = if events.trim().is_empty() {
        read(dir, "stderr").unwrap_or_default()
    } else {
        events
    };
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
#[path = "tests/job.rs"]
mod tests;
