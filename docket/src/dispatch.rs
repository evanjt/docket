//! What a lead does to a machine: choose one with a free slot, reach it, push a branch to it, fetch
//! one back and commit a job's change here. A machine is the one this client runs on when its name is the host of this client's key,
//! and any other is reached over ssh at the address the server records for it. Nothing here names a
//! machine, an address or a path: they all come from the server and the machine itself.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use docket_core::machine::Machine;

use crate::job::Change;

/// The job role an item's kind gives when the lead names none: an investigation's or a decided
/// question's planning, anything else a build. A plan with children is audited and one with none is
/// planned, as the queue gives it.
#[must_use]
pub fn role_of(kind: &str, has_children: bool) -> &'static str {
    match kind {
        "audit" if has_children => "audit",
        "audit" | "research" | "decision" => "plan",
        _ => "build",
    }
}

/// The branch a dispatch claims an item on: the lead's prefix, the id, and a number nobody guesses.
#[must_use]
pub fn branch_for(id: &str, n: u32) -> String {
    format!("lead/{}-{n}", id.to_lowercase())
}

/// A number for a branch, from the clock and the process, so two leads never pick the same one.
#[must_use]
pub fn nonce() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    (nanos ^ std::process::id().rotate_left(16)) % 100_000
}

/// The machine to start a job on: one with the runner, not under a usage limit of it at `now`, and
/// a free slot, the most free first, this machine first among equals, then by name. `running`
/// counts the jobs running on each machine.
#[must_use]
pub fn choose<'a>(
    machines: &'a [Machine],
    running: &BTreeMap<String, usize>,
    runner: &str,
    here: &str,
    now: &str,
) -> Option<&'a Machine> {
    machines
        .iter()
        .filter(|m| m.runners.iter().any(|r| r == runner) && m.limited(runner, now).is_none())
        .map(|m| (m, free(m, running)))
        .filter(|(_, free)| *free > 0)
        .min_by_key(|(m, free)| (std::cmp::Reverse(*free), m.name != here, m.name.clone()))
        .map(|(m, _)| m)
}

/// The runners under a usage limit at `now` on every machine that has them, each with the earliest
/// reset among those machines: the runners no dispatch can use until then.
#[must_use]
pub fn limited_runners(machines: &[Machine], now: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for runner in docket_core::machine::RUNNERS {
        let having: Vec<&Machine> = machines
            .iter()
            .filter(|m| m.runners.iter().any(|r| r == runner))
            .collect();
        let resets: Vec<&str> = having
            .iter()
            .filter_map(|m| m.limited(runner, now))
            .collect();
        if !having.is_empty() && resets.len() == having.len() {
            if let Some(first) = resets.into_iter().min() {
                out.insert(runner.to_string(), first.to_string());
            }
        }
    }
    out
}

/// The slots a machine has left.
#[must_use]
pub fn free(m: &Machine, running: &BTreeMap<String, usize>) -> usize {
    let slots = usize::try_from(m.slots).unwrap_or(0);
    slots.saturating_sub(running.get(&m.name).copied().unwrap_or(0))
}

/// A word for a POSIX shell, quoted so it reaches the program as it is.
#[must_use]
pub fn quote(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:,@+%".contains(c))
    {
        return word.to_string();
    }
    format!("'{}'", word.replace('\'', r"'\''"))
}

/// The directories put before `PATH` on a machine whose record names none.
const DEFAULT_PATH: &str = "$HOME/.local/bin:$HOME/.cargo/bin";

/// The command line run on another machine: `docket` with the arguments, found in the machine's
/// path prefix, a user install's directories by default, when the login's path does not have it.
#[must_use]
pub fn remote_line(args: &[String], path: Option<&str>) -> String {
    let words: Vec<String> = args.iter().map(|a| quote(a)).collect();
    format!(
        "PATH=\"{}:$PATH\" docket {}",
        path.unwrap_or(DEFAULT_PATH),
        words.join(" ")
    )
}

/// How a machine is reached from the one this client runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Via {
    Here,
    /// The address and the machine's path prefix.
    Ssh(String, Option<String>),
}

impl Via {
    #[must_use]
    pub fn of(m: &Machine, here: &str) -> Self {
        if m.name == here {
            Via::Here
        } else {
            Via::Ssh(m.ssh.clone(), m.path.clone())
        }
    }

    /// The git address of a repository at `path` on the machine: `user@host:path`, or a URL for an
    /// address given as `ssh://user@host:port`, the form that carries a port.
    #[must_use]
    pub fn git_url(&self, path: &str) -> String {
        match self {
            Via::Here => path.to_string(),
            Via::Ssh(addr, _) if addr.starts_with("ssh://") => {
                format!(
                    "{}/{}",
                    addr.trim_end_matches('/'),
                    path.trim_start_matches('/')
                )
            }
            Via::Ssh(addr, _) => format!("{addr}:{path}"),
        }
    }

    /// `docket ARGS` on the machine: its output, or what it printed when it failed.
    ///
    /// # Errors
    /// The command could not be started or exited non-zero.
    pub fn docket(&self, args: &[String]) -> Result<String, String> {
        let mut cmd = match self {
            Via::Here => {
                let exe = std::env::current_exe().map_err(|e| format!("docket: {e}"))?;
                let mut c = Command::new(exe);
                c.args(args);
                c
            }
            Via::Ssh(addr, path) => {
                let mut c = Command::new(ssh_program());
                c.args([
                    "-o",
                    "BatchMode=yes",
                    addr.as_str(),
                    &remote_line(args, path.as_deref()),
                ]);
                c
            }
        };
        output(&mut cmd, &self.label())
    }

    fn label(&self) -> String {
        match self {
            Via::Here => "this machine".into(),
            Via::Ssh(addr, _) => addr.clone(),
        }
    }
}

/// The ssh program, `DOCKET_SSH` when set, which a test points at a stand-in.
#[must_use]
pub fn ssh_program() -> String {
    std::env::var("DOCKET_SSH")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "ssh".into())
}

fn output(cmd: &mut Command, label: &str) -> Result<String, String> {
    let out = cmd.output().map_err(|e| format!("{label}: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(format!(
        "{label}: {}",
        if stderr.is_empty() { stdout } else { stderr }
    ))
}

/// `git ARGS` in a repository, its ssh the one `docket` uses, never asking for a password.
///
/// # Errors
/// git could not be started or exited non-zero.
pub fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(args);
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        cmd.env(
            "GIT_SSH_COMMAND",
            format!("{} -o BatchMode=yes", ssh_program()),
        );
    }
    output(&mut cmd, "git")
}

/// The commits a job's change was made into here, none of them on a branch yet.
#[derive(Debug)]
pub struct Made {
    /// The commit in the lead's checkout.
    pub sha: String,
    /// Each submodule's commit, with the repository it was made in.
    pub inside: Vec<(PathBuf, String)>,
}

impl Made {
    /// The branch set to these commits, in the checkout and in each submodule they were made in.
    ///
    /// # Errors
    /// git refuses to set a branch.
    pub fn keep(&self, repo: &Path, branch: &str) -> Result<(), String> {
        for (dir, sha) in &self.inside {
            git(dir, &["branch", "-f", branch, sha])?;
        }
        git(repo, &["branch", "-f", branch, &self.sha]).map(|_| ())
    }
}

/// A job's change committed with `message` on `base`, in a worktree of `repo` removed afterwards.
/// Each submodule's change is first made a commit in this checkout's own repository of that
/// submodule, on the commit it is pinned at, and the commit here points at it. No branch moves.
///
/// # Errors
/// A submodule is not checked out here or lacks its pinned commit, a patch does not apply, or a
/// commit is refused.
pub fn commit_change(
    repo: &Path,
    base: &str,
    change: &Change,
    message: &str,
) -> Result<Made, String> {
    let mut inside = Vec::new();
    let pins = commit_submodules(repo, change, message, &mut inside)?;
    let tmp = scratch("worktree");
    let tmp_s = tmp.display().to_string();
    git(repo, &["worktree", "add", "--detach", &tmp_s, base])?;
    let done = (|| {
        if !change.patch.trim().is_empty() {
            apply(&tmp, None, "--index", &change.patch)?;
        }
        pin(&tmp, None, &pins)?;
        git(&tmp, &["commit", "-q", "-m", message])?;
        git(&tmp, &["rev-parse", "HEAD"])
    })();
    let _ = git(repo, &["worktree", "remove", "--force", &tmp_s]);
    Ok(Made { sha: done?, inside })
}

/// Each submodule's change committed in its repository under `dir`, as the path and commit to pin.
fn commit_submodules(
    dir: &Path,
    change: &Change,
    message: &str,
    inside: &mut Vec<(PathBuf, String)>,
) -> Result<Vec<(String, String)>, String> {
    let mut pins = Vec::new();
    for s in &change.submodules {
        let sha = commit_inside(&dir.join(&s.path), &s.base, &s.change, message, inside)
            .map_err(|e| format!("submodule {}: {e}", s.path))?;
        pins.push((s.path.clone(), sha));
    }
    Ok(pins)
}

/// A submodule's change as one commit on `base` in XX
fn commit_inside(
    dir: &Path,
    base: &str,
    change: &Change,
    message: &str,
    inside: &mut Vec<(PathBuf, String)>,
) -> Result<String, String> {
    if !crate::job::is_repository(dir) {
        return Err(format!(
            "not checked out at {}: git submodule update --init, then collect again",
            dir.display()
        ));
    }
    git(dir, &["cat-file", "-e", &format!("{base}^{{commit}}")]).map_err(|_| {
        format!(
            "no commit {base} in {}, the one it is pinned at",
            dir.display()
        )
    })?;
    let pins = commit_submodules(dir, change, message, inside)?;
    let index = scratch("index");
    let made = (|| {
        let at = Some(index.as_path());
        index_git(dir, at, &["read-tree", base])?;
        if !change.patch.trim().is_empty() {
            apply(dir, at, "--cached", &change.patch)?;
        }
        pin(dir, at, &pins)?;
        let tree = index_git(dir, at, &["write-tree"])?;
        let signs = git(dir, &["config", "--type=bool", "--get", "commit.gpgsign"])
            .is_ok_and(|v| v == "true");
        let mut args = vec!["commit-tree"];
        if signs {
            args.push("-S");
        }
        args.extend([tree.as_str(), "-p", base, "-m", message]);
        git(dir, &args)
    })();
    let _ = std::fs::remove_file(&index);
    let sha = made?;
    inside.push((dir.to_path_buf(), sha.clone()));
    Ok(sha)
}

/// Each submodule at `path` pinned at its commit in the index.
fn pin(dir: &Path, index: Option<&Path>, pins: &[(String, String)]) -> Result<(), String> {
    for (path, sha) in pins {
        let info = format!("160000,{sha},{path}");
        index_git(dir, index, &["update-index", "--cacheinfo", &info])?;
    }
    Ok(())
}

/// A patch applied to the index, given on standard input so nothing of it is lost on the way.
fn apply(dir: &Path, index: Option<&Path>, to: &str, patch: &str) -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(["apply", to, "--binary"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(i) = index {
        cmd.env("GIT_INDEX_FILE", i);
    }
    let mut child = cmd.spawn().map_err(|e| format!("git: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let mut text = patch.to_string();
        if !text.ends_with('\n') {
            text.push('\n');
        }
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("git apply: {e}"))?;
    }
    let out = child.wait_with_output().map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    Err(format!(
        "git apply: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}

/// `git ARGS` on the index at `index`, or the repository's own when none is given.
fn index_git(dir: &Path, index: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    if let Some(i) = index {
        cmd.env("GIT_INDEX_FILE", i);
    }
    output(&mut cmd, "git")
}

/// A path for a scratch file or worktree of this process, a different one on every call.
fn scratch(kind: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("docket-collect-{}-{n}.{kind}", std::process::id()))
}

#[cfg(test)]
#[path = "tests/dispatch.rs"]
mod tests;
