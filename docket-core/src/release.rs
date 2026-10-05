//! A project's releases: rows with a name, a position, a target date and a shipped time. Order comes
//! from the position, and every write that names a release names one that exists.

use serde::{Deserialize, Serialize};

use crate::item::Refused;
use crate::migrate::version;
use crate::text::py_repr;

/// The one alias a write may give for a release: the first not shipped.
pub const CURRENT: &str = "current";

/// One release as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub name: String,
    pub position: i64,
    pub target_date: Option<String>,
    pub shipped_at: Option<String>,
    pub note: Option<String>,
}

/// A release as `/releases` lists it: the row's id and the release.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    pub id: i64,
    #[serde(flatten)]
    pub release: Release,
}

/// A project's releases with their row ids, for reading an item's release from its key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listed {
    pub rows: Vec<(i64, Release)>,
    /// The names not yet shipped, in position order; the first is the current release.
    pub open: Vec<String>,
}

impl Listed {
    #[must_use]
    pub fn new(rows: Vec<(i64, Release)>) -> Self {
        let all: Vec<Release> = rows.iter().map(|(_, r)| r.clone()).collect();
        Listed {
            open: unshipped(&all),
            rows,
        }
    }

    #[must_use]
    pub fn all(&self) -> Vec<Release> {
        self.rows.iter().map(|(_, r)| r.clone()).collect()
    }

    /// The name of the release a row id names; none for the backlog.
    #[must_use]
    pub fn name(&self, id: Option<i64>) -> Option<&str> {
        let id = id?;
        self.rows
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, r)| r.name.as_str())
    }

    #[must_use]
    pub fn id(&self, name: &str) -> Option<i64> {
        self.rows
            .iter()
            .find(|(_, r)| r.name == name)
            .map(|(i, _)| *i)
    }
}

/// The releases not yet shipped, by name in position order. The first is the current release.
#[must_use]
pub fn unshipped(all: &[Release]) -> Vec<String> {
    let mut open: Vec<&Release> = all.iter().filter(|r| r.shipped_at.is_none()).collect();
    open.sort_by_key(|r| r.position);
    open.into_iter().map(|r| r.name.clone()).collect()
}

/// Refuses a name a release cannot take: empty, with whitespace, the alias, or not a version.
///
/// # Errors
/// The name is refused, with the reason.
pub fn check_name(name: &str) -> Result<(), Refused> {
    if name.is_empty() || name.chars().any(char::is_whitespace) {
        return Err(Refused(format!(
            "{} is not a release name: one word, no spaces",
            py_repr(name)
        )));
    }
    if name == CURRENT {
        return Err(Refused(format!(
            "{CURRENT} names the first release not shipped, so no release may take it"
        )));
    }
    if version(name).is_none() {
        return Err(Refused(format!(
            "{name} is not a version: a release is named as numbers joined by dots, such as 1.0.0"
        )));
    }
    Ok(())
}

/// The position a new release takes among `all`: after every release with a lower version, the
/// ones after it moved up one. Refuses a name already taken or one that is no version.
///
/// # Errors
/// The name is refused or already a release.
pub fn place(all: &[Release], name: &str) -> Result<i64, Refused> {
    check_name(name)?;
    if all.iter().any(|r| r.name == name) {
        return Err(Refused(format!("{name} is already a release")));
    }
    let mine = version(name);
    let before = all.iter().filter(|r| version(&r.name) < mine).count();
    Ok(i64::try_from(before).unwrap_or(i64::MAX))
}

/// The release a write names: `current` is the first not shipped, any other name a release not yet
/// shipped. An empty name is the backlog.
///
/// # Errors
/// The name is no release here, has shipped, or is `current` while every release has shipped.
pub fn resolve(all: &[Release], given: &str) -> Result<Option<String>, Refused> {
    let given = given.trim();
    if given.is_empty() {
        return Ok(None);
    }
    let open = unshipped(all);
    let choices = || {
        if open.is_empty() {
            "none is open: add one with docket releases add".to_string()
        } else {
            format!("give {CURRENT} or one of {}", open.join(" "))
        }
    };
    if given == CURRENT {
        return open
            .first()
            .cloned()
            .map(Some)
            .ok_or_else(|| Refused(format!("there is no current release: {}", choices())));
    }
    match all.iter().find(|r| r.name == given) {
        Some(r) if r.shipped_at.is_some() => {
            Err(Refused(format!("{given} has shipped: {}", choices())))
        }
        Some(r) => Ok(Some(r.name.clone())),
        None => Err(Refused(format!(
            "{given} is not a release here: {}",
            choices()
        ))),
    }
}

/// Why a release cannot ship while `open` items remain in it and none are moved on.
#[must_use]
pub fn ship_refusal(name: &str, open: &[String]) -> Option<String> {
    (!open.is_empty()).then(|| {
        format!(
            "{name} still holds {} open: {}. Close them, or pass --move-open-to a later release.",
            open.len(),
            open.join(", ")
        )
    })
}

#[cfg(test)]
#[path = "tests/release.rs"]
mod tests;
