//! Plans, stories, packages, concepts and central ideas as a tree with progress, the selected one beside
//! it with what comes next under it.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::board::Board;
use crate::doc::{Doc, Target, seg, spot};
use crate::page::{Page, PlanRow, Plans};
use crate::source::Source;
use crate::style;
use crate::view::browser::follow;
use crate::view::project::bar;
use crate::view::{doc as page_doc, list_width};

fn row_line(r: &PlanRow, selected: bool, marked: bool, width: usize) -> Line<'static> {
    if let Some(h) = &r.heading {
        return Line::from(Span::styled(format!(" {h}"), style::bold()));
    }
    let mark = if selected {
        Modifier::REVERSED
    } else {
        Modifier::empty()
    };
    let fold = match r.folded {
        Some(true) => "+ ",
        Some(false) => "- ",
        None => "  ",
    };
    let indent = "  ".repeat(r.depth);
    let progress = if r.total > 0 {
        format!("{} {:>3}/{:<3} ", bar(r.done, r.total), r.done, r.total)
    } else {
        format!("{:<19}", r.word)
    };
    let room = width.saturating_sub(indent.len() + 2 + 7 + progress.len() + 1);
    let title: String = r.title.chars().take(room).collect();
    Line::from(vec![
        Span::raw(format!("{}{indent}{fold}", if marked { "*" } else { " " })),
        Span::styled(
            format!("{:<7}", r.id),
            style::word(&r.word).add_modifier(mark | Modifier::BOLD),
        ),
        Span::styled(progress, Style::default().add_modifier(mark)),
        Span::styled(title, Style::default().add_modifier(mark)),
    ])
}

pub fn draw<S: Source>(app: &mut App<S>, f: &mut Frame, area: Rect) {
    let [left, right] = Layout::horizontal([
        Constraint::Length(list_width(area.width) + 10),
        Constraint::Min(10),
    ])
    .areas(area);
    draw_side(app, f, right);
    let Page::Plans(p) = &mut app.page else {
        return;
    };
    let height = usize::from(left.height.saturating_sub(1));
    p.list_top = follow(p.list_top, p.sel, height.max(1));
    let width = usize::from(left.width.saturating_sub(1));
    let lines: Vec<Line> = p
        .rows
        .iter()
        .enumerate()
        .skip(p.list_top)
        .take(height)
        .map(|(i, r)| row_line(r, i == p.sel, p.marks.contains(&r.id), width))
        .collect();
    let block = Block::default()
        .borders(Borders::RIGHT)
        .title(Span::styled(" plans, packages and concepts", style::bold()));
    f.render_widget(Paragraph::new(lines).block(block), left);
}

fn draw_side<S: Source>(app: &mut App<S>, f: &mut Frame, area: Rect) {
    let d = page_doc(app);
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

/// The selected row: its head and progress, what comes next under it, and its rows below it.
#[must_use]
pub fn doc(board: Option<&Board>, p: &Plans, width: usize) -> Doc {
    let mut d = Doc::default();
    let Some(r) = p.rows.get(p.sel).filter(|r| r.heading.is_none()) else {
        d.plain(
            "nothing open here: no plan, story, package or concept",
            style::dim(),
        );
        return d;
    };
    let title: String = r
        .title
        .chars()
        .take(width.saturating_sub(r.id.len() + 2))
        .collect();
    d.line(vec![
        spot(
            r.id.clone(),
            style::word(&r.word),
            Target::Item(r.id.clone()),
        ),
        seg(format!(". {title}"), style::bold()),
    ]);
    d.line(vec![
        seg(format!("{:<9}", r.word), style::word(&r.word)),
        seg(
            format!("{} of {} done, {} live", r.done, r.total, r.live),
            Style::default(),
        ),
    ]);
    if let Some(g) = p.shown.as_ref().and_then(|s| s.progress) {
        d.plain(
            format!("members: {} of {} done, {} live", g.done, g.total, g.live),
            style::dim(),
        );
    }
    d.blank();
    next_under(&mut d, board, p, width);
    d
}

fn next_under(d: &mut Doc, board: Option<&Board>, p: &Plans, width: usize) {
    if p.next.is_empty() {
        d.plain(
            "NEXT under it: nothing an agent may take now",
            style::bold(),
        );
        return;
    }
    d.plain("NEXT under it, in queue order", style::bold());
    for r in &p.next {
        let word = board
            .and_then(|b| b.word_of(&r.id))
            .unwrap_or_else(|| r.word.clone());
        let title: String = r.title.chars().take(width.saturating_sub(18)).collect();
        d.line(vec![
            seg(" ", Style::default()),
            spot(r.id.clone(), style::word(&word), Target::Item(r.id.clone())),
            seg(
                " ".repeat(7usize.saturating_sub(r.id.len())),
                Style::default(),
            ),
            seg(format!("{word:<9}"), style::word(&word)),
            seg(title, Style::default()),
        ]);
    }
}
