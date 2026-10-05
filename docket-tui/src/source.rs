//! Where the screen reads from: the server through its routes, or fixed rows in a test.

use std::collections::HashSet;

use docket_client::{Api, Error};
use docket_core::api::{LeadState, Machines};
use docket_core::member::{Edge, Tie};
use docket_core::release::Listed;
use docket_core::rows::{Derived, EventRow, ProjectRow, Row, Shown, Status};
use serde_json::Value;

use docket_core::board::Board;

use crate::filter::Filter;

pub type Result<T> = std::result::Result<T, String>;

/// The event kinds that move an item, which the moves and the pace read.
pub const MOVE_KINDS: [&str; 13] = [
    "opened",
    "reopened",
    "closed",
    "dropped",
    "claimed",
    "released",
    "claim_lost",
    "asked",
    "replied",
    "decided",
    "waited",
    "resumed",
    "legacy",
];

/// How many of a project's newest moves the screen reads.
pub const RECENT: usize = 400;

pub trait Source: Sync {
    /// # Errors
    /// The words of whatever refused or failed.
    fn projects(&self) -> Result<Vec<ProjectRow>>;
    /// # Errors
    /// As `projects`.
    fn status(&self, slug: &str) -> Result<Status>;
    /// # Errors
    /// As `projects`.
    fn next(&self, slug: &str, n: usize, filter: &Filter) -> Result<Vec<Row>>;
    /// A list route by name: `todo`, `questions`, `groups`, `wip` and the rest.
    ///
    /// # Errors
    /// As `projects`.
    fn list(&self, route: &str, slug: &str) -> Result<Vec<Row>>;
    /// # Errors
    /// As `projects`.
    fn group(&self, slug: &str, name: &str) -> Result<Vec<Row>>;
    /// # Errors
    /// As `projects`.
    fn derived(&self, slug: &str) -> Result<Vec<Derived>>;
    /// # Errors
    /// As `projects`.
    fn search(&self, slug: &str, words: &str) -> Result<Vec<Row>>;
    /// # Errors
    /// As `projects`.
    fn show(&self, slug: &str, id: &str) -> Result<Shown>;
    /// # Errors
    /// As `projects`.
    fn board(&self, slug: &str) -> Result<Board>;
    /// One item's events, oldest first.
    ///
    /// # Errors
    /// As `projects`.
    fn log(&self, rid: i64) -> Result<Vec<EventRow>>;
    /// A project's newest moves, newest first.
    ///
    /// # Errors
    /// As `projects`.
    fn recent(&self, slug: &str) -> Result<Vec<EventRow>>;
    /// Who leads a project.
    ///
    /// # Errors
    /// As `projects`.
    fn lead(&self, slug: &str) -> Result<LeadState>;
    /// Every machine jobs run on, with its slots.
    ///
    /// # Errors
    /// As `projects`.
    fn machines(&self) -> Result<Machines>;
    /// A write verb, `POST /do/{verb}`, and the server's answer.
    ///
    /// # Errors
    /// The server's refusal in its own words, or why it could not be reached.
    fn post(&self, verb: &str, body: &Value) -> Result<Value>;
}

pub struct Http(pub Api);

impl Source for Http {
    fn projects(&self) -> Result<Vec<ProjectRow>> {
        self.0.projects().map_err(|e| e.to_string())
    }

    fn status(&self, slug: &str) -> Result<Status> {
        self.0.status(slug).map_err(|e| e.to_string())
    }

    fn next(&self, slug: &str, n: usize, filter: &Filter) -> Result<Vec<Row>> {
        self.0
            .next(slug, n, &filter.query())
            .map_err(|e| e.to_string())
    }

    fn list(&self, route: &str, slug: &str) -> Result<Vec<Row>> {
        self.0.list(route, slug).map_err(|e| e.to_string())
    }

    fn group(&self, slug: &str, name: &str) -> Result<Vec<Row>> {
        self.0.group(slug, name).map_err(|e| e.to_string())
    }

    fn derived(&self, slug: &str) -> Result<Vec<Derived>> {
        self.0.derived(slug).map_err(|e| e.to_string())
    }

    fn search(&self, slug: &str, words: &str) -> Result<Vec<Row>> {
        self.0.search(slug, words, 200).map_err(|e| e.to_string())
    }

    fn show(&self, slug: &str, id: &str) -> Result<Shown> {
        self.0.show(slug, id).map_err(|e| e.to_string())
    }

    fn board(&self, slug: &str) -> Result<Board> {
        board(&self.0, slug).map_err(|e| e.to_string())
    }

    fn log(&self, rid: i64) -> Result<Vec<EventRow>> {
        self.0.log(rid).map_err(|e| e.to_string())
    }

    fn recent(&self, slug: &str) -> Result<Vec<EventRow>> {
        self.0
            .recent(slug, &MOVE_KINDS, RECENT)
            .map_err(|e| e.to_string())
    }

    fn lead(&self, slug: &str) -> Result<LeadState> {
        self.0.lead(slug).map_err(|e| e.to_string())
    }

    fn machines(&self) -> Result<Machines> {
        self.0.machines().map_err(|e| e.to_string())
    }

    fn post(&self, verb: &str, body: &Value) -> Result<Value> {
        self.0.post(verb, body).map_err(|e| match e {
            Error::Refused(_, why) | Error::Failed(why) | Error::Unreadable(why) => why,
        })
    }
}

/// A project's board from the stored lists: its row, its items with their parents, every `origin`
/// tie leaving them, and the `related` ties reaching or leaving a standing item, which make a
/// concept's members.
fn board(api: &Api, slug: &str) -> docket_client::api::Result<Board> {
    let mut project = api
        .projects()?
        .into_iter()
        .find(|p| p.slug == slug)
        .ok_or_else(|| docket_client::Error::Refused(404, format!("no project {slug}")))?;
    let rows = api.releases(slug, true)?;
    project.releases = Listed::new(rows.into_iter().map(|r| (r.id, r.release)).collect());
    let items = api.items(slug)?;
    let rids: Vec<i64> = items.iter().map(|i| i.rid).collect();
    let standing: Vec<i64> = items
        .iter()
        .filter(|i| project.kind(&i.key).is_standing())
        .map(|i| i.rid)
        .collect();
    let mut links = api.links_from(&rids, "origin")?;
    links.extend(api.links_from(&standing, "related")?);
    links.extend(api.links_to(&standing, "related")?);
    let mut seen = HashSet::new();
    let ties = links
        .into_iter()
        .filter_map(|l| {
            Some(Tie {
                rid: l.rid,
                edge: Edge::parse(&l.kind)?,
                to: l.to_rid?,
            })
        })
        .filter(|t| seen.insert((t.rid, t.to, t.edge == Edge::Origin)))
        .collect();
    Ok(Board::new(project, items, ties))
}
