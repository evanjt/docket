//! Reading what each page shows, from the source and the project's board.

use std::time::{SystemTime, UNIX_EPOCH};

use docket_core::pace::{Logged, Move, epoch, minutes, moves};
use docket_core::rows::{EventRow, Row};
use docket_core::word::Kind;

use crate::app::App;
use crate::doc::{Listing, Route};
use crate::filter::Filter;
use crate::page::{Detail, Entry, HomeRow, Page, PlanRow, ProjectData};
use crate::source::Source;
use docket_core::board::Board;

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
            if let Ok(whole) = self.source.whole(&slug, TREND_DAYS) {
                data.progress = whole.progress;
                data.trend = whole.daily.iter().map(|d| (d.opened, d.closed)).collect();
            }
            data.releases = self.source.releases(&slug).unwrap_or_default();
            data.problems = self.source.problems(&slug).unwrap_or_default();
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

    /// The rows a listing holds, each with the area its item is in, and only the filter's area when
    /// the filter names one.
    fn entries(&mut self, slug: &str, listing: &Listing) -> crate::source::Result<Vec<Entry>> {
        let mut entries = self.read_entries(slug, listing)?;
        if let Some(board) = self.board(slug) {
            for e in &mut entries {
                if let Some(i) = board.get(&e.id) {
                    e.area = board
                        .project
                        .areas
                        .name(i.area_id)
                        .unwrap_or_default()
                        .to_string();
                }
            }
        }
        if let Listing::Next(f) = listing
            && let Some(a) = &f.area
        {
            entries.retain(|e| e.area.eq_ignore_ascii_case(a));
        }
        Ok(entries)
    }

    fn read_entries(&mut self, slug: &str, listing: &Listing) -> crate::source::Result<Vec<Entry>> {
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
                    .map(|i| {
                        Entry::of_item(
                            i,
                            w.clone(),
                            board.project.releases.name(i.release_id),
                            board.project.areas.name(i.area_id),
                        )
                    })
                    .collect())
            }
            Listing::Ties(id) => self.ties(slug, id),
            Listing::Group(name) => self.source.group(slug, name).map(rows),
            Listing::Problems(kind) => {
                let problems = self.source.problems(slug)?;
                let board = self.board(slug).ok_or("no board")?;
                Ok(docket_core::check::ids(&problems, kind)
                    .iter()
                    .filter_map(|id| board.get(id))
                    .map(|i| {
                        let mut e = Entry::of_item(
                            i,
                            board.word(i),
                            board.project.releases.name(i.release_id),
                            board.project.areas.name(i.area_id),
                        );
                        e.note = docket_core::check::phrase(kind, 1);
                        e
                    })
                    .collect())
            }
        }
    }

    /// An item, then its plan, what spawned it, what it is related to, its children and what it
    /// spawned.
    fn ties(&mut self, slug: &str, id: &str) -> crate::source::Result<Vec<Entry>> {
        let shown = self.source.show(slug, id)?;
        let board = self.board(slug).ok_or("no board")?;
        let entry = |id: &str, note: &str| {
            board.get(id).map(|i| {
                let mut e = Entry::of_item(
                    i,
                    board.word(i),
                    board.project.releases.name(i.release_id),
                    board.project.areas.name(i.area_id),
                );
                e.note = note.to_string();
                e
            })
        };
        let mut out: Vec<Entry> = entry(id, "this item").into_iter().collect();
        out.extend(shown.parent.iter().filter_map(|t| entry(t, "its plan")));
        out.extend(shown.origin.iter().filter_map(|t| entry(t, "spawned it")));
        out.extend(shown.related.iter().filter_map(|t| entry(t, "related")));
        out.extend(shown.children.iter().filter_map(|t| entry(t, "under it")));
        if let Some(me) = board.get(id) {
            let spawned: Vec<String> = board.spawned(me.rid).iter().map(|c| c.id.clone()).collect();
            out.extend(spawned.iter().filter_map(|t| entry(t, "it spawned")));
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

/// The project page's rows: the moves of the window, or the newest ones when it holds none.
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
        releases: Vec::new(),
        status,
        next,
        wip,
        minutes: rows,
        stale,
        now,
        lead: None,
        machines: Vec::new(),
        problems: Vec::new(),
        progress: docket_core::metrics::Progress::default(),
        trend: Vec::new(),
    }
}

/// The days of the trend line.
pub const TREND_DAYS: u32 = 7;

/// The moves of a project's newest events, oldest first.
fn moves_of(board: &Board, recent: &[EventRow]) -> Vec<Move> {
    let ids: Vec<(i64, String, &EventRow)> = recent
        .iter()
        .rev()
        .filter_map(|e| {
            let rid = e.rid?;
            let item = board.by_rid(rid)?;
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

/// The tree's sections in order, each with the kinds it holds.
const SECTIONS: [(&str, Kind); 3] = [
    ("PLANS", Kind::Audit),
    ("STORIES", Kind::Story),
    ("PACKAGES", Kind::Package),
];

/// The open plans, stories and packages, each with its progress, opened ones showing their
/// children beneath them, then each area with its open and closed items.
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
    let areas = docket_core::metrics::area_progress(&board.project.areas, &board.subjects());
    if !areas.is_empty() {
        out.push(PlanRow {
            heading: Some(format!("AREAS  {}", areas.len())),
            ..PlanRow::default()
        });
        for a in areas {
            let live = if a.live > 0 {
                format!(", {} live", a.live)
            } else {
                String::new()
            };
            out.push(PlanRow {
                heading: Some(format!(
                    "  {}  {} open, {} done{live}",
                    a.name, a.open, a.done
                )),
                done: a.done,
                total: a.open + a.done,
                live: a.live,
                area: Some(a.name),
                ..PlanRow::default()
            });
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
    let g = board.held_progress(item);
    let unfolded = open.contains(&item.id);
    out.push(PlanRow {
        heading: None,
        id: item.id.clone(),
        depth,
        word: board.word(item),
        title: item.title.clone(),
        done: g.done,
        total: g.counted,
        live: g.live,
        folded: (!held.is_empty()).then_some(!unfolded),
        area: None,
    });
    if !unfolded || depth > 3 {
        return;
    }
    let mut kids: Vec<&docket_core::rows::ItemRow> = board.children(item.rid);
    kids.sort_by(|a, b| (&a.key, a.num).cmp(&(&b.key, b.num)));
    for kid in kids {
        push_plan(board, kid, depth + 1, open, out);
    }
}
