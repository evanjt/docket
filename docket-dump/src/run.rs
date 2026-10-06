//! One pass: the rows changed after the checkout's cursor, written, committed under one subject.

use std::fmt;
use std::path::Path;

use docket_core::dump::{DumpPage, commit_subject, events_path, files, messages, project_path};

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
    let sha = git::commit(repo, &paths(repo, &page), &subject)?;
    git::set_cursor(repo, page.cursor)?;
    Ok(Report {
        cursor: page.cursor,
        full: page.full,
        written,
        commit: sha.map(|s| (s, subject)),
    })
}

/// The paths of every project a page names, staged by project, one batch each.
fn paths(repo: &Path, page: &DumpPage) -> Vec<String> {
    let mut slugs: Vec<&str> = page.projects.iter().map(|p| p.slug.as_str()).collect();
    slugs.extend(page.items.iter().map(|i| i.project.as_str()));
    slugs.extend(page.events.iter().map(|e| e.project.as_str()));
    slugs.sort_unstable();
    slugs.dedup();
    slugs
        .into_iter()
        .flat_map(|slug| {
            [
                project_path(slug),
                events_path(slug),
                format!("{slug}/items"),
            ]
        })
        .filter(|rel| repo.join(rel).exists())
        .collect()
}

#[cfg(test)]
#[path = "tests/run.rs"]
mod tests;
