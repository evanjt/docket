//! What a lead does to a machine: choose one with a free slot, reach it, push a branch to it and fetch
//! one back. A machine is the one this client runs on when its name is the host of this client's key,
//! and any other is reached over ssh at the address the server records for it. Nothing here names a
//! machine, an address or a path: they all come from the server and the machine itself.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use docket_core::machine::Machine;

/// The job role an item's kind gives when the lead names none: a plan's audit, an investigation's
/// or a decided question's planning, anything else a build. A new plan with nothing opened is planned,
/// which the lead says with `--role plan`, as `docket next --role plan` listed it.
#[must_use]
pub fn role_of(kind: &str) -> &'static str {
    match kind {
        "audit" => "audit",
        "research" | "decision" => "plan",
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

/// The machine to start a job on: one with the runner and a free slot, the most free first, this
/// machine first among equals, then by name. `running` counts the jobs running on each machine.
#[must_use]
pub fn choose<'a>(
    machines: &'a [Machine],
    running: &BTreeMap<String, usize>,
    runner: &str,
    here: &str,
) -> Option<&'a Machine> {
    machines
        .iter()
        .filter(|m| m.runners.iter().any(|r| r == runner))
        .map(|m| (m, free(m, running)))
        .filter(|(_, free)| *free > 0)
        .min_by_key(|(m, free)| (std::cmp::Reverse(*free), m.name != here, m.name.clone()))
        .map(|(m, _)| m)
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

/// The command line run on another machine: `docket` with the arguments, found where a user install
/// puts it when the login's path does not have it.
#[must_use]
pub fn remote_line(args: &[String]) -> String {
    let words: Vec<String> = args.iter().map(|a| quote(a)).collect();
    format!(
        "PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:$PATH\" docket {}",
        words.join(" ")
    )
}

/// How a machine is reached from the one this client runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Via {
    Here,
    Ssh(String),
}

impl Via {
    #[must_use]
    pub fn of(m: &Machine, here: &str) -> Self {
        if m.name == here {
            Via::Here
        } else {
            Via::Ssh(m.ssh.clone())
        }
    }

    /// The git address of a repository at `path` on the machine: `user@host:path`, or a URL for an
    /// address given as `ssh://user@host:port`, the form that carries a port.
    #[must_use]
    pub fn git_url(&self, path: &str) -> String {
        match self {
            Via::Here => path.to_string(),
            Via::Ssh(addr) if addr.starts_with("ssh://") => {
                format!(
                    "{}/{}",
                    addr.trim_end_matches('/'),
                    path.trim_start_matches('/')
                )
            }
            Via::Ssh(addr) => format!("{addr}:{path}"),
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
            Via::Ssh(addr) => {
                let mut c = Command::new(ssh_program());
                c.args(["-o", "BatchMode=yes", addr.as_str(), &remote_line(args)]);
                c
            }
        };
        output(&mut cmd, &self.label())
    }

    fn label(&self) -> String {
        match self {
            Via::Here => "this machine".into(),
            Via::Ssh(addr) => addr.clone(),
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

#[cfg(test)]
#[path = "tests/dispatch.rs"]
mod tests;
