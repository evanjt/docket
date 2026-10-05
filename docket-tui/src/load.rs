//! Reading what each page shows, from the source and the project's board.

use std::time::{SystemTime, UNIX_EPOCH};

use docket_core::pace::{Class, Counted, Logged, Move, Net, epoch, minutes, moves, net, pace};
use docket_core::rows::{EventRow, Row};
use docket_core::word::Kind;

use crate::app::App;
use crate::doc::{Listing, Route};
use crate::filter::Filter;
use crate::page::{Detail, Entry, HomeRow, Page, PlanRow, ProjectData};
use crate::source::Source;
use docket_core::board::Board;

/// How many closes the pace is read over.
const RECENT_CLOSES: u64 = 20;
/// How far back the moves reach, in seconds.
const WINDOW: i64 = 86_400;
/// How many rows NEXT shows.
pub const NEXT_ROWS: usize = 8;

#[must_use]
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

impl<S: Source> App<S> {
    /// Reads the open page again, keeping where its pointers stand.
    pub fn load(&mut self) {
        self.generation += 1;
        match &self.page {
            Page::Home(_) => self.load_home(),
            Page::Project(_) => self.load_project(),
            Page::Browser(_) => self.load_browser(),
            Page::Queue(_) => self.load_queue(),
            Page::Plans(_) => self.load_plans(),
            Page::Settings(s) => {
                let slug = s.slug.clone();
                self.board(&slug);
            }
            Page::Help(_) => {}
        }
    }

    fn load_home(&mut self) {
        let projects = match self.source.projects() {
            Ok(p) => p,
            Err(e) => {
                self.flash = Some(e);
                return;
            }
        };
        let source = &self.source;
        let rows: Vec<HomeRow> = std::thread::scope(|s| {
            let handles: Vec<_> = projects
                .into_iter()
                .map(|project| s.spawn(move || home_row(source, project)))
                .collect();
            handles.into_iter().filter_map(|h| h.join().ok()).collect()
        });
        if let Page::Home(h) = &mut self.page {
            h.rows = rows;
            if h.cursor.index.is_none() && !h.rows.is_empty() {
                h.cursor.index = Some(0);
            }
        }
    }

    fn load_project(&mut self) {
        let Some(slug) = self.page.slug().map(str::to_string) else {
            return;
        };
        let Some(board) = self.board(&slug).cloned() else {
            return;
        };
        let read = || -> crate::source::Result<ProjectData> {
            let status = self.source.status(&slug)?;
            let next = self.source.next(&slug, NEXT_ROWS, &Filter::default())?;
            let wip = self.source.list("wip", &slug)?;
            let recent = self.source.recent(&slug)?;
            let mut data = project_data(&board, status, next, wip, &recent, now());
            data.lead = self.source.lead(&slug).ok();
            data.machines = self
                .source
                .machines()
                .map(|m| m.machines)
                .unwrap_or_default();
            Ok(data)
        };
        match read() {
            Ok(data) => {
                if let Page::Project(p) = &mut self.page {
                    p.data = Some(data);
                }
            }
            Err(e) => self.flash = Some(e),
        }
    }

    fn load_browser(&mut self) {
        let Page::Browser(b) = &self.page else {
            return;
        };
        let (slug, listing) = (b.slug.clone(), b.listing.clone());
        let entries = match self.entries(&slug, &listing) {
            Ok(e) => e,
            Err(e) => {
                self.flash = Some(e);
                Vec::new()
            }
        };
        if let Page::Browser(b) = &mut self.page {
            b.sel = b.sel.min(entries.len().saturating_sub(1));
            b.entries = entries;
        }
        self.load_detail();
    }

    /// The rows a listing holds.
    fn entries(&mut self, slug: &str, listing: &Listing) -> crate::source::Result<Vec<Entry>> {
        let rows = |r: Vec<Row>| r.iter().map(Entry::of_row).collect();
        match listing {
            Listing::Search(q) if q.trim().is_empty() => Ok(Vec::new()),
            Listing::Search(q) => self.source.search(slug, q).map(rows),
            Listing::Route(Route::Next) => {
                self.source.next(slug, 100, &Filter::default()).map(rows)
            }
            Listing::Next(f) => self.source.next(slug, 100, f).map(rows),
            Listing::Route(Route::Derived) => self
                .source
                .derived(slug)
                .map(|d| d.iter().map(Entry::of_derived).collect()),
            Listing::Route(r) => self.source.list(r.name(), slug).map(rows),
            Listing::Word(w) => {
                let board = self.board(slug).ok_or("no board")?;
                Ok(board
                    .with_word(w)
                    .into_iter()
                    .map(|i| Entry::of_item(i, w.clone()))
                    .collect())
            }
            Listing::Ties(id) => self.ties(slug, id),
            Listing::Group(name) => self.source.group(slug, name).map(rows),
        }
    }

    /// An item, then what opened it, what it is related to, and what it opened.
    fn ties(&mut self, slug: &str, id: &str) -> crate::source::Result<Vec<Entry>> {
        let shown = self.source.show(slug, id)?;
        let board = self.board(slug).ok_or("no board")?;
        let entry = |id: &str, note: &str| {
            board.get(id).map(|i| {
                let mut e = Entry::of_item(i, board.word(i));
                e.note = note.to_string();
                e
            })
        };
        let mut out: Vec<Entry> = entry(id, "this item").into_iter().collect();
        out.extend(shown.opened.iter().filter_map(|t| entry(t, "opened it")));
        out.extend(shown.related.iter().filter_map(|t| entry(t, "related")));
        if let Some(me) = board.get(id) {
            let kids: Vec<String> = board
                .children(me.rid)
                .iter()
                .map(|c| c.id.clone())
                .collect();
            out.extend(kids.iter().filter_map(|t| entry(t, "it opened")));
        }
        Ok(out)
    }

    /// The selected row of a browser in full, from the cache when it was read since the last change.
    pub fn load_detail(&mut self) {
        let Page::Browser(b) = &self.page else {
            return;
        };
        let Some(id) = b.selected().map(|e| e.id.clone()) else {
            if let Page::Browser(b) = &mut self.page {
                b.detail = None;
            }
            return;
        };
        let slug = b.slug.clone();
        let detail = self.detail(&slug, &id);
        if let Page::Browser(b) = &mut self.page {
            b.detail = detail;
        }
    }

    /// One item and its log, kept until the next change.
    pub fn detail(&mut self, slug: &str, id: &str) -> Option<Detail> {
        let key = (slug.to_string(), id.to_string());
        if let Some(d) = self.details.get(&key) {
            return Some(d.clone());
        }
        let rid = self.board(slug).and_then(|b| b.get(id)).map(|i| i.rid);
        let read = || -> crate::source::Result<Detail> {
            let shown = self.source.show(slug, id)?;
            let log = match rid {
                Some(rid) => self.source.log(rid)?,
                None => Vec::new(),
            };
            Ok(Detail { shown, log })
        };
        match read() {
            Ok(d) => {
                self.details.insert(key, d.clone());
                Some(d)
            }
            Err(e) => {
                self.flash = Some(e);
                None
            }
        }
    }

    /// The owner's queue: what is on their turn, then the questions not already in it.
    fn load_queue(&mut self) {
        let Some(slug) = self.page.slug().map(str::to_string) else {
            return;
        };
        let read = || -> crate::source::Result<Vec<Row>> {
            let mut rows = self.source.list("todo", &slug)?;
            for q in self.source.list("questions", &slug)? {
                if !rows.iter().any(|r| r.id == q.id) {
                    rows.push(q);
                }
            }
            Ok(rows)
        };
        match read() {
            Ok(rows) => {
                self.board(&slug);
                if let Page::Queue(q) = &mut self.page {
                    q.at = q.at.min(rows.len().saturating_sub(1));
                    q.rows = rows;
                }
            }
            Err(e) => self.flash = Some(e),
        }
    }

    fn load_plans(&mut self) {
        let Some(slug) = self.page.slug().map(str::to_string) else {
            return;
        };
        let Some(board) = self.board(&slug).cloned() else {
            return;
        };
        if let Page::Plans(p) = &mut self.page {
            p.rows = plan_rows(&board, &p.open);
            if p.rows.get(p.sel).is_none_or(|r| r.heading.is_some()) {
                p.sel = p.rows.iter().position(|r| r.heading.is_none()).unwrap_or(0);
            }
        }
        self.load_plan();
    }

    /// The selected plan row's item and what comes next under it.
    pub fn load_plan(&mut self) {
        let Page::Plans(p) = &self.page else {
            return;
        };
        let Some(id) = p
            .rows
            .get(p.sel)
            .map(|r| r.id.clone())
            .filter(|i| !i.is_empty())
        else {
            return;
        };
        let slug = p.slug.clone();
        let shown = self.source.show(&slug, &id);
        let next = self.source.next(&slug, 10, &Filter::under(&id));
        if let Err(e) = &shown {
            self.flash = Some(e.clone());
        }
        if let Page::Plans(p) = &mut self.page {
            p.shown = shown.ok();
            p.next = next.unwrap_or_default();
        }
    }
}

fn home_row<S: Source>(source: &S, project: docket_core::rows::ProjectRow) -> HomeRow {
    let slug = project.slug.clone();
    let status = source.status(&slug);
    let owner = source.list("todo", &slug).map(|r| r.len());
    let error = status.as_ref().err().or(owner.as_ref().err()).cloned();
    HomeRow {
        project,
        status: status.ok(),
        owner: owner.ok(),
        error,
    }
}

/// The project page's rows: the moves of the window, or the newest ones when it holds none, and the pace.
#[must_use]
pub fn project_data(
    board: &Board,
    status: docket_core::rows::Status,
    next: Vec<Row>,
    wip: Vec<Row>,
    recent: &[EventRow],
    now: i64,
) -> ProjectData {
    let all = moves_of(board, recent);
    let open = i64::try_from(status.open()).unwrap_or(0);
    let windowed: Vec<Move> = all
        .iter()
        .filter(|m| now - m.at <= WINDOW)
        .cloned()
        .collect();
    let stale = windowed.is_empty();
    let rows = if stale {
        minutes(&all, open).into_iter().take(8).collect()
    } else {
        minutes(&windowed, open)
    };
    ProjectData {
        pace: pace(&all, RECENT_CLOSES, now),
        net: net_of(board, recent, now - 3600),
        status,
        next,
        wip,
        minutes: rows,
        stale,
        now,
        lead: None,
        machines: Vec::new(),
        trend: daily(recent, TREND_DAYS, now),
    }
}

/// The days of the trend line.
const TREND_DAYS: usize = 7;

/// Opened and closed counts per local day for the `days` days ending today, oldest first. Days older
/// than the oldest event read are left out, so a page of events that does not reach back the whole
/// window never shows a false zero.
#[must_use]
pub fn daily(recent: &[EventRow], days: usize, now: i64) -> Vec<(u64, u64)> {
    use chrono::{Duration, Local, TimeZone};
    let Some(today) = Local.timestamp_opt(now, 0).single().map(|t| t.date_naive()) else {
        return Vec::new();
    };
    let stamps: Vec<(chrono::NaiveDate, &str)> = recent
        .iter()
        .filter_map(|e| {
            let at = Local.timestamp_opt(epoch(&e.at)?, 0).single()?;
            Some((at.date_naive(), e.kind.as_str()))
        })
        .collect();
    let Some(oldest) = stamps.iter().map(|(d, _)| *d).min() else {
        return Vec::new();
    };
    let span = i64::try_from(days).unwrap_or(0);
    (0..span)
        .rev()
        .map(|back| today - Duration::days(back))
        .filter(|day| *day > oldest)
        .map(|day| {
            let on = |kind: &str| {
                stamps
                    .iter()
                    .filter(|(d, k)| *d == day && *k == kind)
                    .count() as u64
            };
            (on("opened"), on("closed"))
        })
        .collect()
}

/// The moves of a project's newest events, oldest first, standing items left out.
fn moves_of(board: &Board, recent: &[EventRow]) -> Vec<Move> {
    let ids: Vec<(i64, String, &EventRow)> = recent
        .iter()
        .rev()
        .filter_map(|e| {
            let rid = e.rid?;
            let item = board.by_rid(rid).filter(|_| !board.is_standing(rid))?;
            Some((epoch(&e.at)?, item.id.clone(), e))
        })
        .collect();
    let logged: Vec<Logged> = ids
        .iter()
        .map(|(at, id, e)| Logged {
            at: *at,
            id,
            kind: &e.kind,
            note: e.note.as_deref(),
        })
        .collect();
    moves(&logged)
}

/// The current release's tickets closed and opened since `since`, and the research closes that opened
/// nothing, read from the newest events.
#[must_use]
pub fn net_of(board: &Board, recent: &[EventRow], since: i64) -> Net {
    let releases =
        docket_core::fact::releases(board.project.skills.get("releases").map(String::as_str));
    let counted: Vec<Counted> = recent
        .iter()
        .filter_map(|e| {
            let item = board.by_rid(e.rid?)?;
            let class = match board.kind(item) {
                Kind::Work => Class::Code {
                    current: docket_core::queue::release_rank(&releases, item.theme.as_deref())
                        == 0,
                },
                Kind::Decision | Kind::Research => Class::Research {
                    opened: !board.children(item.rid).is_empty(),
                },
                _ => Class::Other,
            };
            Some(Counted {
                at: epoch(&e.at)?,
                kind: &e.kind,
                class,
            })
        })
        .collect();
    net(&counted, since)
}

/// The tree's sections in order, each with the kinds it holds.
const SECTIONS: [(&str, Kind); 5] = [
    ("PLANS", Kind::Audit),
    ("STORIES", Kind::Story),
    ("PACKAGES", Kind::Package),
    ("CONCEPTS", Kind::Concept),
    ("CENTRAL IDEAS", Kind::Idea),
];

/// The open plans, stories, packages, concepts and ideas, each with its progress, opened ones showing
/// their children beneath them.
#[must_use]
pub fn plan_rows(board: &Board, open: &std::collections::BTreeSet<String>) -> Vec<PlanRow> {
    let mut out = Vec::new();
    for (heading, kind) in SECTIONS {
        let roots = board.open_of(&[kind]);
        if roots.is_empty() {
            continue;
        }
        out.push(PlanRow {
            heading: Some(format!("{heading}  {} open", roots.len())),
            ..PlanRow::default()
        });
        for root in roots {
            push_plan(board, root, 0, open, &mut out);
        }
    }
    out
}

fn push_plan(
    board: &Board,
    item: &docket_core::rows::ItemRow,
    depth: usize,
    open: &std::collections::BTreeSet<String>,
    out: &mut Vec<PlanRow>,
) {
    let held = board.holds(item);
    let g = board.progress(item);
    let unfolded = open.contains(&item.id);
    out.push(PlanRow {
        heading: None,
        id: item.id.clone(),
        depth,
        word: board.word(item),
        title: item.title.clone(),
        done: g.done,
        total: g.total,
        live: g.live,
        folded: (!held.is_empty()).then_some(!unfolded),
    });
    if !unfolded || depth > 3 {
        return;
    }
    let mut kids: Vec<&docket_core::rows::ItemRow> = match board.kind(item) {
        Kind::Concept | Kind::Idea => board.tied(item.rid),
        _ => board.children(item.rid),
    };
    kids.sort_by(|a, b| (&a.key, a.num).cmp(&(&b.key, b.num)));
    for kid in kids {
        push_plan(board, kid, depth + 1, open, out);
    }
}
