//! Every project on one page: its open tickets, what waits on the owner, and its flow.

use crate::doc::{Doc, Seg, Target, seg, spot};
use crate::page::HomeRow;
use crate::style;

/// The flow's words in the order the columns read.
pub const WORDS: [&str; 6] = [
    "ready",
    "in progress",
    "under way",
    "audit due",
    "blocked",
    "done",
];

#[must_use]
pub fn doc(rows: &[HomeRow], width: usize) -> Doc {
    let name = rows
        .iter()
        .map(|r| r.project.slug.len())
        .max()
        .unwrap_or(8)
        .clamp(8, (width / 3).max(16))
        + 2;
    let mut d = Doc::default();
    let mut head = vec![seg(
        format!("{:<name$}{:>6}{:>7}  ", "PROJECT", "OPEN", "YOURS"),
        style::bold(),
    )];
    head.extend(
        WORDS
            .iter()
            .map(|w| seg(format!("{w:>13}"), style::word(w))),
    );
    d.line(head);
    for r in rows {
        d.line(row(r, name));
    }
    if rows.is_empty() {
        d.plain(
            "  no projects: the server holds none, or could not be read",
            style::dim(),
        );
    }
    d.blank();
    d.plain(
        "  YOURS is what waits on the owner: docket todo.",
        style::dim(),
    );
    d
}

fn row(r: &HomeRow, name: usize) -> Vec<Seg> {
    let slug: String = r.project.slug.chars().take(name - 2).collect();
    let mut segs = vec![
        spot(
            slug.clone(),
            style::bold(),
            Target::Project(r.project.slug.clone()),
        ),
        seg(" ".repeat(name - slug.chars().count()), style::dim()),
    ];
    let Some(s) = &r.status else {
        let why = r.error.as_deref().unwrap_or("not read");
        segs.push(seg(format!("  {why}"), style::alarm()));
        return segs;
    };
    let owner = r.owner.map_or_else(|| "?".to_string(), |n| n.to_string());
    segs.push(seg(format!("{:>6}{owner:>7}  ", s.open()), style::bold()));
    for w in WORDS {
        let n = s.count(w);
        let tone = if n > 0 { style::word(w) } else { style::dim() };
        segs.push(seg(format!("{n:>13}"), tone));
    }
    segs
}
