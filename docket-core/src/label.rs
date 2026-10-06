//! A project's labels: a name and a description. An item carries a label it was given, and every item
//! under it inherits the label when it is read.

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
