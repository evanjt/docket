//! The mouse: where each hot spot, list row and pane was drawn, and what a click, the wheel or a move
//! over them does.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::app::App;
use crate::doc::{Cursor, Doc, Target};
use crate::page::Page;
use crate::source::Source;

/// Lines the wheel moves a pane at each notch.
pub const WHEEL: isize = 3;

/// What a drawn cell stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Zone {
    /// The page document's hot spot at this place among its spots.
    Spot(usize, Target),
    /// A row of the page's list.
    Row(usize),
}

/// The part of the page the wheel scrolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    List,
    Doc,
}

/// Every zone and pane of the last draw, and how many rows the list keeps in view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Map {
    pub zones: Vec<(Rect, Zone)>,
    pub panes: Vec<(Rect, Pane)>,
    pub list_rows: usize,
}

impl Map {
    #[must_use]
    pub fn zone(&self, x: u16, y: u16) -> Option<&Zone> {
        let at = Position::new(x, y);
        self.zones
            .iter()
            .find(|(r, _)| r.contains(at))
            .map(|(_, z)| z)
    }

    #[must_use]
    pub fn pane(&self, x: u16, y: u16) -> Option<(Rect, Pane)> {
        let at = Position::new(x, y);
        self.panes.iter().find(|(r, _)| r.contains(at)).copied()
    }

    /// A document drawn in an area from its line `top`: the pane, and each hot spot in view.
    pub fn doc(&mut self, doc: &Doc, area: Rect, top: usize) {
        self.panes.push((area, Pane::Doc));
        for (i, ((line, x, width), (_, target))) in
            doc.places().into_iter().zip(doc.spots()).enumerate()
        {
            let Some(y) = line
                .checked_sub(top)
                .and_then(|y| u16::try_from(y).ok())
                .filter(|y| *y < area.height)
            else {
                continue;
            };
            if width == 0 || x >= area.width {
                continue;
            }
            let rect = Rect::new(area.x + x, area.y + y, width.min(area.width - x), 1);
            self.zones.push((rect, Zone::Spot(i, target)));
        }
    }

    /// A list row drawn `height` lines tall at `line` of the list's inner area, cut at its foot.
    pub fn row(&mut self, inner: Rect, row: usize, line: usize, height: usize) {
        let Ok(y) = u16::try_from(line) else {
            return;
        };
        if y >= inner.height {
            return;
        }
        let tall = u16::try_from(height).unwrap_or(1).min(inner.height - y);
        let rect = Rect::new(inner.x, inner.y + y, inner.width, tall);
        self.zones.push((rect, Zone::Row(row)));
    }
}

/// The first row and the selected row after the wheel moves a list of `n` rows, `rows` in view, by
/// `by`: the selection kept in view.
#[must_use]
pub fn scrolled(top: usize, sel: usize, n: usize, rows: usize, by: isize) -> (usize, usize) {
    let top = top.saturating_add_signed(by).min(n.saturating_sub(rows));
    let last = (top + rows.max(1) - 1).min(n.saturating_sub(1));
    (top, sel.clamp(top, last.max(top)))
}

impl<S: Source> App<S> {
    /// One mouse event. While a line is typed only the wheel acts, so nothing typed is lost.
    pub fn mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column, m.row);
        let typing = matches!(&self.page, Page::Browser(b) if b.typing.is_some());
        let busy = typing || self.prompt.is_some();
        match m.kind {
            MouseEventKind::ScrollDown => self.wheel(x, y, WHEEL),
            MouseEventKind::ScrollUp => self.wheel(x, y, -WHEEL),
            _ if busy => {}
            MouseEventKind::Down(MouseButton::Left) => self.click(x, y),
            MouseEventKind::Down(MouseButton::Right) => {
                self.flash = None;
                self.go_back();
            }
            MouseEventKind::Moved => {
                if let Some(Zone::Spot(i, t)) = self.map.zone(x, y).cloned() {
                    self.point(i, t);
                }
            }
            _ => {}
        }
    }

    /// A left click: a hot spot opens as Enter opens it, a list row is selected or, selected, opened.
    fn click(&mut self, x: u16, y: u16) {
        let Some(zone) = self.map.zone(x, y).cloned() else {
            return;
        };
        self.flash = None;
        match zone {
            Zone::Spot(i, t) => {
                self.point(i, t);
                self.enter();
            }
            Zone::Row(row) => self.click_row(row),
        }
    }

    /// Selects the hot spot at a place among the document's spots.
    fn point(&mut self, i: usize, target: Target) {
        let c = self.cursor_mut();
        c.index = Some(i);
        c.target = Some(target);
    }

    fn click_row(&mut self, row: usize) {
        let (sel, id) = match &self.page {
            Page::Browser(b) => (b.sel, b.entries.get(row).map(|e| e.id.clone())),
            Page::Plans(p) => (p.sel, p.rows.get(row).map(|r| r.id.clone())),
            _ => return,
        };
        if sel != row {
            self.select_row(row);
        } else if let Some(id) = id.filter(|id| !id.is_empty()) {
            self.open(Target::Item(id));
        }
    }

    fn select_row(&mut self, row: usize) {
        match &mut self.page {
            Page::Browser(b) => {
                b.sel = row;
                b.cursor = Cursor::default();
                self.load_detail();
            }
            Page::Plans(p) => {
                p.sel = row;
                p.cursor = Cursor::default();
                self.load_plan();
            }
            _ => {}
        }
    }

    fn wheel(&mut self, x: u16, y: u16, by: isize) {
        match self.map.pane(x, y) {
            Some((_, Pane::List)) => self.scroll_list(by),
            Some((area, Pane::Doc)) => self.scroll(by, usize::from(area.height)),
            None => {}
        }
    }

    /// Moves the list's window, the selection carried along when it would leave it.
    fn scroll_list(&mut self, by: isize) {
        let rows = self.map.list_rows;
        let moved = match &mut self.page {
            Page::Browser(b) => {
                let before = b.sel;
                (b.list_top, b.sel) = scrolled(b.list_top, b.sel, b.entries.len(), rows, by);
                b.sel != before
            }
            Page::Plans(p) => {
                let before = p.sel;
                let (top, sel) = scrolled(p.list_top, p.sel, p.rows.len(), rows, by);
                p.list_top = top;
                p.sel = p.nearest_item(sel, top, rows).unwrap_or(before);
                p.sel != before
            }
            _ => false,
        };
        if !moved {
            return;
        }
        *self.cursor_mut() = Cursor::default();
        self.load_detail();
        self.load_plan();
    }
}

impl crate::page::Plans {
    /// The item row nearest `at` among the `rows` from `top`, headings skipped.
    #[must_use]
    pub fn nearest_item(&self, at: usize, top: usize, rows: usize) -> Option<usize> {
        (top..(top + rows).min(self.rows.len()))
            .filter(|i| self.rows[*i].heading.is_none())
            .min_by_key(|i| i.abs_diff(at))
    }
}

#[cfg(test)]
#[path = "tests/mouse.rs"]
mod tests;
