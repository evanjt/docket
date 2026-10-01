//! Which project a checkout belongs to: its slug from an origin URL, and the projects it may match.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};

static URL_SLUG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[:/]([^/:]+)/([^/]+?)(?:\.git)?/?$").unwrap());

/// `owner/name` from a git remote URL, or `None` when it carries no such pair.
#[must_use]
pub fn slug_from_url(url: &str) -> Option<String> {
    let m = URL_SLUG.captures(url)?;
    Some(format!("{}/{}", &m[1], &m[2]))
}

/// The projects a checkout may be: the slug its remote names, a project sharing one of its remotes, or
/// one whose slug ends in the checkout's directory name. Sorted, each once.
#[must_use]
pub fn matches(
    projects: &[(String, Vec<String>)],
    candidate: Option<&str>,
    urls: &[String],
    basename: &str,
) -> Vec<String> {
    let mut out: Vec<String> = projects
        .iter()
        .filter(|(slug, remotes)| {
            candidate == Some(slug.as_str())
                || urls.iter().any(|u| remotes.contains(u))
                || slug.rsplit('/').next() == Some(basename)
        })
        .map(|(slug, _)| slug.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The keys a new project opens with.
#[must_use]
pub fn default_keys() -> Value {
    json!([
        {"key": "T", "kind": "work", "meaning": "tasks", "turn": "agent"},
        {"key": "B", "kind": "work", "meaning": "bugs", "turn": "agent"},
        {"key": "Q", "kind": "decision", "meaning": "questions, a decision not a commit", "turn": "user"},
        {"key": "I", "kind": "research", "meaning": "investigations, measured before decided", "turn": "agent"},
        {"key": "A", "kind": "audit", "meaning": "audits, a plan checked against the tree once all it opened is closed",
         "turn": "agent"},
        {"key": "STY", "kind": "story", "meaning": "user stories, accepted by the owner once all they opened is closed",
         "turn": "agent"},
        {"key": "CON", "kind": "concept", "meaning": "concepts, the domain areas every item belongs to", "turn": "agent"},
        {"key": "CID", "kind": "idea", "meaning": "central ideas, the rules every plan is held to", "turn": "agent"},
        {"key": "PK", "kind": "package", "meaning": "packages: one fact given one owner, its symptoms held under it",
         "turn": "agent"}
    ])
}

#[cfg(test)]
#[path = "tests/project.rs"]
mod tests;
