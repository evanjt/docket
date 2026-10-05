//! What a release closed, grouped by area: one section per area in position order, each headed by
//! its counts by type and followed by a line per closed item.

use std::fmt::Write;

use serde::{Deserialize, Serialize};

use crate::area::Area;
use crate::word::Kind;

/// The area an item without one is listed under, after every named area.
pub const NO_AREA: &str = "no area";

/// One item as the changelog reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub state: String,
    pub kind: Kind,
    pub area: Option<String>,
    /// The title of the plan the item belongs to.
    pub plan: Option<String>,
}

/// One closed item's line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub id: String,
    pub title: String,
    pub plan: Option<String>,
}

/// One area's closed items and their counts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub area: String,
    pub fixed: usize,
    pub added: usize,
    pub lines: Vec<Line>,
}

impl Section {
    /// `lanterns: 2 fixed, 1 added`: a count of none is left out.
    #[must_use]
    pub fn head(&self) -> String {
        let counts: Vec<String> = [(self.fixed, "fixed"), (self.added, "added")]
            .iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, word)| format!("{n} {word}"))
            .collect();
        format!("{}: {}", self.area, counts.join(", "))
    }
}

/// The sections of the closed work in `items`: dropped items, plans and questions are left out.
/// A bug is fixed and any other work is added. Items keep the order they are given in.
#[must_use]
pub fn changelog(items: &[Entry], areas: &[Area]) -> Vec<Section> {
    let mut ordered: Vec<&Area> = areas.iter().collect();
    ordered.sort_by_key(|a| a.position);
    let mut names: Vec<String> = ordered.iter().map(|a| a.name.clone()).collect();
    names.push(NO_AREA.to_string());
    let mut out: Vec<Section> = names
        .into_iter()
        .map(|area| Section {
            area,
            fixed: 0,
            added: 0,
            lines: Vec::new(),
        })
        .collect();
    for e in items
        .iter()
        .filter(|e| e.state == "done" && e.kind == Kind::Work)
    {
        let name = e.area.as_deref().unwrap_or(NO_AREA);
        let Some(s) = out
            .iter_mut()
            .find(|s| s.area.to_lowercase() == name.to_lowercase())
        else {
            continue;
        };
        if e.id.starts_with('B') {
            s.fixed += 1;
        } else {
            s.added += 1;
        }
        s.lines.push(Line {
            id: e.id.clone(),
            title: e.title.clone(),
            plan: e.plan.clone(),
        });
    }
    out.retain(|s| !s.lines.is_empty());
    out
}

/// The sections as markdown: a `##` heading per area and a bullet per item. With `plans`, a line
/// has its plan's title above the tickets under it. With `colour`, headings are bold and ids dim.
#[must_use]
pub fn render(sections: &[Section], plans: bool, colour: bool) -> String {
    let (bold, dim, off) = if colour {
        ("\x1b[1m", "\x1b[2m", "\x1b[0m")
    } else {
        ("", "", "")
    };
    let mut out = String::new();
    for s in sections {
        let _ = write!(out, "{bold}## {}{off}\n\n", s.head());
        let mut shown: Option<&Option<String>> = None;
        for l in &s.lines {
            if plans && shown != Some(&l.plan) {
                if let Some(plan) = &l.plan {
                    let _ = write!(out, "{bold}### {plan}{off}\n\n");
                }
                shown = Some(&l.plan);
            }
            let _ = writeln!(out, "- {} {dim}({}){off}", l.title, l.id);
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[path = "tests/changelog.rs"]
mod tests;
