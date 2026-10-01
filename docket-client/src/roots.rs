//! The directories bound to a project on this machine: `roots` beside the client file, one line each.

use std::fs;
use std::path::{Path, PathBuf};

/// One directory bound to a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    pub path: String,
    pub project: String,
    /// `auto` when a command bound the checkout it ran in, `bind` when the person did.
    pub how: String,
    pub bound_at: String,
}

/// The roots file: `path<TAB>project<TAB>how<TAB>bound_at` per line.
pub struct Roots {
    file: PathBuf,
    pub roots: Vec<Root>,
}

/// `roots` in the directory of the client file.
#[must_use]
pub fn path() -> Option<PathBuf> {
    Some(crate::config::path()?.with_file_name("roots"))
}

impl Roots {
    /// The roots a file lists; none when it is missing.
    #[must_use]
    pub fn load(file: PathBuf) -> Self {
        let text = fs::read_to_string(&file).unwrap_or_default();
        Self {
            file,
            roots: parse(&text),
        }
    }

    /// The project of the longest root holding a real path.
    #[must_use]
    pub fn bound(&self, real: &str) -> Option<String> {
        bound(&self.roots, real)
    }

    /// One project's roots, shortest first.
    #[must_use]
    pub fn of(&self, project: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .roots
            .iter()
            .filter(|r| r.project == project)
            .map(|r| r.path.clone())
            .collect();
        out.sort_by_key(String::len);
        out
    }

    /// A directory bound to a project, in place of what it was bound to.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn bind(&mut self, root: Root) -> std::io::Result<()> {
        self.roots.retain(|r| r.path != root.path);
        self.roots.push(root);
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&self.file, render(&self.roots))
    }

    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }
}

/// The roots a file's text lists; a line that is not four fields is skipped.
#[must_use]
pub fn parse(text: &str) -> Vec<Root> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some(Root {
                path: f.next()?.to_string(),
                project: f.next()?.to_string(),
                how: f.next()?.to_string(),
                bound_at: f.next()?.to_string(),
            })
        })
        .filter(|r| !r.path.is_empty() && !r.project.is_empty())
        .collect()
}

#[must_use]
pub fn render(roots: &[Root]) -> String {
    let mut out = String::new();
    for r in roots {
        out.push_str(&[r.path.as_str(), &r.project, &r.how, &r.bound_at].join("\t"));
        out.push('\n');
    }
    out
}

/// The project of the longest root that is the path or holds it.
#[must_use]
pub fn bound(roots: &[Root], real: &str) -> Option<String> {
    let mut best: Option<(&str, &str)> = None;
    for r in roots {
        let p = r.path.trim_end_matches('/');
        let holds = real == p || real.starts_with(&format!("{p}/"));
        if holds && best.is_none_or(|(b, _)| p.len() > b.len()) {
            best = Some((p, &r.project));
        }
    }
    best.map(|(_, project)| project.to_string())
}

#[cfg(test)]
#[path = "tests/roots.rs"]
mod tests;
