use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, MouseEvent, MouseEventKind};

use super::*;
use crate::tests_fixture::Fixture;

struct Queue(VecDeque<Event>);

impl Events for Queue {
    fn poll(&mut self, _wait: Duration) -> io::Result<bool> {
        Ok(!self.0.is_empty())
    }

    fn read(&mut self) -> io::Result<Event> {
        Ok(self.0.pop_front().expect("poll said an event was waiting"))
    }
}

fn moved(column: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column,
        row: 3,
        modifiers: crossterm::event::KeyModifiers::NONE,
    })
}

#[test]
fn test_a_burst_of_motion_then_a_key_is_one_draw_and_the_key_lands() {
    let mut app = App::new(Fixture::default());
    let mut events: VecDeque<Event> = (0..100).map(moved).collect();
    events.push_back(Event::Key(KeyEvent::from(KeyCode::Char('q'))));
    let mut events = Queue(events);
    assert!(step(&mut app, &mut events, Duration::ZERO).unwrap());
    assert!(app.quit);
    assert!(events.0.is_empty());
}

#[test]
fn test_motion_over_nothing_asks_for_no_draw() {
    let mut app = App::new(Fixture::default());
    let mut events = Queue((0..100).map(moved).collect());
    assert!(!step(&mut app, &mut events, Duration::ZERO).unwrap());
    assert!(events.0.is_empty());
}

#[test]
fn test_motion_off_a_hovered_zone_asks_for_a_draw() {
    let mut app = App::new(Fixture::default());
    app.hover = Some(crate::mouse::Zone::Row(0));
    let mut events = Queue((0..3).map(moved).collect());
    assert!(step(&mut app, &mut events, Duration::ZERO).unwrap());
    assert_eq!(app.hover, None);
}

fn click_at(column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    })
}

#[test]
fn test_a_click_queued_behind_a_click_that_moved_the_page_is_dropped() {
    let mut app = App::new(Fixture::default());
    let layout = app.map.layout;
    let mut events = Queue(VecDeque::from([click_at(1, 1), click_at(1, 1)]));
    step(&mut app, &mut events, Duration::ZERO).unwrap();
    assert!(app.map.layout > layout);
    assert!(events.0.is_empty());
}

#[test]
fn test_the_first_hello_after_a_read_is_not_a_change() {
    let mut app = App::new(Fixture::default());
    let (tx, rx) = mpsc::channel();
    tx.send(Notice::Live).unwrap();
    assert!(!drain(&rx, &mut app, false));
    assert!(app.live);
    tx.send(Notice::Down).unwrap();
    tx.send(Notice::Live).unwrap();
    assert!(drain(&rx, &mut app, false));
}

#[test]
fn test_a_reload_reads_the_page_apart_from_the_screen_and_hands_it_back() {
    let mut scratch = App::unread(Fixture::default());
    let page = Page::Project(crate::page::Project {
        slug: "o/p".into(),
        data: None,
        cursor: crate::doc::Cursor::default(),
        all_moves: false,
    });
    let read = scratch.reload(page);
    assert!(matches!(&read.page, Page::Project(p) if p.data.is_some()));
    assert!(read.boards.contains_key("o/p"));

    let mut shown = App::new(Fixture::default());
    shown.adopt(read);
    assert!(matches!(&shown.page, Page::Project(p) if p.data.is_some()));
}
