//! Which project a checkout belongs to: its slug from an origin URL, and the projects it may match.

use std::sync::LazyLock;

use regex::Regex;

use crate::item::Refused;
use crate::word::ItemType;

static URL_SLUG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[:/]([^/:]+)/([^/]+?)(?:\.git)?/?$").unwrap());

/// `owner/name` from a git remote URL, or `None` when it carries no such pair.
#[must_use]
pub fn slug_from_url(url: &str) -> Option<String> {
    let m = URL_SLUG.captures(url)?;
    Some(format!("{}/{}", &m[1], &m[2]))
}

static SLUG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9_][A-Za-z0-9._-]*(/[A-Za-z0-9_][A-Za-z0-9._-]*)?$").unwrap()
});

/// A slug a project may be given by hand: one or two words of letters, digits, `.`, `_` and `-`,
/// joined by one `/`, neither word starting with a dot.
///
/// # Errors
/// Refused for anything else, naming the shape.
pub fn slug_valid(slug: &str) -> Result<(), Refused> {
    if SLUG.is_match(slug) {
        return Ok(());
    }
    Err(Refused(format!(
        "a project slug is NAME or OWNER/NAME, words of letters, digits, '.', '_' and '-' that do not \
         start with a dot, not {}",
        crate::text::quoted(slug)
    )))
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

/// The type a new item filed under the key takes.
///
/// # Errors
/// Refused when the key is not one of the five docket fixes: a key that holds stored items keeps them
/// and takes no new ones.
pub fn require_fileable(key: &str) -> Result<ItemType, Refused> {
    ItemType::filed_under(key).ok_or_else(|| {
        let keys: Vec<&str> = ItemType::ALL.iter().map(|t| t.key()).collect();
        Refused(format!(
            "{key} is not one of docket's keys. File under {}.",
            keys.join(", ")
        ))
    })
}

#[cfg(test)]
#[path = "tests/project.rs"]
mod tests;
