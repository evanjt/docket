use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use super::*;
use crate::doc::{Listing, Route};
use crate::tests_fixture::Fixture;
use crate::view::{self, list_width};
use crate::write::Ask;

fn app() -> App<Fixture> {
    App::new(Fixture::default())
}

fn draw(app: &mut App<Fixture>, width: u16, height: u16) -> Buffer {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| view::draw(app, f)).unwrap();
    term.backend().buffer().clone()
}

/// The first cell, reading down, where the text starts between columns `from` and `to`.
fn find(buf: &Buffer, text: &str, from: u16, to: u16) -> (u16, u16) {
    let chars: Vec<String> = text.chars().map(String::from).collect();
    let n = u16::try_from(chars.len()).unwrap();
    for y in 0..buf.area.height {
        for x in from..to.min(buf.area.width).saturating_sub(n - 1) {
            if (0..n).all(|k| buf[(x + k, y)].symbol() == chars[usize::from(k)]) {
                return (x, y);
            }
        }
    }
    panic!("{text} is not on screen");
}

fn event(app: &mut App<Fixture>, kind: MouseEventKind, (column, row): (u16, u16)) {
    app.mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

fn click(app: &mut App<Fixture>, at: (u16, u16)) {
    event(app, MouseEventKind::Down(MouseButton::Left), at);
}

fn browse(app: &mut App<Fixture>, listing: Listing) {
    app.open(Target::Project("o/p".into()));
    app.open(Target::List(listing));
}

/// The cell of an id in the detail beside a browser's list, drawn at 160 by 40.
fn in_detail(app: &mut App<Fixture>, text: &str) -> (u16, u16) {
    let buf = draw(app, 160, 40);
    find(&buf, text, list_width(160) + 1, 160)
}

fn row_of(app: &App<Fixture>) -> (usize, usize) {
    match &app.page {
        Page::Browser(b) => (b.sel, b.list_top),
        Page::Plans(p) => (p.sel, p.list_top),
        _ => panic!("no list"),
    }
}

#[test]
fn test_mouse_click_on_a_drawn_id_opens_it() {
    let mut a = app();
    browse(&mut a, Listing::Route(Route::Questions));
    let at = in_detail(&mut a, "B1");
    click(&mut a, at);
    assert_eq!(a.page.title(), "B1");
}

#[test]
fn test_mouse_click_on_a_count_opens_its_list() {
    let mut a = app();
    a.open(Target::Project("o/p".into()));
    let buf = draw(&mut a, 160, 40);
    let at = find(&buf, "ready 1", 0, 160);
    click(&mut a, (at.0 + 6, at.1));
    assert_eq!(a.page.title(), "ready");
}

#[test]
fn test_mouse_click_on_empty_space_changes_nothing() {
    let mut a = app();
    a.open(Target::Project("o/p".into()));
    let buf = draw(&mut a, 160, 40);
    assert_eq!(buf[(159, 1)].symbol(), " ");
    let before = (a.page.title(), a.back.len(), a.cursor().clone());
    click(&mut a, (159, 1));
    click(&mut a, (40, 0));
    assert_eq!((a.page.title(), a.back.len(), a.cursor().clone()), before);
}

#[test]
fn test_mouse_wheel_over_the_side_pane_scrolls_it_and_not_the_list() {
    let mut a = app();
    browse(&mut a, Listing::Ties("PK1".into()));
    draw(&mut a, 160, 12);
    event(&mut a, MouseEventKind::ScrollDown, (100, 5));
    assert_eq!(a.cursor().top, 3);
    assert_eq!(row_of(&a), (0, 0));
    draw(&mut a, 160, 12);
    assert_eq!(a.cursor().top, 3, "the next draw keeps the scroll");
    event(&mut a, MouseEventKind::ScrollUp, (5, 5));
    assert_eq!(a.cursor().top, 3, "the wheel over the list leaves the side");
}

#[test]
fn test_mouse_wheel_over_the_list_moves_its_window_and_keeps_the_selection_in_it() {
    let mut a = app();
    a.open(Target::Project("o/p".into()));
    a.key(KeyEvent::from(KeyCode::Char('t')));
    let rows = match &a.page {
        Page::Plans(p) => p.rows.len(),
        _ => panic!("not the plans"),
    };
    // A body of four lines under the crumbs and foot: three rows below the list's title.
    draw(&mut a, 160, 6);
    assert!(rows > 3, "the tree outgrows the window");
    event(&mut a, MouseEventKind::ScrollDown, (5, 3));
    let (sel, top) = row_of(&a);
    assert_eq!(top, 3.min(rows - 3));
    assert!(sel >= top && sel < top + 3, "{sel} in {top}..");
    assert_eq!(a.cursor().top, 0);
}

#[test]
fn test_mouse_right_click_after_an_open_returns_to_the_page_and_its_selection() {
    let mut a = app();
    browse(&mut a, Listing::Route(Route::Questions));
    let at = in_detail(&mut a, "B1");
    click(&mut a, at);
    assert_eq!(a.page.title(), "B1");
    draw(&mut a, 160, 40);
    event(&mut a, MouseEventKind::Down(MouseButton::Right), (100, 10));
    assert_eq!(a.page.title(), "questions");
    assert_eq!(a.cursor().selected(), Some(&Target::Item("B1".into())));
    assert_eq!(row_of(&a), (0, 0));
}

#[test]
fn test_mouse_click_on_a_fact_starts_editing_it() {
    let mut a = app();
    a.open(Target::Project("o/p".into()));
    a.open_settings();
    let buf = draw(&mut a, 160, 120);
    let (x, y) = find(&buf, "  gates  ", 0, 160);
    click(&mut a, (x + 3, y));
    let p = a.prompt.as_ref().expect("a fact typed");
    assert_eq!(p.ask, Ask::Fact("gates".into()));
    assert_eq!(p.label, "gates = ");
    assert_eq!(a.page.title(), "settings");
}

#[test]
fn test_mouse_click_selects_a_row_and_a_second_click_opens_it() {
    let mut a = app();
    browse(&mut a, Listing::Ties("PK1".into()));
    let buf = draw(&mut a, 160, 40);
    let at = find(&buf, "T1 ", 0, list_width(160));
    click(&mut a, at);
    assert_eq!((a.page.title(), row_of(&a).0), ("PK1".into(), 2));
    draw(&mut a, 160, 40);
    click(&mut a, (at.0 + 12, at.1));
    assert_eq!(a.page.title(), "T1");
}

#[test]
fn test_mouse_click_in_the_lower_half_of_one_line_rows_keeps_the_window() {
    let mut a = app();
    browse(&mut a, Listing::Ties("PK1".into()));
    let Page::Browser(b) = &mut a.page else {
        panic!("not a browser");
    };
    b.entries = (0..80)
        .map(|i| crate::page::Entry {
            id: format!("R{i}"),
            word: "ready".into(),
            title: "t".into(),
            note: String::new(),
            area: String::new(),
            release: String::new(),
        })
        .collect();
    b.sel = 0;
    let buf = draw(&mut a, 160, 48);
    let at = find(&buf, "R30 ", 0, list_width(160));
    click(&mut a, at);
    assert_eq!(row_of(&a), (30, 0));
    let buf = draw(&mut a, 160, 48);
    assert_eq!(find(&buf, "R30 ", 0, list_width(160)), at);
    assert_eq!(row_of(&a), (30, 0));
}

#[test]
fn test_mouse_move_over_an_id_hovers_it_and_leaves_the_cursor() {
    let mut a = app();
    browse(&mut a, Listing::Route(Route::Questions));
    let at = in_detail(&mut a, "B1");
    let before = a.cursor().clone();
    event(&mut a, MouseEventKind::Moved, at);
    assert!(matches!(a.hover, Some(Zone::Spot(_, Target::Item(ref id))) if id == "B1"));
    assert_eq!(*a.cursor(), before);
    assert_eq!(a.page.title(), "questions");
    event(&mut a, MouseEventKind::Moved, (0, 0));
    assert_eq!(a.hover, None);
    assert_eq!(*a.cursor(), before);
}

#[test]
fn test_mouse_move_over_a_list_row_hovers_it() {
    let mut a = app();
    browse(&mut a, Listing::Ties("PK1".into()));
    let buf = draw(&mut a, 160, 40);
    let at = find(&buf, "T1 ", 0, list_width(160));
    let sel = row_of(&a);
    event(&mut a, MouseEventKind::Moved, at);
    assert!(matches!(a.hover, Some(Zone::Row(_))));
    assert_eq!(row_of(&a), sel);
}

#[test]
fn test_mouse_clicks_wait_while_a_line_is_typed() {
    let mut a = app();
    browse(&mut a, Listing::Route(Route::Questions));
    a.key(KeyEvent::from(KeyCode::Char('a')));
    let at = in_detail(&mut a, "B1");
    click(&mut a, at);
    event(&mut a, MouseEventKind::Down(MouseButton::Right), at);
    assert_eq!(a.page.title(), "questions");
    assert!(a.prompt.is_some());
}

#[test]
fn test_scrolled_keeps_the_selection_in_the_window() {
    // Ten rows, four in view.
    assert_eq!(scrolled(0, 0, 10, 4, 3), (3, 3));
    assert_eq!(scrolled(5, 9, 10, 4, 3), (6, 9));
    assert_eq!(scrolled(6, 9, 10, 4, -3), (3, 6));
    assert_eq!(scrolled(1, 2, 10, 4, -3), (0, 2));
    assert_eq!(scrolled(0, 0, 3, 4, 3), (0, 0));
    assert_eq!(scrolled(0, 0, 0, 4, 3), (0, 0));
}

#[test]
fn test_help_names_what_the_mouse_does() {
    let mut a = app();
    a.key(KeyEvent::from(KeyCode::Char('?')));
    let buf = draw(&mut a, 120, 120);
    find(
        &buf,
        "wheel                  scroll the pane under the pointer",
        0,
        120,
    );
    find(&buf, "right click            back", 0, 120);
}

fn click_stamped(app: &mut App<Fixture>, (column, row): (u16, u16), layout: u64) {
    app.mouse_stamped(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
        layout,
    );
}

#[test]
fn test_a_click_stamped_before_a_layout_change_is_ignored() {
    let mut a = app();
    browse(&mut a, Listing::Route(Route::Questions));
    let old_at = in_detail(&mut a, "B1");
    let before = a.map.layout;
    draw(&mut a, 120, 30);
    assert_ne!(a.map.layout, before);
    click_stamped(&mut a, old_at, before);
    assert_ne!(a.page.title(), "B1");
}

#[test]
fn test_a_click_after_a_layout_change_hits_the_new_map() {
    let mut a = app();
    browse(&mut a, Listing::Route(Route::Questions));
    in_detail(&mut a, "B1");
    let buf = draw(&mut a, 120, 30);
    let at = find(&buf, "B1", list_width(120) + 1, 120);
    let layout = a.map.layout;
    click_stamped(&mut a, at, layout);
    assert_eq!(a.page.title(), "B1");
}

#[test]
fn test_drawing_the_same_layout_again_keeps_its_generation() {
    let mut a = app();
    browse(&mut a, Listing::Route(Route::Questions));
    draw(&mut a, 160, 40);
    let layout = a.map.layout;
    draw(&mut a, 160, 40);
    assert_eq!(a.map.layout, layout);
}

#[test]
fn test_a_release_line_opens_the_queue_for_that_release() {
    let mut a = app();
    a.open(Target::Project("o/p".into()));
    let at = find(&draw(&mut a, 160, 80), "4.2.0", 0, 44);
    click(&mut a, at);
    let Page::Browser(b) = &a.page else {
        panic!("not a browser");
    };
    assert_eq!(b.listing.title(), "next release:4.2.0");
}

#[test]
fn test_a_check_line_opens_the_items_with_that_problem() {
    let mut a = app();
    a.open(Target::Project("o/p".into()));
    let at = find(&draw(&mut a, 160, 80), "1 cycle", 0, 44);
    click(&mut a, at);
    let Page::Browser(b) = &a.page else {
        panic!("not a browser");
    };
    let ids: Vec<&str> = b.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["B1"]);
}
