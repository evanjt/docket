//! Starting a lead for a project that has none: on each refresh the screen asks, for every project
//! bound to a root on this machine, whether `lead::should_start` allows one, and launches it through
//! the function it is given. Nothing here starts a process itself.

use std::collections::{BTreeMap, HashMap};

use docket_core::fact::{self, Model, Role};
use docket_core::lead;

use crate::source::Source;

/// What a launch is given: the project, its roots on this machine, its facts and the lead's model.
pub struct Start {
    pub slug: String,
    pub roots: Vec<String>,
    pub skills: BTreeMap<String, String>,
    pub model: Model,
}

/// Starts a lead, answering with a line saying what started or why it could not.
pub type Launch<'a> = &'a dyn Fn(&Start) -> Result<String, String>;

/// What the last look at a project came to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// When the last start was attempted, successful or not.
    pub at: Option<i64>,
    /// How the last attempt went.
    pub outcome: Option<Result<String, String>>,
    /// Why no lead was started at the last look; empty when one was or the window was still shut.
    pub why: Vec<String>,
}

/// One record per project, kept for as long as the screen is open.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Starts(HashMap<String, Record>);

impl Starts {
    #[must_use]
    pub fn of(&self, slug: &str) -> Option<&Record> {
        self.0.get(slug)
    }
}

/// One refresh: every project with a root here that a lead may be started for gets one, at most one
/// attempt per lapse window. A read that fails leaves the project alone until the next refresh.
pub fn tick<S: Source>(
    source: &S,
    roots_of: &dyn Fn(&str) -> Vec<String>,
    starts: &mut Starts,
    now: i64,
    launch: Launch,
) {
    let Ok(projects) = source.projects() else {
        return;
    };
    for project in projects {
        let roots = roots_of(&project.slug);
        if roots.is_empty() {
            continue;
        }
        let record = starts.0.entry(project.slug.clone()).or_default();
        if !lead::attempt_due(record.at, now, &project.skills) {
            continue;
        }
        let Ok(state) = source.lead(&project.slug) else {
            continue;
        };
        if let Err(why) = lead::should_start(state.lead.as_ref(), &project.skills, now) {
            record.why = why;
            continue;
        }
        record.why.clear();
        record.at = Some(now);
        record.outcome = Some(launch_for(&project, roots, launch));
    }
}

fn launch_for(
    project: &docket_core::rows::ProjectRow,
    roots: Vec<String>,
    launch: Launch,
) -> Result<String, String> {
    let model = fact::model_for(&project.skills, &BTreeMap::new(), Role::Lead, None)
        .ok_or("models has no lead or high entry")?;
    launch(&Start {
        slug: project.slug.clone(),
        roots,
        skills: project.skills.clone(),
        model,
    })
}

#[cfg(test)]
#[path = "tests/starter.rs"]
mod tests;
