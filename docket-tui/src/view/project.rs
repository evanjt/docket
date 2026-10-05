//! One project as a dashboard: a sidebar with the progress, what is claimed now, what waits on the
//! owner, the plans due for audit and how work starts, beside NEXT, the plans under way and the moves.
//! Narrower than `WIDE`, the sidebar stands above the main pane.

use chrono::{Local, TimeZone};
use docket_core::flow::GET_GOING;
use docket_core::pace::{Minute, duration};
use docket_core::rows::Row;
use ratatui::style::Style;

use crate::doc::{Doc, Listing, Route, Seg, Target, seg, spot, wrap};
use crate::filter::Filter;
use crate::page::ProjectData;
use crate::style;
use docket_core::board::Board;

/// The narrowest screen the sidebar stands beside the main pane on.
const WIDE: usize = 120;
/// The sidebar's width beside the main pane, and the gap after it.
const SIDE: usize = 44;
const GAP: usize = 2;
/// The flow counts the sidebar always shows; the rest only when something carries them.
const FLOW: [&str; 3] = ["ready", "in progress", "building"];
const ASIDE: [&str; 4] = ["audit due", "blocked", "waiting on owner", "parked"];
/// Rows each sidebar block shows before it says how many more.
const ROWS: usize = 5;
/// Rows of NEXT, and of MOVES until `m` shows them all.
const NEXT: usize = 5;
/// Cell heights of the trend lines, lowest first.
const SPARK: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const MOVES: usize = 5;

#[must_use]
pub fn doc(board: &Board, data: Option<&ProjectData>, width: usize, all_moves: bool) -> Doc {
    let Some(data) = data else {
        let mut d = Doc::default();
        d.plain("  reading...", style::dim());
        return d;
    };
    if width >= WIDE {
        let main_width = width - SIDE - GAP;
        beside(
            sidebar(board, data, SIDE),
            main(board, data, main_width, all_moves),
        )
    } else {
        let mut d = sidebar(board, data, width);
        d.blank();
        d.lines.extend(main(board, data, width, all_moves).lines);
        d
    }
}

/// Two documents side by side, the left one padded to `SIDE` and the gap.
fn beside(left: Doc, right: Doc) -> Doc {
    let rows = left.lines.len().max(right.lines.len());
    let mut lefts = left.lines.into_iter();
    let mut rights = right.lines.into_iter();
    let mut d = Doc::default();
    for _ in 0..rows {
        let mut line = lefts.next().unwrap_or_default();
        let used: usize = line.iter().map(|s| s.text.chars().count()).sum();
        line.push(seg(
            " ".repeat(SIDE + GAP - used.min(SIDE)),
            Style::default(),
        ));
        line.extend(rights.next().unwrap_or_default());
        d.line(line);
    }
    d
}

/// A line cut to the width, the cut falling inside whichever segment crosses it.
fn fit(segs: Vec<Seg>, width: usize) -> Vec<Seg> {
    let mut room = width;
    let mut out = Vec::new();
    for mut s in segs {
        if room == 0 {
            break;
        }
        let n = s.text.chars().count();
        if n > room {
            s.text = s.text.chars().take(room).collect();
        }
        room -= s.text.chars().count();
        out.push(s);
    }
    out
}

fn cut(text: &str, room: usize) -> String {
    text.chars().take(room).collect()
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

// ---- the sidebar ----

fn sidebar(board: &Board, data: &ProjectData, width: usize) -> Doc {
    let mut d = Doc::default();
    progress(&mut d, data, width);
    d.blank();
    releases(&mut d, data, width);
    d.blank();
    checks(&mut d, data, width);
    d.blank();
    lead(&mut d, data, width);
    d.blank();
    claimed(&mut d, board, &data.wip, data.now, width);
    d.blank();
    machines(&mut d, data, width);
    d.blank();
    yours(&mut d, board, width);
    d.blank();
    due(&mut d, board, width);
    d.blank();
    get_going(&mut d, width);
    d.blank();
    lists(&mut d, board, width);
    d
}

/// PROGRESS: closed of everything in the flow, the forecast, and the open counts, each a hot spot.
fn progress(d: &mut Doc, data: &ProjectData, width: usize) {
    d.plain("PROGRESS", style::bold());
    let closed = data.status.count("done");
    let all = closed + data.status.open();
    d.plain(
        cut(
            &format!(" {}  {closed} of {all} closed", bar(closed, all)),
            width,
        ),
        Style::default(),
    );
    if let Some(forecast) = data.releases.first().map(|r| &r.forecast) {
        for line in wrap(&forecast.line(), width.saturating_sub(1)) {
            d.plain(cut(&format!(" {line}"), width), Style::default());
        }
    }
    let trend = trend(&data.trend);
    if !trend.is_empty() {
        d.plain(cut(&format!(" {trend}"), width), Style::default());
    }
    let mut segs = vec![seg(" ", Style::default())];
    let shown = FLOW
        .iter()
        .chain(ASIDE.iter().filter(|w| data.status.count(w) > 0));
    for (i, w) in shown.enumerate() {
        if i > 0 {
            segs.push(seg("  ", Style::default()));
        }
        segs.push(spot(
            format!("{w} {}", data.status.count(w)),
            style::word(w),
            Target::List(Listing::Word((*w).into())),
        ));
    }
    d.line(fit(segs, width));
}

/// RELEASES: each release not shipped, its closed of all, a hot spot to the queue for that release.
fn releases(d: &mut Doc, data: &ProjectData, width: usize) {
    d.plain("RELEASES", style::bold());
    if data.releases.is_empty() {
        d.plain(" none set", style::dim());
    }
    for r in &data.releases {
        let name = &r.name;
        let total = r.closed + r.open;
        let counts = [
            ("ready", r.ready),
            ("building", r.building),
            ("owner", r.waiting_owner),
            ("blocked", r.blocked),
            ("held later", r.held_later),
        ]
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(w, n)| format!("{w} {n}"))
        .collect::<Vec<_>>()
        .join("  ");
        let segs = vec![
            seg(" ", Style::default()),
            spot(
                format!("{name:<9}"),
                Style::default(),
                Target::List(Listing::Next(Box::new(Filter {
                    release: Some(name.clone()),
                    ..Filter::default()
                }))),
            ),
            seg(
                format!("{}  {} of {total} closed", bar(r.closed, total), r.closed),
                Style::default(),
            ),
        ];
        d.line(fit(segs, width));
        if !counts.is_empty() {
            d.plain(cut(&format!("   {counts}"), width), style::dim());
        }
    }
}

/// CHECKS: the count of each kind of problem `check` finds, a hot spot to the items it names.
fn checks(d: &mut Doc, data: &ProjectData, width: usize) {
    let problems = serde_json::Value::Array(data.problems.clone());
    let counts = docket_core::check::counts(&problems);
    d.plain("CHECKS", style::bold());
    if counts.is_empty() {
        d.plain(" clean", style::dim());
    }
    for (kind, n) in counts {
        let segs = vec![
            seg(" ", Style::default()),
            spot(
                docket_core::check::phrase(&kind, n),
                style::alarm(),
                Target::List(Listing::Problems(kind)),
            ),
        ];
        d.line(fit(segs, width));
    }
}

/// Opened and closed per day as two sparklines, oldest day first.
fn trend(days: &[(u64, u64)]) -> String {
    if days.is_empty() {
        return String::new();
    }
    let peak = days.iter().map(|(o, c)| (*o).max(*c)).max().unwrap_or(0);
    let line = |pick: fn(&(u64, u64)) -> u64| -> String {
        days.iter()
            .map(|d| {
                let n = pick(d);
                if n == 0 {
                    return ' ';
                }
                let level = usize::try_from(n * 7 / peak.max(1)).unwrap_or(7).min(7);
                SPARK[level]
            })
            .collect()
    };
    format!(
        "{}d opened [{}] closed [{}]",
        days.len(),
        line(|d| d.0),
        line(|d| d.1)
    )
}

/// LEAD: who leads the project and when the claim was last renewed, or that it lapsed or is free.
fn lead(d: &mut Doc, data: &ProjectData, width: usize) {
    d.plain("LEAD", style::bold());
    let now = data.now;
    let line = match data.lead.as_ref() {
        None => " lead unread".to_string(),
        Some(state) => match &state.lead {
            None => " none holds the lead".to_string(),
            Some(l) => {
                let when = docket_core::pace::epoch(&l.renewed_at).map_or_else(
                    || l.renewed_at.clone(),
                    |t| format!("{} ago", duration(now - t)),
                );
                let host = l.host.split('.').next().unwrap_or("");
                let word = if state.lapsed { "lapsed" } else { "renewed" };
                format!(" {} on {host}, {word} {when}", l.session)
            }
        },
    };
    let tone = match data.lead.as_ref() {
        Some(s) if s.lapsed => style::alarm(),
        _ => Style::default(),
    };
    d.plain(cut(&line, width), tone);
}

/// MACHINES: each machine's slots in use, counted from the claims running on it.
fn machines(d: &mut Doc, data: &ProjectData, width: usize) {
    d.plain("MACHINES", style::bold());
    if data.machines.is_empty() {
        d.plain(" none set", style::dim());
    }
    for m in &data.machines {
        let used = data
            .wip
            .iter()
            .filter(|r| r.claim_on.as_deref() == Some(m.name.as_str()))
            .count();
        d.plain(
            cut(&format!(" {} {used}/{}", m.name, m.slots), width),
            Style::default(),
        );
    }
}

/// CLAIMED NOW: every claim, its branch, the host it was claimed on and for how long.
fn claimed(d: &mut Doc, board: &Board, wip: &[Row], now: i64, width: usize) {
    d.line(vec![spot(
        format!("CLAIMED NOW  {}", wip.len()),
        style::bold(),
        Target::List(Listing::Route(Route::Wip)),
    )]);
    if wip.is_empty() {
        d.plain(" nothing claimed", style::dim());
    }
    for r in wip.iter().take(ROWS) {
        let host = r
            .claim_host
            .as_deref()
            .map_or("", |h| h.split('.').next().unwrap_or(""));
        let since = r
            .claim_since
            .as_deref()
            .and_then(docket_core::pace::epoch)
            .map_or_else(String::new, |t| duration(now - t));
        let branch = r.claim_branch.clone().unwrap_or_default();
        let runner = match (r.claim_runner.as_deref(), r.claim_job.as_deref()) {
            (Some(runner), Some(job)) => format!("  {runner} {job}"),
            (None, Some(job)) => format!("  {job}"),
            (Some(runner), None) => format!("  {runner}"),
            (None, None) => String::new(),
        };
        let mut segs = vec![seg(" ", Style::default())];
        segs.extend(item_spot(board, &r.id, 7));
        segs.push(seg(
            format!("{branch} on {host}  {since}{runner}"),
            Style::default(),
        ));
        d.line(fit(segs, width));
    }
    more(d, wip.len());
}

/// YOURS: what waits on the owner, a hot spot to the owner's queue.
fn yours(d: &mut Doc, board: &Board, width: usize) {
    let mut rows: Vec<_> = board
        .items
        .iter()
        .filter(|i| i.state == "open" && i.turn.as_deref() == Some("user"))
        .collect();
    rows.sort_by(|a, b| (&a.key, a.num).cmp(&(&b.key, b.num)));
    d.line(vec![spot(
        format!("YOURS  {}", rows.len()),
        style::word("waiting on owner"),
        Target::List(Listing::Route(Route::Todo)),
    )]);
    if rows.is_empty() {
        d.plain(" nothing waits on you", style::dim());
    }
    for i in rows.iter().take(ROWS) {
        titled(d, board, &i.id, &i.title, width);
    }
    more(d, rows.len());
}

/// AUDITS DUE: the plans whose tickets are all closed and that nobody audits yet.
fn due(d: &mut Doc, board: &Board, width: usize) {
    let rows = board.due_audits();
    d.plain(format!("AUDITS DUE  {}", rows.len()), style::bold());
    if rows.is_empty() {
        d.plain(" no plan is due", style::dim());
    }
    for p in rows.iter().take(ROWS) {
        titled(d, board, &p.id, &p.title, width);
    }
    more(d, rows.len());
}

fn titled(d: &mut Doc, board: &Board, id: &str, title: &str, width: usize) {
    let mut segs = vec![seg(" ", Style::default())];
    segs.extend(item_spot(board, id, 7));
    segs.push(seg(title.to_string(), Style::default()));
    d.line(fit(segs, width));
}

fn more(d: &mut Doc, n: usize) {
    if n > ROWS {
        d.plain(format!(" and {} more", n - ROWS), style::dim());
    }
}

/// TO GET GOING: a session per role, each started with its skill.
fn get_going(d: &mut Doc, width: usize) {
    d.plain("TO GET GOING  a session per role", style::bold());
    for (skill, what) in GET_GOING {
        d.line(fit(
            vec![
                seg(format!(" {skill:<8}"), style::word("ready")),
                seg(what, Style::default()),
            ],
            width,
        ));
    }
}

/// The list routes, each a hot spot, wrapped to the width.
fn lists(d: &mut Doc, board: &Board, width: usize) {
    let mut line = vec![seg("LISTS", style::bold())];
    let mut used = 5;
    for r in Route::ALL {
        let text = match r {
            Route::Questions => format!("questions {}", board.undecided()),
            _ => r.name().to_string(),
        };
        if used + 1 + text.len() > width {
            d.line(std::mem::take(&mut line));
            used = 0;
        }
        line.push(seg(" ", Style::default()));
        used += 1 + text.len();
        line.push(spot(
            text,
            Style::default(),
            Target::List(Listing::Route(r)),
        ));
    }
    d.line(line);
}

// ---- the main pane ----

fn main(board: &Board, data: &ProjectData, width: usize, all_moves: bool) -> Doc {
    let mut d = Doc::default();
    next(&mut d, board, &data.next, width);
    d.blank();
    plans(&mut d, board, width);
    d.blank();
    moves(&mut d, data, all_moves);
    d
}

fn next(d: &mut Doc, board: &Board, rows: &[Row], width: usize) {
    if rows.is_empty() {
        d.plain(
            "NEXT  nothing ready: what is blocked or waits on the owner holds the rest",
            style::bold(),
        );
        return;
    }
    d.line(vec![spot(
        format!("NEXT  the first {} in the queue", rows.len().min(NEXT)),
        style::bold(),
        Target::List(Listing::Route(Route::Next)),
    )]);
    for r in rows.iter().take(NEXT) {
        let mut segs = vec![seg(" ", Style::default())];
        segs.extend(item_spot(board, &r.id, 7));
        let pri = if r.priority == "normal" {
            ""
        } else {
            r.priority.as_str()
        };
        segs.push(seg(format!("{pri:<9}"), style::alarm()));
        segs.push(seg(r.title.clone(), Style::default()));
        d.line(fit(segs, width));
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

/// PLANS: those with tickets being worked, then the nearest done.
fn plans(d: &mut Doc, board: &Board, width: usize) {
    let all = board.plans_under_way();
    if all.is_empty() {
        d.plain("PLANS  none under way", style::bold());
        return;
    }
    d.plain(format!("PLANS  {} under way", all.len()), style::bold());
    for (p, g) in all.iter().take(ROWS) {
        let mut segs = vec![seg(" ", Style::default())];
        segs.extend(item_spot(board, &p.id, 7));
        segs.push(seg(
            format!(
                "{}  {:>3}/{:<3}{:>4}  {}",
                bar(g.done, g.total),
                g.done,
                g.total,
                g.live,
                p.title
            ),
            Style::default(),
        ));
        d.line(fit(segs, width));
    }
    more(d, all.len());
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

/// MOVES, newest first: the last few, or all of them once `m` asks.
fn moves(d: &mut Doc, data: &ProjectData, all: bool) {
    let Some(newest) = data.minutes.first() else {
        d.plain("MOVES  none recorded yet", style::bold());
        return;
    };
    let n = data.minutes.len();
    let (count, key) = match (n > MOVES, all) {
        (false, _) => (format!("the last {n}"), String::new()),
        (true, false) => (
            format!("the last {MOVES} of {n}"),
            "   m shows them all".to_string(),
        ),
        (true, true) => (format!("all {n}"), format!("   m shows the last {MOVES}")),
    };
    d.plain(
        format!(
            "MOVES  {count}, the newest {} ago{key}",
            duration(data.now - newest.at)
        ),
        style::bold(),
    );
    let shown = if all { n } else { n.min(MOVES) };
    for m in &data.minutes[..shown] {
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
