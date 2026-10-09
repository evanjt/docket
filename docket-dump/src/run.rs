//! One pass: the rows changed after the checkout's cursor, written, committed under one subject.

use std::fmt;
use std::path::Path;

use docket_core::dump::{
    DumpPage, commit_subject, events_path, files, merge_logs, messages, project_path, renames,
};

use crate::client::Source;
use crate::git;
use crate::tree;

/// What a pass did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub cursor: i64,
    pub full: bool,
    pub written: usize,
    pub commit: Option<(String, String)>,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = if self.full { "full dump" } else { "dump" };
        match &self.commit {
            Some((sha, subject)) => write!(
                f,
                "{what} to event {}: {} files written, committed {sha} {subject}",
                self.cursor, self.written
            ),
            None => write!(f, "{what} to event {}: nothing new", self.cursor),
        }
    }
}

/// The page after the cursor, or every row when there is none or `full` asks, written and committed.
/// The cursor moves only once the commit is made, so a pass that fails is taken again whole.
///
/// # Errors
/// The source, a write or git fails.
pub fn pass(source: &impl Source, repo: &Path, full: bool, host: &str) -> Result<Report, String> {
    let since = if full {
        0
    } else {
        git::cursor(repo)?.unwrap_or(0)
    };
    let page = source.page(since)?;
    let mut gone = Vec::new();
    for (old, new) in renames(&page) {
        if follow_rename(repo, &old, &new)? {
            gone.push(old);
        }
    }
    let subject = if page.full {
        format!("Sync dump ({host})")
    } else {
        commit_subject(&messages(&page.events))
    };
    let mut written = 0;
    for (path, text) in files(&page, |p| tree::read(repo, p)) {
        if tree::write(repo, &path, &text)? {
            written += 1;
        }
    }
    let sha = git::commit(repo, &paths(repo, &page, &gone), &subject)?;
    git::set_cursor(repo, page.cursor)?;
    Ok(Report {
        cursor: page.cursor,
        full: page.full,
        written,
        commit: sha.map(|s| (s, subject)),
    })
}

/// A renamed project's directory moved to its new slug, so its history stays whole: the directory
/// renamed when the new one does not exist yet; else the old event log merged into the new one and
/// the old directory removed. Whether the old directory was there; git sees the removals when the
/// old paths are staged with the commit.
fn follow_rename(repo: &Path, old: &str, new: &str) -> Result<bool, String> {
    let (from, to) = (repo.join(old), repo.join(new));
    if !from.is_dir() {
        return Ok(false);
    }
    if !to.is_dir() {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::rename(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
        return Ok(true);
    }
    let log = events_path(new);
    let merged = merge_logs(
        &tree::read(repo, &events_path(old)).unwrap_or_default(),
        &tree::read(repo, &log).unwrap_or_default(),
    );
    tree::write(repo, &log, &merged)?;
    std::fs::remove_dir_all(&from).map_err(|e| format!("{}: {e}", from.display()))?;
    Ok(true)
}

/// The paths of every project a page names, staged by project, one batch each, and the paths of
/// the projects whose directories a rename took away, so their removals go in the same commit.
fn paths(repo: &Path, page: &DumpPage, gone: &[String]) -> Vec<String> {
    let mut slugs: Vec<&str> = page.projects.iter().map(|p| p.slug.as_str()).collect();
    slugs.extend(page.items.iter().map(|i| i.project.as_str()));
    slugs.extend(page.events.iter().map(|e| e.project.as_str()));
    slugs.sort_unstable();
    slugs.dedup();
    let of = |slug: &str| {
        [
            project_path(slug),
            events_path(slug),
            format!("{slug}/items"),
        ]
    };
    let mut out: Vec<String> = slugs
        .into_iter()
        .flat_map(of)
        .filter(|rel| repo.join(rel).exists())
        .collect();
    out.extend(gone.iter().flat_map(|slug| of(slug)));
    out
}

#[cfg(test)]
#[path = "tests/run.rs"]
mod tests;
