//! The git a pass runs in the dump checkout: its cursor, one commit, the push.

use std::path::Path;
use std::process::Command;

/// Where the cursor lives: the checkout's own config, never committed.
const CURSOR: &str = "docket.dumpcursor";

/// What a push did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pushed {
    /// No `origin` remote to push to.
    NoOrigin,
    /// Nothing ahead of the upstream.
    UpToDate,
    /// This many commits went to origin.
    Commits(usize),
}

/// git in the checkout, its stdout trimmed.
///
/// # Errors
/// git fails, with what it said.
pub fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(if out.stderr.is_empty() {
            &out.stdout
        } else {
            &out.stderr
        });
        return Err(format!("git {} failed: {}", args.join(" "), said.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The last event seq this checkout holds, or none before its first dump.
///
/// # Errors
/// The config holds something that is not a number.
pub fn cursor(repo: &Path) -> Result<Option<i64>, String> {
    let Ok(text) = git(repo, &["config", "--local", "--get", CURSOR]) else {
        return Ok(None);
    };
    text.parse()
        .map(Some)
        .map_err(|_| format!("{CURSOR} is {text:?}, not an event seq"))
}

/// # Errors
/// git cannot write the checkout's config.
pub fn set_cursor(repo: &Path, seq: i64) -> Result<(), String> {
    git(repo, &["config", "--local", CURSOR, &seq.to_string()]).map(|_| ())
}

/// The cursor forgotten, so the next pass writes every row.
///
/// # Errors
/// git cannot write the checkout's config.
pub fn forget_cursor(repo: &Path) -> Result<(), String> {
    match cursor(repo) {
        Ok(None) => Ok(()),
        _ => git(repo, &["config", "--local", "--unset", CURSOR]).map(|_| ()),
    }
}

/// One commit of the paths under the subject, by path so nothing else staged goes with it. The
/// short sha, or none when the paths hold nothing new.
///
/// # Errors
/// git refuses the add or the commit.
pub fn commit(repo: &Path, paths: &[String], subject: &str) -> Result<Option<String>, String> {
    if paths.is_empty() {
        return Ok(None);
    }
    let with = |head: &[&str]| -> Vec<String> {
        head.iter()
            .map(ToString::to_string)
            .chain(paths.iter().cloned())
            .collect()
    };
    run(repo, &with(&["add", "--"]))?;
    if run(repo, &with(&["diff", "--cached", "--name-only", "--"]))?.is_empty() {
        return Ok(None);
    }
    run(
        repo,
        &with(&["commit", "-q", "--no-gpg-sign", "-m", subject, "--"]),
    )?;
    git(repo, &["rev-parse", "--short", "HEAD"]).map(Some)
}

fn run(repo: &Path, args: &[String]) -> Result<String, String> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    git(repo, &args)
}

/// Every local commit to origin: fetched, rebased onto the upstream when it moved, then pushed.
///
/// # Errors
/// The fetch, rebase or push fails. A failed rebase is aborted, so the commits stay as they were.
pub fn push(repo: &Path) -> Result<Pushed, String> {
    if !git(repo, &["remote"])?.lines().any(|r| r == "origin") {
        return Ok(Pushed::NoOrigin);
    }
    let branch = git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let upstream = git(
        repo,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .unwrap_or_else(|_| format!("origin/{branch}"));
    git(repo, &["fetch", "-q", "origin"])?;
    let known = git(repo, &["rev-parse", "--verify", "-q", &upstream]).is_ok();
    if known {
        rebase(repo, &upstream)?;
    }
    let range = if known {
        format!("{upstream}..HEAD")
    } else {
        "HEAD".to_string()
    };
    let ahead: usize = git(repo, &["rev-list", "--count", &range])?
        .parse()
        .unwrap_or(0);
    if ahead == 0 {
        return Ok(Pushed::UpToDate);
    }
    git(repo, &["push", "-q", "-u", "origin", &branch])?;
    Ok(Pushed::Commits(ahead))
}

fn rebase(repo: &Path, upstream: &str) -> Result<(), String> {
    let behind = git(repo, &["rev-list", "--count", &format!("HEAD..{upstream}")])?;
    if behind == "0" {
        return Ok(());
    }
    if let Err(e) = git(repo, &["rebase", "-q", "--autostash", upstream]) {
        let _ = git(repo, &["rebase", "--abort"]);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/git.rs"]
mod tests;
