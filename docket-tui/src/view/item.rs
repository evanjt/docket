//! One item in full: its head, its facts, its body, what it cites, its ties and its log. Every id in it
//! that the project holds is a hot spot.

use docket_core::pace::{duration, epoch};
use docket_core::rows::{EventRow, Row, Shown};
use ratatui::style::Style;

use crate::board::Board;
use crate::doc::{Doc, Listing, Seg, Target, linked, seg, spot, wrap};
use crate::page::{Detail, Queue};
use crate::style;

/// The log's newest moves shown, edits left out.
const LOG_ROWS: usize = 15;

/// The colour of an id the board holds, `None` for one it does not, which then stays plain text.
fn lookup(board: Option<&Board>) -> impl Fn(&str) -> Option<Style> + '_ {
    move |id| board?.word_of(id).map(|w| style::word(&w))
}

#[must_use]
pub fn doc(board: Option<&Board>, d: &Detail, width: usize, now: i64) -> Doc {
    let mut out = Doc::default();
    let r = &d.shown.row;
    head(&mut out, board, r, width);
    facts(&mut out, board, r, now, width);
    if let Some(g) = d.shown.progress {
        out.plain(
            format!(
                "       members: {} of {} done, {} live",
                g.done, g.total, g.live
            ),
            Style::default(),
        );
    }
    out.blank();
    body(&mut out, board, &r.body, width);
    cites(&mut out, &d.shown, width);
    ties(&mut out, board, &d.shown);
    log(&mut out, board, &d.log, width);
    out
}

fn head(out: &mut Doc, board: Option<&Board>, r: &Row, width: usize) {
    let title: String = r
        .title
        .chars()
        .take(width.saturating_sub(r.id.len() + 2))
        .collect();
    out.line(vec![seg(format!("{}. {title}", r.id), style::bold())]);
    let mut segs = vec![
        seg(format!("{:<7}", r.id), Style::default()),
        seg(format!("{:<9}", r.word), style::word(&r.word)),
    ];
    if r.priority != "normal" {
        segs.push(seg(format!("{} ", r.priority), style::alarm()));
    }
    if let Some(c) = &r.complexity {
        segs.push(seg(format!("[{c}] "), style::dim()));
    }
    if let Some(g) = &r.group {
        segs.push(seg("group ", style::dim()));
        segs.push(spot(
            g.clone(),
            Style::default(),
            Target::List(Listing::Group(g.clone())),
        ));
    }
    out.line(segs);
    if let Some(s) = &r.superseded_by {
        let mut segs = vec![seg("       superseded by ", Style::default())];
        segs.extend(linked(s, Style::default(), &lookup(board)));
        out.line(segs);
    }
}

/// Who holds it, what it waits on, what the owner was asked, its decision or resolution, its dates.
fn facts(out: &mut Doc, board: Option<&Board>, r: &Row, now: i64, width: usize) {
    let ago =
        |at: &str| epoch(at).map_or_else(String::new, |t| format!(" ({} ago)", duration(now - t)));
    let mut lines = Vec::new();
    if let Some(b) = &r.claim_branch {
        let host = r.claim_host.clone().unwrap_or_default();
        let since = r.claim_since.clone().unwrap_or_default();
        lines.push(format!(
            "held by {b} on {host} since {since}{}",
            ago(&since)
        ));
    }
    match (r.wait_on.as_deref(), r.wait_ref.as_deref()) {
        (Some("item"), Some(on)) => lines.push(format!("waits on {on}")),
        (Some(_), Some(until)) => lines.push(format!("waits until: {until}")),
        _ => {}
    }
    if r.turn.as_deref() == Some("user")
        && let Some(note) = &r.turn_note
    {
        lines.push(format!("asked: {note}"));
    }
    if let Some(dec) = &r.decision {
        lines.push(format!(
            "decided {}: {dec}",
            r.decided_at.clone().unwrap_or_default()
        ));
    }
    if r.state != "open" {
        lines.push(format!(
            "{}: {}",
            r.state,
            r.resolution.clone().unwrap_or_default()
        ));
    }
    lines.push(format!("opened {}{}", r.opened_at, ago(&r.opened_at)));
    lines.push(format!("updated {}{}", r.updated_at, ago(&r.updated_at)));
    if let Some(t) = &r.theme {
        lines.push(format!("theme {t}"));
    }
    for line in lines {
        for part in wrap(&line, width.saturating_sub(8)) {
            let mut segs = vec![seg("       ", Style::default())];
            segs.extend(linked(&part, style::dim(), &lookup(board)));
            out.line(segs);
        }
    }
}

fn body(out: &mut Doc, board: Option<&Board>, text: &str, width: usize) {
    if text.trim().is_empty() {
        out.plain("(no body)", style::dim());
        return;
    }
    out.prose(text, width.saturating_sub(2), &lookup(board));
}

fn cites(out: &mut Doc, shown: &Shown, width: usize) {
    let paths: Vec<String> = shown
        .cites
        .iter()
        .filter_map(|c| {
            let p = c.path.clone()?;
            Some(c.line.map_or(p.clone(), |l| format!("{p}:{l}")))
        })
        .take(20)
        .collect();
    if paths.is_empty() {
        return;
    }
    out.blank();
    for line in wrap(
        &format!("Cites: {}", paths.join(", ")),
        width.saturating_sub(2),
    ) {
        out.plain(line, style::dim());
    }
}

/// What opened it, what it is related to and what it opened, each with its word and title.
fn ties(out: &mut Doc, board: Option<&Board>, shown: &Shown) {
    let me = board.and_then(|b| b.get(&shown.row.id));
    let children: Vec<String> = match (board, me) {
        (Some(b), Some(me)) => b.children(me.rid).iter().map(|c| c.id.clone()).collect(),
        _ => Vec::new(),
    };
    for (label, ids) in [
        ("Opened by", &shown.opened),
        ("Related", &shown.related),
        ("Opened", &children),
    ] {
        if ids.is_empty() {
            continue;
        }
        out.blank();
        out.plain(format!("{label}:"), style::bold());
        for id in ids {
            out.line(tie(board, id));
        }
    }
}

fn tie(board: Option<&Board>, id: &str) -> Vec<Seg> {
    let item = board.and_then(|b| b.get(id));
    let word = item.and_then(|_| board?.word_of(id)).unwrap_or_default();
    let title = item.map_or("", |i| i.title.as_str());
    vec![
        seg("  ", Style::default()),
        spot(id, style::word(&word), Target::Item(id.to_string())),
        seg(
            " ".repeat(7usize.saturating_sub(id.len())),
            Style::default(),
        ),
        seg(format!("{word:<9}"), style::word(&word)),
        seg(title.to_string(), Style::default()),
    ]
}

fn log(out: &mut Doc, board: Option<&Board>, events: &[EventRow], width: usize) {
    if events.is_empty() {
        return;
    }
    out.blank();
    out.plain(format!("Log, {} events:", events.len()), style::bold());
    let moves: Vec<&EventRow> = events
        .iter()
        .filter(|e| e.kind != "edited" && e.kind != "queue")
        .collect();
    for e in &moves[moves.len().saturating_sub(LOG_ROWS)..] {
        let branch = e.branch.clone().unwrap_or_default();
        let note = e.note.clone().unwrap_or_default().replace('\n', " ");
        let line = format!(
            "  {}  {:<10} {branch:<24} {:<10} {note}",
            e.at, e.host, e.kind
        );
        let line: String = line.chars().take(width.saturating_sub(1)).collect();
        let tone = style::verb_tint(&e.kind).map_or_else(Style::default, style::word);
        out.line(linked(&line, tone, &lookup(board)));
    }
}

/// The owner's queue: which of how many, then the item in full.
#[must_use]
pub fn queue_doc(board: Option<&Board>, q: &Queue, width: usize) -> Doc {
    let Some(row) = q.rows.get(q.at) else {
        let mut d = Doc::default();
        d.plain("YOURS  nothing waits on the owner", style::bold());
        return d;
    };
    let mut d = Doc::default();
    d.line(vec![
        seg(
            format!("YOURS  {} of {}", q.at + 1, q.rows.len()),
            style::bold(),
        ),
        seg(
            "   what is on your turn, then the open questions; j and k step through them",
            style::dim(),
        ),
    ]);
    d.blank();
    let detail = Detail {
        shown: Shown {
            row: row.clone(),
            ..Shown::default()
        },
        log: Vec::new(),
    };
    let item = doc(board, &detail, width, crate::load::now());
    d.lines.extend(item.lines);
    d
}
