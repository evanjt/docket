//! A publication: one commit a squash wrote to a project's published ref, the work snapshot whose
//! tree it carries, and the plans it covers. The server keeps them as rows; nothing about them is
//! written into a commit.

use serde::{Deserialize, Serialize};

use crate::item::Refused;
use crate::text::quoted;

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
        quoted(sha)
    )))
}

/// The age in seconds of the oldest unpublished landing from which a squash is offered.
pub const SQUASH_AFTER_SECONDS: i64 = 24 * 3600;

/// The publication whose work snapshot is the first to contain a commit: `publications` newest
/// first, `contains` whether a work snapshot has the commit in its history.
#[must_use]
pub fn first_containing(
    publications: &[Publication],
    contains: impl Fn(&str) -> bool,
) -> Option<&Publication> {
    publications.iter().rev().find(|p| contains(&p.work))
}

/// Why a squash is due, or `None`: a plan finished that no publication covers, or the oldest
/// unpublished landing is at least `SQUASH_AFTER_SECONDS` old. `oldest` is that landing's commit
/// time and `now` the present, both in Unix seconds. With no landing there is nothing to publish.
#[must_use]
pub fn squash_due(unpublished: &[String], oldest: Option<i64>, now: i64) -> Option<String> {
    let age = now - oldest?;
    if !unpublished.is_empty() {
        return Some(format!(
            "{} closed after the last publication",
            unpublished.join(", ")
        ));
    }
    (age >= SQUASH_AFTER_SECONDS).then(|| {
        format!(
            "the oldest landing past the last publication is {} hours old",
            age / 3600
        )
    })
}

/// Where the next squash builds from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartOn {
    /// The local published ref, which the last publication names; the work commit it covers.
    Local { work: String },
    /// The remote published ref.
    Remote,
}

/// Where a squash starts. A local published ref the last publication names starts on it. One that
/// no publication names starts on the remote ref when the remote holds it (`remote_holds_local`),
/// since nothing on it is unpublished; one with commits the remote lacks is refused. With no local
/// ref the squash starts on the remote ref.
///
/// # Errors
/// The local ref holds commits no publication names and the remote lacks, or neither ref exists.
pub fn start_on(
    local: Option<&str>,
    remote: Option<&str>,
    remote_holds_local: bool,
    last: Option<&Publication>,
    names: (&str, &str),
) -> Result<StartOn, Refused> {
    let (local_name, remote_name) = names;
    match (local, remote) {
        (Some(l), _) if last.is_some_and(|p| p.published == l) => Ok(StartOn::Local {
            work: last.map(|p| p.work.clone()).unwrap_or_default(),
        }),
        (Some(_), Some(r)) if remote_holds_local || local == Some(r) => Ok(StartOn::Remote),
        (Some(l), _) => Err(Refused(format!(
            "{local_name} is at {l}, which no recorded publication names and {remote_name} does not hold: nothing written"
        ))),
        (None, Some(_)) => Ok(StartOn::Remote),
        (None, None) => Err(Refused(format!(
            "neither {local_name} nor {remote_name} exists here to build on: fetch the remote first"
        ))),
    }
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
