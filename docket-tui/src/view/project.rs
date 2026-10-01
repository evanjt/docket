//! One project: the head and why nothing is worked, the flow, NEXT, the jobs, the packages under way,
//! and the moves.

use chrono::{Local, TimeZone};
use docket_core::fact;
use docket_core::pace::{Minute, duration};
use docket_core::rows::Row;
use ratatui::style::Style;

use crate::board::Board;
use crate::doc::{Doc, Listing, Route, Seg, Target, seg, spot, wrap};
use crate::page::ProjectData;
use crate::style;

/// The flow read left to right, then what sits beside it.
const FLOW: [&str; 4] = ["ready", "building", "checking", "done"];
const ASIDE: [&str; 4] = ["blocked", "parked", "inbox", "later"];
/// Started packages shown beyond those being worked.
const NEAREST: usize = 5;

#[must_use]
pub fn doc(board: &Board, data: Option<&ProjectData>, width: usize) -> Doc {
    let mut d = Doc::default();
    head(&mut d, board, data);
    why(&mut d, &board.project.skills, width);
    let Some(data) = data else {
        d.plain("  reading...", style::dim());
        return d;
    };
    d.blank();
    d.line(flow(data));
    d.line(lists(board));
    d.blank();
    next(&mut d, board, &data.next, width);
    d.blank();
    machines(&mut d, board, &data.wip);
    jobs(&mut d, &data.wip, data.now, width);
    d.blank();
    packages(&mut d, board, width);
    d.blank();
    moves(&mut d, data);
    d
}

/// The head line, then the line saying why nothing is worked when the facts say so.
pub fn head(d: &mut Doc, board: &Board, data: Option<&ProjectData>) {
    let skills = &board.project.skills;
    let mode = fact::effective(skills, "mode").unwrap_or_default();
    let mut segs = vec![seg(board.project.slug.clone(), style::bold())];
    if let Some(release) = skills
        .get("release")
        .and_then(|r| r.split_whitespace().next())
    {
        segs.push(seg(format!("   release {release}"), Style::default()));
    }
    let tone = if mode == "run" {
        style::word("ready")
    } else {
        style::alarm()
    };
    segs.push(seg("   ", Style::default()));
    segs.push(seg(mode.to_uppercase(), tone));
    let slots: u64 = fact::pool(&fact::effective(skills, "pool").unwrap_or_default())
        .iter()
        .map(|(_, n)| n)
        .sum();
    if let Some(data) = data {
        let running = data.wip.iter().filter(|r| r.claim_job.is_some()).count();
        segs.push(seg(
            format!("   jobs {running} of {slots} slots"),
            Style::default(),
        ));
        segs.extend(pace(data));
    }
    d.line(segs);
}

/// The line under the head when the facts keep the loop from dispatching, naming each missing fact.
pub fn why(d: &mut Doc, skills: &std::collections::BTreeMap<String, String>, width: usize) {
    let gaps = fact::gaps(skills);
    if gaps.is_empty() {
        return;
    }
    let text = format!(
        "Nothing is worked: {}. S shows the settings.",
        gaps.join(", ")
    );
    for line in wrap(&text, width.saturating_sub(4)) {
        d.plain(format!("  {line}"), style::alarm());
    }
}

/// The pace of closes and how long the ready work takes at it, when anything closed lately.
fn pace(data: &ProjectData) -> Vec<Seg> {
    let Some(per_hour) = data.pace.per_hour() else {
        return Vec::new();
    };
    let mut out = vec![seg(format!("   closing {per_hour}/h"), Style::default())];
    let todo =
        data.status.count("ready") + data.status.count("building") + data.status.count("checking");
    if todo > 0 {
        let eta = i64::try_from(todo * 3600 / per_hour).unwrap_or(0);
        out.push(seg(
            format!("   clear in {}", duration(eta)),
            Style::default(),
        ));
    }
    out
}

/// `ready 212 > building 18 > checking 6 > done 141    blocked 3  parked 9`, every count a hot spot.
fn flow(data: &ProjectData) -> Vec<Seg> {
    let mut segs = Vec::new();
    for (i, w) in FLOW.iter().enumerate() {
        if i > 0 {
            segs.push(seg(" > ", style::dim()));
        }
        let text = format!("{w} {}", data.status.count(w));
        segs.push(spot(
            text,
            style::word(w),
            Target::List(Listing::Word((*w).into())),
        ));
    }
    segs.push(seg("    ", Style::default()));
    for w in ASIDE.iter().filter(|w| data.status.count(w) > 0) {
        let text = format!("{w} {}", data.status.count(w));
        segs.push(spot(
            text,
            style::word(w),
            Target::List(Listing::Word((*w).into())),
        ));
        segs.push(seg("  ", Style::default()));
    }
    segs
}

/// The list routes, each a hot spot, the owner's two with their counts.
fn lists(board: &Board) -> Vec<Seg> {
    let mut segs = vec![seg("LISTS ", style::bold())];
    for r in Route::ALL {
        let text = match r {
            Route::Todo => format!("yours {}", board.on_owner()),
            Route::Questions => format!("questions {}", board.undecided()),
            _ => r.name().to_string(),
        };
        let tone = if r == Route::Todo {
            style::word("parked")
        } else {
            Style::default()
        };
        segs.push(seg(" ", Style::default()));
        segs.push(spot(text, tone, Target::List(Listing::Route(r))));
    }
    segs
}

fn item_spot(board: &Board, id: &str, pad: usize) -> Vec<Seg> {
    let tone = board
        .word_of(id)
        .map_or_else(Style::default, |w| style::word(&w));
    vec![
        spot(id, tone, Target::Item(id.to_string())),
        seg(" ".repeat(pad.saturating_sub(id.len())), Style::default()),
    ]
}

fn cut(text: &str, room: usize) -> String {
    text.chars().take(room).collect()
}

fn next(d: &mut Doc, board: &Board, rows: &[Row], width: usize) {
    if rows.is_empty() {
        d.plain(
            "NEXT  nothing ready: the inbox, what is parked or what is blocked holds the rest",
            style::bold(),
        );
        return;
    }
    d.plain("NEXT  what a free slot takes, in order", style::bold());
    for r in rows {
        let mut segs = vec![seg(" ", Style::default())];
        segs.extend(item_spot(board, &r.id, 7));
        let pri = if r.priority == "normal" {
            ""
        } else {
            r.priority.as_str()
        };
        segs.push(seg(format!("{pri:<9}"), style::alarm()));
        let package = board.get(&r.id).and_then(|i| board.package_of(i.rid));
        if let Some(p) = package {
            segs.extend(item_spot(board, &p.id, p.id.len() + 1));
        }
        segs.push(seg(
            cut(&r.title, width.saturating_sub(26)),
            Style::default(),
        ));
        d.line(segs);
    }
}

/// MACHINES: each host of the pool, with the jobs it runs of its slots. A job names its build host,
/// none meaning the loop's own, `local`.
fn machines(d: &mut Doc, board: &Board, wip: &[Row]) {
    let pool = fact::pool(&fact::effective(&board.project.skills, "pool").unwrap_or_default());
    let mut segs = vec![seg("MACHINES", style::bold())];
    for (host, slots) in pool {
        let running = wip
            .iter()
            .filter(|r| r.claim_job.is_some() && r.claim_on.as_deref().unwrap_or("local") == host)
            .count();
        segs.push(seg(
            format!("  {host} {running} of {slots} slots"),
            Style::default(),
        ));
    }
    d.line(segs);
}

/// JOBS: every claim, its branch, where it runs and for how long.
fn jobs(d: &mut Doc, wip: &[Row], now: i64, width: usize) {
    if wip.is_empty() {
        d.plain("JOBS  nothing claimed", style::bold());
        return;
    }
    d.plain(format!("JOBS  {} claimed", wip.len()), style::bold());
    for r in wip {
        let host = r
            .claim_on
            .as_ref()
            .or(r.claim_host.as_ref())
            .map_or("", |h| h.split('.').next().unwrap_or(""));
        let since = r
            .claim_since
            .as_deref()
            .and_then(docket_core::pace::epoch)
            .map_or_else(String::new, |t| duration(now - t));
        let branch = r.claim_branch.clone().unwrap_or_default();
        let mut segs = vec![seg(" ", Style::default())];
        segs.push(spot(
            r.id.clone(),
            style::word(&r.word),
            Target::Item(r.id.clone()),
        ));
        segs.push(seg(
            " ".repeat(7usize.saturating_sub(r.id.len())),
            Style::default(),
        ));
        let line = format!("{:<12}{since:>8}  {branch}  {}", cut(host, 11), r.title);
        segs.push(seg(cut(&line, width.saturating_sub(9)), Style::default()));
        d.line(segs);
    }
}

/// A bar of ten cells filled in proportion: `######....`.
#[must_use]
pub fn bar(part: u64, whole: u64) -> String {
    if whole == 0 {
        return ".".repeat(10);
    }
    let n = usize::try_from((part * 10 + whole / 2) / whole)
        .unwrap_or(10)
        .min(10);
    format!("{}{}", "#".repeat(n), ".".repeat(10 - n))
}

/// PACKAGES: those with tickets being worked, then the few started ones nearest done.
fn packages(d: &mut Doc, board: &Board, width: usize) {
    let all = board.packages_under_way();
    if all.is_empty() {
        d.plain("PACKAGES  none started", style::bold());
        return;
    }
    let live = all.iter().filter(|(_, g)| g.live > 0).count();
    let shown = (live + NEAREST).min(all.len());
    let nearest = if shown > live {
        format!(", and the {} nearest done", shown - live)
    } else {
        String::new()
    };
    d.plain(
        format!(
            "PACKAGES  {} started, {live} being worked{nearest}",
            all.len()
        ),
        style::bold(),
    );
    for (p, g) in &all[..shown] {
        let mut segs = vec![seg(" ", Style::default())];
        segs.extend(item_spot(board, &p.id, 7));
        let text = format!(
            "{}  {:>3}/{:<3}{:>4}  {}",
            bar(g.done, g.total),
            g.done,
            g.total,
            g.live,
            p.title
        );
        segs.push(seg(cut(&text, width.saturating_sub(9)), Style::default()));
        d.line(segs);
    }
}

/// `HH:MM` for today, the date as well for anything older, in local time.
#[must_use]
pub fn stamp(t: i64, now: i64) -> String {
    let (Some(at), Some(today)) = (
        Local.timestamp_opt(t, 0).single(),
        Local.timestamp_opt(now, 0).single(),
    ) else {
        return String::new();
    };
    if at.date_naive() == today.date_naive() {
        at.format("%H:%M").to_string()
    } else {
        at.format("%m-%d %H:%M").to_string()
    }
}

/// MOVES, newest first: when, the open count before and after, and each verb with its ids.
fn moves(d: &mut Doc, data: &ProjectData) {
    let title = match (data.stale, data.minutes.first()) {
        (_, None) => "MOVES  none recorded yet".to_string(),
        (true, Some(m)) => format!(
            "MOVES  none in the last day; the last ones, {} ago",
            duration(data.now - m.at)
        ),
        (false, _) => "MOVES, last day".to_string(),
    };
    d.plain(title, style::bold());
    for m in &data.minutes {
        d.line(minute(m, data.now));
    }
}

fn minute(m: &Minute, now: i64) -> Vec<Seg> {
    let ago = format!("{} ago", duration(now - m.at));
    let delta = m.after - m.before;
    let tone = if delta < 0 {
        style::word("done")
    } else {
        style::bold()
    };
    let mut segs = vec![
        seg(
            format!(
                "  {:>11} {ago:>8}  {:>4} -> {:<4} ",
                stamp(m.at, now),
                m.before,
                m.after
            ),
            Style::default(),
        ),
        seg(format!("{delta:+}"), tone),
    ];
    for (verb, ids) in &m.verbs {
        segs.push(seg(format!("  {verb}:"), style::verb(verb)));
        for id in ids {
            segs.push(seg(" ", Style::default()));
            segs.push(spot(
                id.clone(),
                style::verb(verb),
                Target::Item(id.clone()),
            ));
        }
    }
    segs
}
