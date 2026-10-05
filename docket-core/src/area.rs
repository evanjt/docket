//! A project's areas: rows with a name, a description, a position and a priority. An item under a
//! plan is in its plan's area, and every write that names an area names one that exists.

use serde::{Deserialize, Serialize};

use crate::item::Refused;

/// One area as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Area {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub position: i64,
    /// One of the four priority words, or none.
    #[serde(default)]
    pub priority: Option<String>,
    /// Holds closed items only: no open or new item is put in it. `areas add` and `areas edit` never set it.
    #[serde(default)]
    pub history: bool,
}

/// An area as `/areas` lists it: the row's id and the area.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    pub id: i64,
    #[serde(flatten)]
    pub area: Area,
}

/// A project's areas with their row ids, in position order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listed {
    pub rows: Vec<(i64, Area)>,
}

impl Listed {
    #[must_use]
    pub fn all(&self) -> Vec<Area> {
        self.rows.iter().map(|(_, a)| a.clone()).collect()
    }

    /// The name of the area a row id names.
    #[must_use]
    pub fn name(&self, id: Option<i64>) -> Option<&str> {
        let id = id?;
        self.rows
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, a)| a.name.as_str())
    }

    /// The row id of an area by its name, ignoring case.
    #[must_use]
    pub fn id(&self, name: &str) -> Option<i64> {
        self.rows
            .iter()
            .find(|(_, a)| same(&a.name, name))
            .map(|(i, _)| *i)
    }
}

fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.trim().to_lowercase()
}

fn find<'a>(all: &'a [Area], name: &str) -> Option<&'a Area> {
    all.iter().find(|a| same(&a.name, name))
}

/// Refuses a name an area cannot take: empty, or another area's ignoring case. `renaming` is the
/// area the name is for when it already exists, which may take its own name in another case.
///
/// # Errors
/// The name is empty or taken.
pub fn check_name(all: &[Area], name: &str, renaming: Option<&str>) -> Result<(), Refused> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Refused("an area needs a name".to_string()));
    }
    match find(all, name) {
        Some(a) if !renaming.is_some_and(|r| same(&a.name, r)) => {
            Err(Refused(format!("{name} is already an area: {}", a.name)))
        }
        _ => Ok(()),
    }
}

/// An area's priority: one of the four words, or none for an empty one.
///
/// # Errors
/// The word is not one of the four.
pub fn check_priority(given: &str) -> Result<Option<String>, Refused> {
    let given = given.trim();
    if given.is_empty() {
        return Ok(None);
    }
    if crate::word::PRIORITIES.contains(&given) {
        return Ok(Some(given.to_string()));
    }
    Err(Refused(format!(
        "{given} is not a priority: give one of {}, or \"\" for none",
        crate::word::PRIORITIES.join(", ")
    )))
}

/// The area a write names, by its stored name, found ignoring case. An empty name is none.
///
/// # Errors
/// The name is no area here, refused with the project's areas.
pub fn resolve(all: &[Area], given: &str) -> Result<Option<String>, Refused> {
    let given = given.trim();
    if given.is_empty() {
        return Ok(None);
    }
    if let Some(a) = find(all, given) {
        return Ok(Some(a.name.clone()));
    }
    let mut ordered: Vec<&Area> = all.iter().collect();
    ordered.sort_by_key(|a| a.position);
    let names: Vec<&str> = ordered.iter().map(|a| a.name.as_str()).collect();
    Err(Refused(if names.is_empty() {
        format!("{given} is not an area here: there are none, add one with docket areas add")
    } else {
        format!(
            "{given} is not an area here: give one of {}",
            names.join(", ")
        )
    }))
}

/// Refuses an area an open item cannot be put in: one marked `history`, which holds closed items
/// only. The refusal lists the project's other areas.
///
/// # Errors
/// The area is a history area.
pub fn open_into(all: &[Area], name: &str) -> Result<(), Refused> {
    let Some(a) = find(all, name).filter(|a| a.history) else {
        return Ok(());
    };
    let mut others: Vec<&Area> = all.iter().filter(|o| !o.history).collect();
    others.sort_by_key(|o| o.position);
    let names: Vec<&str> = others.iter().map(|o| o.name.as_str()).collect();
    Err(Refused(format!(
        "{} holds closed items only: put an open item in one of {}",
        a.name,
        names.join(", ")
    )))
}

/// The names in their order once `name` is moved to place `to`, counted from one; a place past the
/// end is the last.
///
/// # Errors
/// The name is no area here, or the place is zero.
pub fn reorder(all: &[Area], name: &str, to: i64) -> Result<Vec<String>, Refused> {
    if to < 1 {
        return Err(Refused(format!("{to} is not a place: the first area is 1")));
    }
    let Some(moving) = find(all, name) else {
        return Err(resolve(all, name)
            .err()
            .unwrap_or_else(|| Refused(format!("{name} is not an area here"))));
    };
    let mut ordered: Vec<&Area> = all.iter().filter(|a| a.name != moving.name).collect();
    ordered.sort_by_key(|a| a.position);
    let at = usize::try_from(to - 1)
        .unwrap_or(usize::MAX)
        .min(ordered.len());
    ordered.insert(at, moving);
    Ok(ordered.into_iter().map(|a| a.name.clone()).collect())
}

/// Why an area cannot be removed while `carried` items carry it.
#[must_use]
pub fn rm_refusal(name: &str, carried: &[String]) -> Option<String> {
    (!carried.is_empty()).then(|| {
        format!(
            "{name} is carried by {} items: {}. Move them to another area first.",
            carried.len(),
            carried.join(", ")
        )
    })
}

/// Where an agent placed an item no rule could: the area it named and what the area is about, read
/// from the data of a `decided` event. The event is the input a project takes its areas from
/// before the area rows exist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub area: String,
    #[serde(default)]
    pub about: Option<String>,
}

impl Placement {
    /// The placement a `decided` event's data holds; none when it holds only a basis.
    #[must_use]
    pub fn of(data: &str) -> Option<Self> {
        serde_json::from_str::<Self>(data).ok()
    }
}

/// The area name a placement gives: lower case, spaces as hyphens. `unsorted` holds closed items
/// that have no area, and no open item is placed there.
///
/// # Errors
/// The name is empty or `unsorted`.
pub fn placement_name(given: &str) -> Result<String, Refused> {
    let name = given
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    if name.is_empty() {
        return Err(Refused("an area needs a name".to_string()));
    }
    if name == "unsorted" {
        return Err(Refused(
            "unsorted is where closed items without an area go: place an open item in an area it belongs to"
                .to_string(),
        ));
    }
    Ok(name)
}

#[cfg(test)]
#[path = "tests/area.rs"]
mod tests;
