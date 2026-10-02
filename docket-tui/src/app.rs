//! The screen's state: the page it is on, the pages behind and ahead, and what it has read.

use std::collections::{BTreeSet, HashMap};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::doc::{Cursor, Listing, Target};
use crate::mouse::Map;
use crate::page::{Browser, Detail, Help, Home, Page, Plans, Project, Queue, Settings, Typing};
use crate::source::Source;
use crate::view;
use crate::write::{Ask, Prompt};
use docket_core::board::Board;

pub struct App<S: Source> {
    pub source: S,
    pub page: Page,
    pub back: Vec<Page>,
    pub ahead: Vec<Page>,
    pub boards: HashMap<String, Board>,
    pub details: HashMap<(String, String), Detail>,
    pub flash: Option<String>,
    /// Whether the server's change stream is connected.
    pub live: bool,
    pub quit: bool,
    /// The terminal's width and height at the last draw.
    pub size: (u16, u16),
    /// The line being typed for a write.
    pub prompt: Option<Prompt>,
    /// Text for the runner to open in `$EDITOR`, handed back through `edited`.
    pub editor: Option<String>,
    /// Where the last draw put each hot spot, list row and pane, for the mouse.
    pub map: Map,
}

impl<S: Source> App<S> {
    /// Opens on home, read at once.
    pub fn new(source: S) -> Self {
        let mut app = Self {
            source,
            page: Page::Home(Home::default()),
            back: Vec::new(),
            ahead: Vec::new(),
            boards: HashMap::new(),
            details: HashMap::new(),
            flash: None,
            live: false,
            quit: false,
            size: (120, 40),
            prompt: None,
            editor: None,
            map: Map::default(),
        };
        app.load();
        app
    }

    /// The board of a project, read when not held.
    pub fn board(&mut self, slug: &str) -> Option<&Board> {
        if !self.boards.contains_key(slug) {
            match self.source.board(slug) {
                Ok(b) => {
                    self.boards.insert(slug.to_string(), b);
                }
                Err(e) => {
                    self.flash = Some(e);
                    return None;
                }
            }
        }
        self.boards.get(slug)
    }

    /// Moves to a page, the one left behind kept for going back.
    pub fn go(&mut self, page: Page) {
        let left = std::mem::replace(&mut self.page, page);
        self.back.push(left);
        self.ahead.clear();
        self.load();
    }

    pub fn go_back(&mut self) {
        if let Some(prev) = self.back.pop() {
            let left = std::mem::replace(&mut self.page, prev);
            self.ahead.push(left);
            self.load();
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(next) = self.ahead.pop() {
            let left = std::mem::replace(&mut self.page, next);
            self.back.push(left);
            self.load();
        }
    }

    /// The database moved: forget what was read and read the open page again, pointers kept.
    pub fn changed(&mut self) {
        self.boards.clear();
        self.details.clear();
        self.load();
    }

    /// What a hot spot opens, in the project the page is in.
    pub fn open(&mut self, target: Target) {
        let slug = self.page.slug().map(str::to_string);
        let page = match (target, slug) {
            (Target::Project(slug), _) => Page::Project(Project {
                slug,
                data: None,
                cursor: Cursor::default(),
                all_moves: false,
            }),
            (Target::Item(id), Some(slug)) => Page::Browser(Browser::new(&slug, Listing::Ties(id))),
            (Target::List(listing), Some(slug)) => Page::Browser(Browser::new(&slug, listing)),
            _ => return,
        };
        self.go(page);
    }

    /// A page of the project the screen is in, or a word in the footer when it is in none.
    fn in_project(&mut self, make: impl FnOnce(String) -> Page) {
        match self.page.slug() {
            Some(slug) => {
                let page = make(slug.to_string());
                self.go(page);
            }
            None => self.flash = Some("open a project first: Enter on one".into()),
        }
    }

    pub fn key(&mut self, k: KeyEvent) {
        if k.kind != KeyEventKind::Press {
            return;
        }
        self.flash = None;
        if self.prompting(k) || self.typed(k) {
            return;
        }
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if !self.global(k.code) {
            self.local(k.code);
        }
    }

    /// The keys every page answers the same way.
    fn global(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.go(Page::Help(Help::default())),
            KeyCode::Char('g') => self.go(Page::Home(Home::default())),
            KeyCode::Char('S') => self.open_settings(),
            KeyCode::Char('o') => self.in_project(|slug| {
                Page::Queue(Queue {
                    slug,
                    rows: Vec::new(),
                    at: 0,
                    cursor: Cursor::default(),
                    marks: BTreeSet::new(),
                })
            }),
            KeyCode::Char('t') => self.in_project(|slug| Page::Plans(Plans::new(slug))),
            KeyCode::Char('/') => self.search(),
            KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                self.go_back();
            }
            KeyCode::Char('f') | KeyCode::Right => self.go_forward(),
            KeyCode::Char('.') => self.changed(),
            _ => return false,
        }
        true
    }

    /// The project's settings, every fact editable in place.
    pub fn open_settings(&mut self) {
        self.in_project(|slug| {
            Page::Settings(Settings {
                slug,
                cursor: Cursor::default(),
                refused: None,
            })
        });
    }

    /// Enter on a fact: its value typed in place, starting from what is set.
    fn edit_fact(&mut self, key: String) {
        let slug = self.page.slug().unwrap_or_default().to_string();
        let set = self
            .boards
            .get(&slug)
            .and_then(|b| b.project.skills.get(&key))
            .cloned()
            .unwrap_or_default();
        self.start_prompt(&format!("{key} = "), Ask::Fact(key), set);
    }

    /// Starts a search: in the browser it replaces the list, elsewhere it opens one.
    fn search(&mut self) {
        if let Page::Browser(b) = &mut self.page {
            let before = b.listing.clone();
            let text = match &before {
                Listing::Search(q) => q.clone(),
                _ => String::new(),
            };
            b.typing = Some(Typing { text, before });
            return;
        }
        let Some(slug) = self.page.slug().map(str::to_string) else {
            self.flash = Some("open a project first: Enter on one".into());
            return;
        };
        let mut b = Browser::new(&slug, Listing::Search(String::new()));
        b.typing = Some(Typing {
            text: String::new(),
            before: Listing::Search(String::new()),
        });
        self.go(Page::Browser(b));
    }

    /// One key while a search is typed: each change asks `/search` again.
    fn typed(&mut self, k: KeyEvent) -> bool {
        let Page::Browser(b) = &mut self.page else {
            return false;
        };
        let Some(t) = &mut b.typing else {
            return false;
        };
        match k.code {
            KeyCode::Enter => b.typing = None,
            KeyCode::Esc => {
                b.listing = t.before.clone();
                b.typing = None;
            }
            KeyCode::Backspace => {
                t.text.pop();
                b.listing = Listing::Search(t.text.clone());
            }
            KeyCode::Char(c) => {
                t.text.push(c);
                b.listing = Listing::Search(t.text.clone());
            }
            _ => return true,
        }
        b.sel = 0;
        b.list_top = 0;
        b.cursor = Cursor::default();
        self.load();
        true
    }

    /// The keys whose meaning depends on the page.
    fn local(&mut self, code: KeyCode) {
        if self.write_key(code) {
            return;
        }
        let height = self.body_height();
        match code {
            KeyCode::Tab => self.step_spot(true),
            KeyCode::BackTab => self.step_spot(false),
            KeyCode::Char('j') | KeyCode::Down => self.step_row(true),
            KeyCode::Char('k') | KeyCode::Up => self.step_row(false),
            KeyCode::PageDown => self.scroll(isize::try_from(height).unwrap_or(10) - 2, height),
            KeyCode::PageUp => self.scroll(2 - isize::try_from(height).unwrap_or(10), height),
            KeyCode::Enter | KeyCode::Char('l') => self.enter(),
            KeyCode::Char(' ') => self.fold(),
            KeyCode::Char('m') => {
                if let Page::Project(p) = &mut self.page {
                    p.all_moves = !p.all_moves;
                }
            }
            _ => {}
        }
    }

    pub fn body_height(&self) -> usize {
        usize::from(self.size.1.saturating_sub(2))
    }

    /// Tab: the next hot spot of the page's document, the detail where there is a list beside it.
    fn step_spot(&mut self, forward: bool) {
        let spots = view::spots(self);
        let height = self.body_height();
        let c = self.cursor_mut();
        c.step(&spots, forward);
        c.follow(&spots, height);
    }

    /// j and k: the next row of a list, or the next hot spot where the page is one document.
    fn step_row(&mut self, forward: bool) {
        match &mut self.page {
            Page::Browser(b) => {
                if b.entries.is_empty() {
                    return;
                }
                b.sel = shift(b.sel, b.entries.len(), forward);
                b.cursor = Cursor::default();
                self.load_detail();
            }
            Page::Plans(p) => {
                p.sel = p.step(forward);
                p.cursor = Cursor::default();
                self.load_plan();
            }
            Page::Queue(q) => {
                if q.rows.is_empty() {
                    return;
                }
                q.at = shift(q.at, q.rows.len(), forward);
                q.cursor = Cursor::default();
            }
            _ => self.step_doc(forward),
        }
    }

    /// On a page that is one document: the next hot spot, or a line of scroll where it has none.
    fn step_doc(&mut self, forward: bool) {
        if view::spots(self).is_empty() {
            self.scroll(if forward { 1 } else { -1 }, self.body_height());
        } else {
            self.step_spot(forward);
        }
    }

    /// Scrolls the page's document in a window of the height, letting go of a spot it leaves behind.
    pub(crate) fn scroll(&mut self, by: isize, height: usize) {
        let doc = view::doc(self);
        let cursor = self.cursor_mut();
        cursor.scroll(by, doc.lines.len(), height);
        cursor.release(&doc.spots(), height);
    }

    /// Enter: the selected hot spot, or in a list the selected row when no spot is chosen.
    pub(crate) fn enter(&mut self) {
        let chosen = self.cursor().selected().cloned();
        if let Some(Target::Fact(key)) = chosen {
            self.edit_fact(key);
            return;
        }
        if let Some(t) = chosen {
            self.open(t);
            return;
        }
        let row = match &self.page {
            Page::Plans(p) => p.rows.get(p.sel).map(|r| r.id.clone()),
            Page::Queue(q) => q.rows.get(q.at).map(|r| r.id.clone()),
            _ => None,
        };
        if let Some(id) = row.filter(|id| !id.is_empty()) {
            self.open(Target::Item(id));
        }
    }

    /// Space in the plans tree: open or close the selected row's children.
    fn fold(&mut self) {
        if let Page::Plans(p) = &mut self.page
            && let Some(row) = p.rows.get(p.sel)
            && row.folded.is_some()
        {
            let id = row.id.clone();
            if !p.open.remove(&id) {
                p.open.insert(id);
            }
            self.load();
        }
    }

    pub fn cursor_mut(&mut self) -> &mut Cursor {
        match &mut self.page {
            Page::Home(p) => &mut p.cursor,
            Page::Project(p) => &mut p.cursor,
            Page::Browser(p) => &mut p.cursor,
            Page::Queue(p) => &mut p.cursor,
            Page::Plans(p) => &mut p.cursor,
            Page::Settings(p) => &mut p.cursor,
            Page::Help(p) => &mut p.cursor,
        }
    }

    #[must_use]
    pub fn cursor(&self) -> &Cursor {
        match &self.page {
            Page::Home(p) => &p.cursor,
            Page::Project(p) => &p.cursor,
            Page::Browser(p) => &p.cursor,
            Page::Queue(p) => &p.cursor,
            Page::Plans(p) => &p.cursor,
            Page::Settings(p) => &p.cursor,
            Page::Help(p) => &p.cursor,
        }
    }
}

/// One step through `n` rows, wrapping round.
fn shift(at: usize, n: usize, forward: bool) -> usize {
    if forward {
        (at + 1) % n
    } else {
        (at + n - 1) % n
    }
}

impl Plans {
    #[must_use]
    pub fn new(slug: String) -> Self {
        Self {
            slug,
            rows: Vec::new(),
            open: BTreeSet::new(),
            sel: 0,
            list_top: 0,
            shown: None,
            next: Vec::new(),
            cursor: Cursor::default(),
            marks: BTreeSet::new(),
        }
    }

    /// The next row that is an item, headings skipped.
    #[must_use]
    pub fn step(&self, forward: bool) -> usize {
        let n = self.rows.len();
        let mut at = self.sel;
        for _ in 0..n {
            at = shift(at, n, forward);
            if self.rows[at].heading.is_none() {
                return at;
            }
        }
        self.sel
    }
}

#[cfg(test)]
#[path = "tests/app.rs"]
mod tests;
