//! What only this machine knows: its git checkouts and the roots bound on it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `git ARGS` in a directory: its trimmed output, or `None` when it fails or the directory is missing.
#[must_use]
pub fn git(args: &[&str], cwd: &Path) -> Option<String> {
    if !cwd.is_dir() {
        return None;
    }
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The branch checked out in a directory, as the claim records it.
#[must_use]
pub fn branch(cwd: &Path) -> Option<String> {
    git(&["rev-parse", "--abbrev-ref", "HEAD"], cwd).filter(|b| !b.is_empty())
}

/// `a merge` or `a rebase` when the repository at the path is part way through one.
#[must_use]
pub fn unfinished(path: &Path) -> Option<&'static str> {
    let gitdir = PathBuf::from(git(&["rev-parse", "--absolute-git-dir"], path)?);
    if gitdir.join("MERGE_HEAD").exists() {
        return Some("a merge");
    }
    if ["rebase-merge", "rebase-apply"]
        .iter()
        .any(|d| gitdir.join(d).is_dir())
    {
        return Some("a rebase");
    }
    None
}

fn real(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The main worktree's directory for a path inside any worktree of a repository.
fn main_checkout(path: &Path) -> Option<PathBuf> {
    let common = PathBuf::from(git(&["rev-parse", "--git-common-dir"], path)?);
    let common = if common.is_absolute() {
        common
    } else {
        real(&path.join(common))
    };
    if common.file_name().is_some_and(|n| n == ".git") {
        return common.parent().map(Path::to_path_buf);
    }
    git(&["rev-parse", "--show-toplevel"], path).map(PathBuf::from)
}

/// The outermost repository above a directory, and the origin URLs met on the way up.
#[must_use]
pub fn outermost_repo(cwd: &Path) -> (Option<PathBuf>, Vec<String>) {
    let (mut top, mut urls) = (None, Vec::new());
    let mut here = real(cwd);
    while let Some(t) = main_checkout(&here) {
        let t = real(&t);
        if let Some(url) = git(&["remote", "get-url", "origin"], &t).filter(|u| !u.is_empty()) {
            urls.push(url);
        }
        top = Some(t.clone());
        match t.parent() {
            Some(parent) if parent != t => here = parent.to_path_buf(),
            _ => break,
        }
    }
    (top, urls)
}

/// The real path of a directory, as the roots record it.
#[must_use]
pub fn realpath(path: &Path) -> String {
    real(path).to_string_lossy().into_owned()
}

/// The project's repositories on this machine: each root with each of its repos, as directories.
#[must_use]
pub fn repo_dirs(
    all: &[docket_client::roots::Root],
    roots: &[String],
    repos: &[String],
) -> Vec<PathBuf> {
    let rels: Vec<String> = if repos.is_empty() {
        vec![".".into()]
    } else {
        repos.to_vec()
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for root in roots {
        for rel in &rels {
            let Ok(d) = resolve_repo(all, root, rel) else {
                continue;
            };
            if d.is_dir() && !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out
}

/// A path relative to a root, or a home-relative or absolute one as it stands.
#[must_use]
pub fn expand(root: &str, rel: &str) -> PathBuf {
    if let Some(rest) = rel.strip_prefix('~') {
        let home = std::env::var("HOME").unwrap_or_default();
        return PathBuf::from(format!("{home}{rest}"));
    }
    if Path::new(rel).is_absolute() {
        return PathBuf::from(rel);
    }
    Path::new(root).join(rel)
}

/// The directory a repo value names: a path under `root`, the filing project's root, or, for
/// `@OWNER/NAME[/PATH]`, a path under a root bound to that project in `roots` on this machine, the
/// shortest existing one, else the shortest.
///
/// # Errors
/// The named project has no root on this machine; the message names `docket bind`.
pub fn resolve_repo(
    roots: &[docket_client::roots::Root],
    root: &str,
    rel: &str,
) -> Result<PathBuf, String> {
    let Some((slug, sub)) = docket_core::label::foreign(rel) else {
        return Ok(expand(root, rel));
    };
    let mut bound: Vec<&str> = roots
        .iter()
        .filter(|r| r.project == slug)
        .map(|r| r.path.as_str())
        .collect();
    bound.sort_by_key(|p| p.len());
    let Some(other) = bound
        .iter()
        .find(|p| Path::new(p).is_dir())
        .or(bound.first())
    else {
        return Err(format!(
            "{slug} is not bound on this machine: clone it and run docket bind {slug} from its root"
        ));
    };
    Ok(expand(other, sub))
}

#[cfg(test)]
#[path = "tests/local.rs"]
mod tests;
