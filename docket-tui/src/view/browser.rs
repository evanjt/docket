//! A list beside the selected row in full.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::page::{Entry, Page};
use crate::source::Source;
use crate::style;
use crate::view::{doc, list_width};

/// The first row to draw so the selected one stays in a window of the height.
#[must_use]
pub fn follow(top: usize, sel: usize, height: usize) -> usize {
    if sel < top {
        sel
    } else if height > 0 && sel >= top + height {
        sel + 1 - height
    } else {
        top
    }
}

/// One row: a star when marked, id in its word's colour, the word, the title, and under it why it
/// stands there.
fn entry_lines(e: &Entry, selected: bool, marked: bool, width: usize) -> Vec<Line<'static>> {
    let mark = if selected {
        Modifier::REVERSED
    } else {
        Modifier::empty()
    };
    let tone = style::word(&e.word).add_modifier(mark);
    let title: String = e.title.chars().take(width.saturating_sub(18)).collect();
    let star = if marked { "*" } else { " " };
    let mut out = vec![Line::from(vec![
        Span::styled(star, style::bold()),
        Span::styled(format!("{:<7}", e.id), tone.add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:<9}", e.word), tone),
        Span::styled(title, Style::default().add_modifier(mark)),
    ])];
    if !e.note.is_empty() {
        let note: String = e
            .note
            .replace('\n', " ")
            .chars()
            .take(width.saturating_sub(7))
            .collect();
        out.push(Line::from(Span::styled(
            format!("        {note}"),
            style::dim(),
        )));
    }
    out
}

pub fn draw<S: Source>(app: &mut App<S>, f: &mut Frame, area: Rect) {
    let [left, right] = Layout::horizontal([
        Constraint::Length(list_width(area.width)),
        Constraint::Min(10),
    ])
    .areas(area);
    draw_detail(app, f, right);
    let Page::Browser(b) = &mut app.page else {
        return;
    };
    let width = usize::from(left.width.saturating_sub(1));
    let height = usize::from(left.height.saturating_sub(1)) / 2;
    b.list_top = follow(b.list_top, b.sel, height.max(1));
    let head = format!(" {} {}", b.listing.title(), b.entries.len());
    let mut lines = Vec::new();
    let rows = usize::from(left.height);
    for (i, e) in b.entries.iter().enumerate().skip(b.list_top).take(rows) {
        lines.extend(entry_lines(e, i == b.sel, b.marks.contains(&e.id), width));
    }
    if b.entries.is_empty() {
        lines.push(Line::from(Span::styled(" nothing here", style::dim())));
    }
    let block = Block::default()
        .borders(Borders::RIGHT)
        .title(Span::styled(head, style::bold()));
    f.render_widget(Paragraph::new(lines).block(block), left);
}

fn draw_detail<S: Source>(app: &mut App<S>, f: &mut Frame, area: Rect) {
    let d = doc(app);
    let spots = d.spots();
    let cursor = app.cursor_mut();
    cursor.settle(&spots);
    cursor.follow(&spots, usize::from(area.height));
    let (index, top) = (cursor.index, cursor.top);
    let pad = Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(1),
        ..area
    };
    f.render_widget(
        Paragraph::new(d.render(index)).scroll((u16::try_from(top).unwrap_or(0), 0)),
        pad,
    );
}
