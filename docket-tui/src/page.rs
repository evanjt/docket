//! The pages a screen can be on, each holding what it read and where its pointer stands, so going back
//! restores both.

use std::collections::BTreeSet;

use docket_core::pace::{Minute, Net, Pace};
use docket_core::rows::{Derived, EventRow, ItemRow, ProjectRow, Row, Shown, Status};

use crate::doc::{Cursor, Listing};

pub enum Page {
    Home(Home),
    Project(Project),
    Browser(Browser),
    Queue(Queue),
    Plans(Plans),
    Settings(Settings),
    Help(Help),
}

impl Page {
    /// What the crumbs call it.
    #[must_use]
    pub fn title(&self) -> String {
        match self {
            Page::Home(_) => "home".into(),
            Page::Project(p) => p.slug.clone(),
            Page::Browser(b) => b.listing.title(),
            Page::Queue(_) => "yours".into(),
            Page::Plans(_) => "plans".into(),
            Page::Settings(_) => "settings".into(),
            Page::Help(_) => "help".into(),
        }
    }

    /// The rows marked on a list page, none on a page with no list.
    #[must_use]
    pub fn marks(&self) -> Option<&BTreeSet<String>> {
        match self {
            Page::Browser(b) => Some(&b.marks),
            Page::Queue(q) => Some(&q.marks),
            Page::Plans(p) => Some(&p.marks),
            _ => None,
        }
    }

    pub fn marks_mut(&mut self) -> Option<&mut BTreeSet<String>> {
        match self {
            Page::Browser(b) => Some(&mut b.marks),
            Page::Queue(q) => Some(&mut q.marks),
            Page::Plans(p) => Some(&mut p.marks),
            _ => None,
        }
    }

    /// The project the page is in, none for home and help.
    #[must_use]
    pub fn slug(&self) -> Option<&str> {
        match self {
            Page::Home(_) | Page::Help(_) => None,
            Page::Project(p) => Some(&p.slug),
            Page::Browser(b) => Some(&b.slug),
            Page::Queue(q) => Some(&q.slug),
            Page::Plans(p) => Some(&p.slug),
            Page::Settings(s) => Some(&s.slug),
        }
    }
}

/// One project on the home page.
#[derive(Clone, Debug, Default)]
pub struct HomeRow {
    pub project: ProjectRow,
    pub status: Option<Status>,
    pub owner: Option<usize>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Home {
    pub rows: Vec<HomeRow>,
    pub cursor: Cursor,
}

/// What a project's page shows beside its board.
#[derive(Clone, Debug, Default)]
pub struct ProjectData {
    pub status: Status,
    pub next: Vec<Row>,
    pub wip: Vec<Row>,
    pub minutes: Vec<Minute>,
    /// The moves shown are the newest ones, outside the window.
    pub stale: bool,
    pub pace: Pace,
    /// The current release's tickets closed against those opened in the last hour.
    pub net: Net,
    pub now: i64,
}

pub struct Project {
    pub slug: String,
    pub data: Option<ProjectData>,
    pub cursor: Cursor,
    /// Every move of the window, not the last few.
    pub all_moves: bool,
}

/// One row of a browser's list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub word: String,
    pub title: String,
    /// Why it stands where it does, or what tied it to the list.
    pub note: String,
}

impl Entry {
    #[must_use]
    pub fn of_row(r: &Row) -> Self {
        let note = r
            .snip
            .clone()
            .or_else(|| {
                standing(
                    r.claim_branch.as_ref(),
                    r.claim_on.as_ref().or(r.claim_host.as_ref()),
                )
            })
            .or_else(|| waits(r.wait_on.as_deref(), r.wait_ref.as_deref()))
            .or_else(|| r.turn_note.clone())
            .unwrap_or_default();
        Self {
            id: r.id.clone(),
            word: r.word.clone(),
            title: r.title.clone(),
            note,
        }
    }

    #[must_use]
    pub fn of_item(i: &ItemRow, word: String) -> Self {
        let note = standing(
            i.claim_branch.as_ref(),
            i.claim_on.as_ref().or(i.claim_host.as_ref()),
        )
        .or_else(|| waits(i.wait_on.as_deref(), i.wait_ref.as_deref()))
        .or_else(|| i.turn_note.clone())
        .unwrap_or_default();
        Self {
            id: i.id.clone(),
            word,
            title: i.title.clone(),
            note,
        }
    }

    #[must_use]
    pub fn of_derived(d: &Derived) -> Self {
        let chose = d.chose.clone().unwrap_or_default();
        Self {
            id: d.id.clone(),
            word: d.state.clone(),
            title: d.title.clone(),
            note: format!("{chose} (from {})", d.basis),
        }
    }
}

fn standing(branch: Option<&String>, host: Option<&String>) -> Option<String> {
    let host = host.map_or("", |h| h.split('.').next().unwrap_or_default());
    branch.map(|b| format!("{b} on {host}"))
}

fn waits(on: Option<&str>, reference: Option<&str>) -> Option<String> {
    match on? {
        "item" => Some(format!("on {}", reference.unwrap_or_default())),
        _ => Some(format!("until {}", reference.unwrap_or_default())),
    }
}

/// One item in full, with its log.
#[derive(Clone, Debug, Default)]
pub struct Detail {
    pub shown: Shown,
    pub log: Vec<EventRow>,
}

/// The search being typed, and the list to go back to when it is cancelled.
#[derive(Clone, Debug)]
pub struct Typing {
    pub text: String,
    pub before: Listing,
}

pub struct Browser {
    pub slug: String,
    pub listing: Listing,
    pub entries: Vec<Entry>,
    pub sel: usize,
    pub list_top: usize,
    pub detail: Option<Detail>,
    pub cursor: Cursor,
    pub typing: Option<Typing>,
    /// The rows picked for a move on several at once.
    pub marks: BTreeSet<String>,
}

impl Browser {
    #[must_use]
    pub fn new(slug: &str, listing: Listing) -> Self {
        Self {
            slug: slug.to_string(),
            listing,
            entries: Vec::new(),
            sel: 0,
            list_top: 0,
            detail: None,
            cursor: Cursor::default(),
            typing: None,
            marks: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn selected(&self) -> Option<&Entry> {
        self.entries.get(self.sel)
    }
}

pub struct Queue {
    pub slug: String,
    pub rows: Vec<Row>,
    pub at: usize,
    pub cursor: Cursor,
    pub marks: BTreeSet<String>,
}

/// One row of the plans tree: a section heading, or an item at a depth.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlanRow {
    pub heading: Option<String>,
    pub id: String,
    pub depth: usize,
    pub word: String,
    pub title: String,
    pub done: u64,
    pub total: u64,
    pub live: u64,
    pub folded: Option<bool>,
}

pub struct Plans {
    pub slug: String,
    pub rows: Vec<PlanRow>,
    pub open: BTreeSet<String>,
    pub sel: usize,
    pub list_top: usize,
    pub shown: Option<Shown>,
    pub next: Vec<Row>,
    pub cursor: Cursor,
    pub marks: BTreeSet<String>,
}

pub struct Settings {
    pub slug: String,
    pub cursor: Cursor,
    /// The fact whose last value the server refused, and its words.
    pub refused: Option<(String, String)>,
}

#[derive(Default)]
pub struct Help {
    pub cursor: Cursor,
}
