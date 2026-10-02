//! Drawing: the crumbs on top, the page, the keys or a message at the foot.

pub mod browser;
pub mod help;
pub mod home;
pub mod item;
pub mod plans;
pub mod project;
pub mod settings;

use std::collections::BTreeSet;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::doc::{Doc, Target};
use crate::mouse::Map;
use crate::page::Page;
use crate::source::Source;
use crate::style;
use crate::write::Prompt;

/// The width of a browser's or the tree's list beside its detail.
#[must_use]
pub fn list_width(total: u16) -> u16 {
    (total * 2 / 5).clamp(24, 60)
}

/// The page's document, the one Tab walks: the detail where a list stands beside it.
pub fn doc<S: Source>(app: &App<S>) -> Doc {
    let (w, _) = app.size;
    let full = usize::from(w);
    let side = usize::from(w.saturating_sub(list_width(w) + 1));
    let board = app.page.slug().and_then(|s| app.boards.get(s));
    match &app.page {
        Page::Home(h) => home::doc(&h.rows, full),
        Page::Project(p) => board.map_or_else(Doc::default, |b| {
            project::doc(b, p.data.as_ref(), full, p.all_moves)
        }),
        Page::Browser(b) => b.detail.as_ref().map_or_else(Doc::default, |d| {
            item::doc(board, d, side, crate::load::now())
        }),
        Page::Queue(q) => item::queue_doc(board, q, full),
        Page::Plans(p) => plans::doc(board, p, side),
        Page::Settings(s) => board.map_or_else(Doc::default, |b| {
            settings::doc(
                &b.project,
                full,
                &settings::Editing::of(s, app.prompt.as_ref()),
            )
        }),
        Page::Help(_) => help::doc(),
    }
}

pub fn spots<S: Source>(app: &App<S>) -> Vec<(usize, Target)> {
    doc(app).spots()
}

pub fn draw<S: Source>(app: &mut App<S>, f: &mut Frame) {
    let area = f.area();
    app.size = (area.width, area.height);
    app.map = Map::default();
    let foot_lines = footer(app, usize::from(area.width));
    let [top, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(u16::try_from(foot_lines.len()).unwrap_or(1)),
    ])
    .areas(area);
    f.render_widget(Paragraph::new(crumbs(app)).style(style::bar()), top);
    match &app.page {
        Page::Browser(_) => browser::draw(app, f, body),
        Page::Plans(_) => plans::draw(app, f, body),
        _ => draw_doc(app, f, body),
    }
    f.render_widget(Paragraph::new(foot_lines).style(style::bar()), foot);
}

/// The page's document, scrolled to keep the selected spot in view, its hot spots kept for the mouse.
pub fn draw_doc<S: Source>(app: &mut App<S>, f: &mut Frame, area: Rect) {
    let doc = doc(app);
    let spots = doc.spots();
    let cursor = app.cursor_mut();
    cursor.settle(&spots);
    cursor.follow(&spots, usize::from(area.height));
    let (index, top) = (cursor.index, cursor.top);
    let text = doc.render(index);
    f.render_widget(
        Paragraph::new(text).scroll((u16::try_from(top).unwrap_or(0), 0)),
        area,
    );
    app.map.doc(&doc, area, top);
}

/// The pages behind, the one shown in brackets, the ones ahead dimmed, and whether changes arrive.
fn crumbs<S: Source>(app: &App<S>) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    let behind = app.back.len().saturating_sub(4);
    if behind > 0 {
        spans.push(Span::raw(format!("{behind} more > ")));
    }
    for p in &app.back[behind..] {
        spans.push(Span::raw(format!("{} > ", p.title())));
    }
    spans.push(Span::styled(
        format!("[{}]", app.page.title()),
        style::bold(),
    ));
    for p in app.ahead.iter().rev() {
        spans.push(Span::styled(format!(" > {}", p.title()), style::dim()));
    }
    if let Some(n) = app.page.marks().map(BTreeSet::len).filter(|n| *n > 0) {
        spans.push(Span::styled(format!("   {n} marked"), style::bold()));
    }
    let live = if app.live {
        "   live"
    } else {
        "   no change stream: . reads again"
    };
    spans.push(Span::raw(live));
    Line::from(spans)
}

/// The most lines a message at the foot takes.
const FOOT_LINES: usize = 4;

/// The foot: a line being typed, a message wrapped to the width, or the page's keys.
fn footer<S: Source>(app: &App<S>, width: usize) -> Vec<Line<'static>> {
    if let Page::Browser(b) = &app.page
        && let Some(t) = &b.typing
    {
        return vec![Line::from(format!(
            " search: {}_   each key searches again, Enter keeps the list, Esc cancels",
            t.text
        ))];
    }
    if let Some(p) = &app.prompt {
        return vec![prompt_line(p, width)];
    }
    if let Some(f) = &app.flash {
        let mut lines: Vec<Line> = crate::doc::wrap(f, width.saturating_sub(2))
            .into_iter()
            .take(FOOT_LINES)
            .map(|l| Line::from(Span::styled(format!(" {l}"), Style::default())))
            .collect();
        if lines.is_empty() {
            lines.push(Line::from(""));
        }
        return lines;
    }
    vec![Line::from(format!(" {}", keys(&app.page)))]
}

/// The line being typed, its end kept in view, and what the keys do with it.
fn prompt_line(p: &Prompt, width: usize) -> Line<'static> {
    if let Some(choices) = p.choices() {
        let keys: Vec<String> = choices.iter().map(|(k, w)| format!("{k} {w}")).collect();
        return Line::from(format!(" {}{}   Esc cancels", p.label, keys.join("  ")));
    }
    let label = if p.label.is_empty() { ":" } else { &p.label };
    let hint = "   Enter sends, Ctrl-E opens $EDITOR, Esc cancels";
    let room = width.saturating_sub(label.chars().count() + hint.len() + 3);
    let n = p.text.chars().count();
    let shown: String = p.text.chars().skip(n.saturating_sub(room)).collect();
    Line::from(vec![
        Span::styled(format!(" {label}{shown}_"), style::bold()),
        Span::raw(hint),
    ])
}

/// The keys a page answers, in the order they are reached for.
fn keys(page: &Page) -> &'static str {
    match page {
        Page::Home(_) => "j/k move   Enter open a project   ? what everything means   q quit",
        Page::Project(_) => {
            "Tab/j/k move   Enter open   m all moves   / search   o yours   t plans   S settings   : verb   ? help   q quit"
        }
        Page::Browser(_) => {
            "j/k row   Enter open   x mark   a answer   r reply   ! priority   c rate   L link   : verb   ? help"
        }
        Page::Queue(_) => {
            "j/k next   a answer   r reply   R retry   x mark   ! priority   : verb   Esc back   ? help"
        }
        Page::Plans(_) => {
            "j/k move   Space open or close   x mark   ! priority   L link   : verb   ? help"
        }
        Page::Settings(_) => {
            "Tab/j/k fact   Enter edit it   : verb   PgUp/PgDn scroll   Esc back   q quit"
        }
        Page::Help(_) => "PgUp/PgDn scroll   Esc back   g home   q quit",
    }
}
