//! A project's labels: a name and a description. An item carries a label it was given, and every item
//! under it inherits the label when it is read.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::item::Refused;

/// One label as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// A label name as stored: trimmed, and never empty.
///
/// # Errors
/// The name is empty.
pub fn check_name(name: &str) -> Result<String, Refused> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Refused("a label needs a name".to_string()));
    }
    Ok(name.to_string())
}

/// What a group's label name begins with: `--group NAME` gives the label `group:NAME`.
pub const GROUP: &str = "group:";

/// The label `--group` gives for a group name.
#[must_use]
pub fn of_group(name: &str) -> String {
    format!("{GROUP}{}", name.trim())
}

/// The group a label names, or `None` for a label that names none.
#[must_use]
pub fn group_named(label: &str) -> Option<&str> {
    label.strip_prefix(GROUP).filter(|g| !g.is_empty())
}

/// The group among an item's labels: the first that names one.
#[must_use]
pub fn group_of<S: AsRef<str>>(labels: &[S]) -> Option<&str> {
    labels.iter().find_map(|l| group_named(l.as_ref()))
}

/// What the label naming an item's repository begins with: `--repo PATH` gives `repo:PATH`.
pub const REPO: &str = "repo:";

/// The label `--repo` gives for a repository, by [`repo_path`]. A blank path gives none.
///
/// # Errors
/// As [`repo_path`].
pub fn of_repo(path: &str) -> Result<Option<String>, Refused> {
    Ok(repo_path(path)?.map(|p| format!("{REPO}{p}")))
}

/// A repository as an item names it: the path under the project root, without a leading `./` or a
/// trailing `/`, and `.` for the root itself. A blank path is none.
///
/// # Errors
/// The path is absolute, starts at a home directory or climbs out of the root.
pub fn repo_path(path: &str) -> Result<Option<String>, Refused> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(None);
    }
    if path.starts_with('/') || path.starts_with('~') || path.split('/').any(|p| p == "..") {
        return Err(Refused(format!(
            "a repo is a path under the project root, such as web or libs/core, not {path}"
        )));
    }
    let parts: Vec<&str> = path
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    Ok(Some(if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }))
}

/// The items of `all` that change the repository `path`: those whose nearest repo label, in
/// `nearest` by rid, names it, and when `path` is the project's default repository, those with none.
#[must_use]
pub fn in_repo(
    all: &[i64],
    nearest: &BTreeMap<i64, String>,
    path: &str,
    default: &str,
) -> HashSet<i64> {
    let same = |a: &str, b: &str| repo_path(a).ok().flatten() == repo_path(b).ok().flatten();
    all.iter()
        .copied()
        .filter(|rid| match nearest.get(rid) {
            Some(repo) => same(repo, path),
            None => same(default, path),
        })
        .collect()
}

/// The repository a label names, or `None` for a label that names none.
#[must_use]
pub fn repo_named(label: &str) -> Option<&str> {
    label.strip_prefix(REPO).filter(|r| !r.is_empty())
}

/// Whether a label names a repository.
#[must_use]
pub fn is_repo(label: &str) -> bool {
    label.starts_with(REPO)
}

/// The repository among an item's labels, its own before its plans': the first that names one.
#[must_use]
pub fn repo_of<S: AsRef<str>>(labels: &[S]) -> Option<&str> {
    labels.iter().find_map(|l| repo_named(l.as_ref()))
}

/// The label a name finds among `all`, ignoring case.
#[must_use]
pub fn find<'a>(all: &'a [Label], name: &str) -> Option<&'a Label> {
    let name = name.trim().to_lowercase();
    all.iter().find(|l| l.name.to_lowercase() == name)
}

/// What an audit prints for a label: its name, then its description when it has one.
#[must_use]
pub fn line(l: &Label) -> String {
    match l
        .description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        Some(d) => format!("{}: {d}", l.name),
        None => l.name.clone(),
    }
}

#[cfg(test)]
#[path = "tests/label.rs"]
mod tests;
