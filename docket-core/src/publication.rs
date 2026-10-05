//! A publication: one commit a squash wrote to a project's published ref, the work snapshot whose
//! tree it carries, and the plans it covers. The server keeps them as rows; nothing about them is
//! written into a commit.

use serde::{Deserialize, Serialize};

use crate::item::Refused;
use crate::text::py_repr;

/// One publication as `GET /publications` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Publication {
    /// The commit on the published ref.
    pub published: String,
    /// The first-parent snapshot of the work ref whose tree the published commit carries.
    pub work: String,
    /// The plans whose landings it covers, by id.
    pub plans: Vec<String>,
    pub created_at: String,
}

/// Refuses anything but a full commit sha, 40 or 64 lowercase hex digits: a short sha can come to
/// name two commits.
///
/// # Errors
/// `sha` is not a full lowercase sha; `what` names it in the refusal.
pub fn check_sha(what: &str, sha: &str) -> Result<(), Refused> {
    let hex = sha
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if hex && (sha.len() == 40 || sha.len() == 64) {
        return Ok(());
    }
    Err(Refused(format!(
        "the {what} commit is a full sha of 40 or 64 lowercase hex digits, not {}",
        py_repr(sha)
    )))
}

/// The first-parent landings on the work ref past the last publication after which a squash is
/// offered.
pub const LANDINGS_BEFORE_SQUASH: usize = 30;

/// The publication whose work snapshot is the first to contain a commit: `publications` newest
/// first, `contains` whether a work snapshot has the commit in its history.
#[must_use]
pub fn first_containing(
    publications: &[Publication],
    contains: impl Fn(&str) -> bool,
) -> Option<&Publication> {
    publications.iter().rev().find(|p| contains(&p.work))
}

/// Why a squash is due, or `None`: a plan finished that no publication covers, or the work ref is
/// `landings` first-parent landings past the last publication, at least `cap` of them. With no
/// landing there is nothing to publish.
#[must_use]
pub fn squash_due(unpublished: &[String], landings: usize, cap: usize) -> Option<String> {
    if landings == 0 {
        return None;
    }
    if !unpublished.is_empty() {
        return Some(format!(
            "{} closed after the last publication",
            unpublished.join(", ")
        ));
    }
    (landings >= cap).then(|| format!("{landings} landings past the last publication"))
}

/// The title of the owner ask to push a published ref.
#[must_use]
pub fn push_title(local: &str, remote: &str) -> String {
    format!("Push {local} to {remote}")
}

/// The command that pushes the published ref to its remote ref.
#[must_use]
pub fn push_command(local: &str, remote: &str) -> String {
    let (name, branch) = remote.split_once('/').unwrap_or((remote, ""));
    format!("git push {name} {local}:{branch}")
}

#[cfg(test)]
#[path = "tests/publication.rs"]
mod tests;
