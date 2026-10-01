//! The files a ticket says it touches, and when two citations name one file.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

static DECLARED_FILES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\*\*Touches\.\*\*\s*(.*)$").unwrap());
static DECLARED_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r":\d+(?:-\d+)?$").unwrap());
static FILE_EXTENSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.[A-Za-z][A-Za-z0-9_+-]*$").unwrap());

/// One entry per file: a citation by bare name or short path folds into the longer path it ends.
pub fn fold_paths(paths: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    let paths: BTreeSet<String> = paths.into_iter().collect();
    paths
        .iter()
        .filter(|p| {
            !paths
                .iter()
                .any(|q| q != *p && q.ends_with(&format!("/{p}")))
        })
        .cloned()
        .collect()
}

/// Whether two citations name one file. A shorter one counts only when it carries a directory.
#[must_use]
pub fn same_file(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short.contains('/') && long.ends_with(&format!("/{short}"))
}

/// The paths a body's Touches line declares, folded and stripped of line numbers.
#[must_use]
pub fn declared_files(body: &str) -> BTreeSet<String> {
    let mut found = Vec::new();
    for line in body.lines() {
        let Some(m) = DECLARED_FILES.captures(line) else {
            continue;
        };
        for part in m[1].split(',') {
            let path = part.trim().trim_start_matches('`');
            let path = path.trim_end_matches(['`', '.', ';', ',']);
            let path = DECLARED_LINE.replace(path, "").to_string();
            let lower = path.to_lowercase();
            if lower == "none" || lower == "n/a" || path.chars().any(char::is_whitespace) {
                continue;
            }
            if path.contains('/') || FILE_EXTENSION.is_match(&path) {
                found.push(path);
            }
        }
    }
    fold_paths(found)
}

#[cfg(test)]
#[path = "tests/touch.rs"]
mod tests;
