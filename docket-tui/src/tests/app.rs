use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

use super::*;
use crate::doc::Route;
use crate::tests_fixture::Fixture;

const WIDTHS: [u16; 2] = [80, 160];

fn app() -> App<Fixture> {
    App::new(Fixture::default())
}

/// The screen as text, one line per row, after a draw at the width.
fn screen(app: &mut App<Fixture>, width: u16) -> String {
    tall(app, width, 40)
}

fn tall(app: &mut App<Fixture>, width: u16, height: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| view::draw(app, f)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The foreground of the first cell where the text starts on screen.
fn colour_of(app: &mut App<Fixture>, width: u16, text: &str) -> Option<Color> {
    let mut term = Terminal::new(TestBackend::new(width, 120)).unwrap();
    term.draw(|f| view::draw(app, f)).unwrap();
    let buf = term.backend().buffer().clone();
    for y in 0..buf.area.height {
        let line: String = (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect();
        if let Some(x) = line.find(text) {
            return buf[(u16::try_from(x).unwrap(), y)].style().fg;
        }
    }
    None
}

fn press(app: &mut App<Fixture>, code: KeyCode) {
    app.key(KeyEvent::from(code));
}

fn typed(app: &mut App<Fixture>, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn project(app: &mut App<Fixture>, slug: &str) {
    app.open(Target::Project(slug.into()));
}

fn browse(app: &mut App<Fixture>, listing: Listing) {
    project(app, "o/p");
    app.open(Target::List(listing));
}

#[test]
fn test_home_lists_every_project_with_its_mode_and_owner_queue() {
    for w in WIDTHS {
        let s = screen(&mut app(), w);
        assert!(s.contains("[home]"), "{s}");
        assert!(s.contains("o/p") && s.contains("o/q"), "{s}");
        assert!(s.contains("pause") && s.contains("run"), "{s}");
        assert!(s.contains("YOURS"), "{s}");
    }
    let s = screen(&mut app(), 160);
    assert!(
        s.contains("ready") && s.contains("building") && s.contains("done"),
        "{s}"
    );
}

#[test]
fn test_home_enter_opens_the_selected_project() {
    let mut a = app();
    press(&mut a, KeyCode::Char('j'));
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.page.slug(), Some("o/q"));
}

#[test]
fn test_project_head_names_each_missing_fact() {
    for w in WIDTHS {
        let mut a = app();
        project(&mut a, "o/p");
        let s = screen(&mut a, w);
        assert!(s.contains("o/p   PAUSE   jobs 1 of 2 slots"), "{s}");
        assert!(
            s.contains("Nothing is worked: mode is pause, no land"),
            "{s}"
        );
        // At 80 columns the line wraps rather than losing its end.
        assert!(s.contains("no model_plan. S shows the settings."), "{s}");
    }
    let mut a = app();
    project(&mut a, "o/p");
    let s = screen(&mut a, 160);
    assert!(
        s.contains("no model_build, no model_review, no model_plan. S shows the settings."),
        "{s}"
    );
}

#[test]
fn test_project_head_says_nothing_when_the_facts_let_the_loop_run() {
    let mut a = app();
    project(&mut a, "o/q");
    let s = screen(&mut a, 160);
    assert!(s.contains("o/q   RUN"), "{s}");
    assert!(!s.contains("Nothing is worked"), "{s}");
}

#[test]
fn test_project_shows_flow_lists_next_jobs_packages_and_moves() {
    for w in WIDTHS {
        let mut a = app();
        project(&mut a, "o/p");
        let s = screen(&mut a, w);
        assert!(
            s.contains("ready 1 > building 1 > checking 0 > done 1"),
            "{s}"
        );
        assert!(s.contains("LISTS  next yours 1 questions 1"), "{s}");
        assert!(s.contains("NEXT  what a free slot takes"), "{s}");
        assert!(s.contains(" T3 "), "{s}");
        assert!(s.contains("JOBS  1 claimed") && s.contains("T2"), "{s}");
        assert!(s.contains("PK1    #####.....    1/2     1"), "{s}");
        assert!(s.contains("claimed: T2"), "{s}");
    }
}

#[test]
fn test_flow_count_opens_its_tickets_in_the_browser() {
    let mut a = app();
    project(&mut a, "o/p");
    press(&mut a, KeyCode::Tab);
    press(&mut a, KeyCode::Enter);
    let s = screen(&mut a, 160);
    assert!(s.contains("[ready]"), "{s}");
    assert!(s.contains("ready 1"), "{s}");
    assert!(s.contains("T3. Fix the bun cache"), "{s}");
}

#[test]
fn test_browser_lists_beside_the_selected_item_in_full() {
    for w in WIDTHS {
        let mut a = app();
        browse(&mut a, Listing::Word("ready".into()));
        let s = screen(&mut a, w);
        assert!(s.contains("T3     ready"), "{s}");
        assert!(s.contains("The cache in PK1 forgets T1's fix."), "{s}");
        assert!(s.contains("Log, 1 events:"), "{s}");
    }
}

#[test]
fn test_browser_ties_list_the_item_then_what_opened_it_and_what_it_opened() {
    let mut a = app();
    browse(&mut a, Listing::Ties("PK1".into()));
    let ids: Vec<String> = match &a.page {
        Page::Browser(b) => b
            .entries
            .iter()
            .map(|e| format!("{} {}", e.id, e.note))
            .collect(),
        _ => panic!("not a browser"),
    };
    assert_eq!(
        ids,
        [
            "PK1 this item",
            "A1 opened it",
            "T1 it opened",
            "T2 it opened"
        ]
    );
    let s = screen(&mut a, 160);
    assert!(s.contains("members: 1 of 2 done, 1 live"), "{s}");
}

#[test]
fn test_opening_a_linked_id_and_going_back_restores_the_view_and_its_selection() {
    let mut a = app();
    browse(&mut a, Listing::Route(Route::Questions));
    press(&mut a, KeyCode::Tab);
    press(&mut a, KeyCode::Tab);
    let before = a.cursor().selected().cloned();
    assert_eq!(before, Some(Target::Item("B1".into())));
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.page.title(), "B1");
    let s = screen(&mut a, 80);
    assert!(s.contains("B1. Buns vanish after a wipe"), "{s}");

    press(&mut a, KeyCode::Esc);
    assert_eq!(a.page.title(), "questions");
    assert_eq!(a.cursor().selected().cloned(), before);
    let s = screen(&mut a, 80);
    assert!(s.contains("Q1. Which store holds the buns"), "{s}");
    assert!(
        s.contains("> B1"),
        "the page ahead shows in the crumbs: {s}"
    );

    press(&mut a, KeyCode::Char('f'));
    assert_eq!(a.page.title(), "B1");
}

#[test]
fn test_going_back_restores_the_list_row() {
    let mut a = app();
    browse(&mut a, Listing::Ties("PK1".into()));
    press(&mut a, KeyCode::Char('j'));
    press(&mut a, KeyCode::Char('j'));
    let sel = |a: &App<Fixture>| match &a.page {
        Page::Browser(b) => b.selected().map(|e| e.id.clone()),
        _ => None,
    };
    assert_eq!(sel(&a).as_deref(), Some("T1"));
    a.open(Target::Item("T3".into()));
    press(&mut a, KeyCode::Char('h'));
    assert_eq!(sel(&a).as_deref(), Some("T1"));
}

#[test]
fn test_search_follows_each_key_and_esc_restores_the_list() {
    let mut a = app();
    browse(&mut a, Listing::Word("ready".into()));
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "bun");
    let s = screen(&mut a, 80);
    assert!(s.contains("search bun 5"), "{s}");
    assert!(s.contains("search: bun_"), "{s}");
    press(&mut a, KeyCode::Backspace);
    typed(&mut a, "e c");
    let s = screen(&mut a, 80);
    assert!(s.contains("search bun c 2"), "{s}");
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.page.title(), "ready");
}

#[test]
fn test_search_from_the_project_opens_a_browser() {
    let mut a = app();
    project(&mut a, "o/p");
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "wipe");
    press(&mut a, KeyCode::Enter);
    let s = screen(&mut a, 160);
    assert!(
        s.contains("[search wipe]") && s.contains("B1. Buns vanish after a wipe"),
        "{s}"
    );
}

#[test]
fn test_owner_queue_shows_one_item_at_a_time_with_its_whole_body() {
    for w in WIDTHS {
        let mut a = app();
        project(&mut a, "o/p");
        press(&mut a, KeyCode::Char('o'));
        let s = screen(&mut a, w);
        assert!(s.contains("YOURS  1 of 1"), "{s}");
        assert!(s.contains("Q1. Which store holds the buns"), "{s}");
        assert!(s.contains("asked: pick one"), "{s}");
        assert!(s.contains("The owner picks one; B1 waits on it."), "{s}");
    }
}

#[test]
fn test_plans_show_each_tree_with_progress_and_next_under_it() {
    for w in WIDTHS {
        let mut a = app();
        project(&mut a, "o/p");
        press(&mut a, KeyCode::Char('t'));
        let s = screen(&mut a, w);
        assert!(s.contains("PLANS  1 open"), "{s}");
        assert!(s.contains("+ A1     ###.......   1/4"), "{s}");
        assert!(
            s.contains("PACKAGES  1 open") && s.contains("CONCEPTS  1 open"),
            "{s}"
        );
        assert!(s.contains("NEXT under it, in queue order"), "{s}");
    }
}

#[test]
fn test_plans_space_opens_a_row_s_children() {
    let mut a = app();
    project(&mut a, "o/p");
    press(&mut a, KeyCode::Char('t'));
    press(&mut a, KeyCode::Char(' '));
    let s = screen(&mut a, 160);
    assert!(s.contains("- A1"), "{s}");
    assert!(s.contains("+ PK1") && s.contains("T3     ready"), "{s}");
    press(&mut a, KeyCode::Char('j'));
    press(&mut a, KeyCode::Char(' '));
    let s = screen(&mut a, 160);
    assert!(s.contains("T2     building"), "{s}");
}

#[test]
fn test_settings_list_every_fact_and_mark_what_the_loop_lacks_in_red() {
    for w in WIDTHS {
        let mut a = app();
        project(&mut a, "o/p");
        press(&mut a, KeyCode::Char('S'));
        let s = tall(&mut a, w, 120);
        assert!(s.contains("SETTINGS  o/p"), "{s}");
        assert!(
            s.contains("Nothing is worked: mode is pause, no land"),
            "{s}"
        );
        assert!(s.contains("  owner           the owner (default)"), "{s}");
        assert!(s.contains("  land            (not set)"), "{s}");
        assert!(
            s.contains("the command that lands a collected branch"),
            "{s}"
        );
        assert_eq!(colour_of(&mut a, w, "  land   "), Some(Color::Red));
        assert_ne!(colour_of(&mut a, w, "  owner  "), Some(Color::Red));
    }
}

#[test]
fn test_settings_need_a_project() {
    let mut a = app();
    press(&mut a, KeyCode::Char('S'));
    assert_eq!(a.page.title(), "home");
    assert!(screen(&mut a, 80).contains("open a project first"));
}

#[test]
fn test_help_explains_pages_words_and_keys() {
    for w in WIDTHS {
        let mut a = app();
        press(&mut a, KeyCode::Char('?'));
        let s = screen(&mut a, w);
        assert!(s.contains("What this screen shows"), "{s}");
        assert!(s.contains("Settings (S)"), "{s}");
        assert!(s.contains("  parked     waiting on you"), "{s}");
    }
    let mut a = app();
    press(&mut a, KeyCode::Char('?'));
    assert_eq!(
        colour_of(&mut a, 80, "parked     waiting"),
        Some(Color::Magenta)
    );
}

#[test]
fn test_a_change_reads_again_and_keeps_the_selection() {
    let mut a = app();
    browse(&mut a, Listing::Ties("PK1".into()));
    press(&mut a, KeyCode::Char('j'));
    a.source.items.lock().unwrap()[0].title = "First member, renamed".into();
    let s = screen(&mut a, 160);
    assert!(!s.contains("renamed"), "{s}");
    a.changed();
    let s = screen(&mut a, 160);
    assert!(s.contains("renamed"), "{s}");
    match &a.page {
        Page::Browser(b) => assert_eq!(b.selected().map(|e| e.id.as_str()), Some("A1")),
        _ => panic!("not a browser"),
    }
}

#[test]
fn test_words_and_verbs_take_their_colours() {
    let mut a = app();
    project(&mut a, "o/p");
    assert_eq!(colour_of(&mut a, 160, "ready 1"), Some(Color::Green));
    assert_eq!(colour_of(&mut a, 160, "building 1"), Some(Color::Yellow));
    // claimed leads to building, so it takes building's yellow.
    assert_eq!(colour_of(&mut a, 160, "claimed:"), Some(Color::Yellow));
    assert_eq!(
        colour_of(&mut a, 160, "Nothing is worked"),
        Some(Color::Red)
    );
}

#[test]
fn test_q_quits() {
    let mut a = app();
    press(&mut a, KeyCode::Char('q'));
    assert!(a.quit);
}

#[test]
fn test_machines_count_the_jobs_on_each_host_of_the_pool() {
    let mut a = app();
    project(&mut a, "o/p");
    let s = screen(&mut a, 80);
    assert!(s.contains("MACHINES  local 1 of 2 slots"), "{s}");
}

#[test]
fn test_a_group_name_in_the_detail_opens_the_group() {
    let mut a = app();
    browse(&mut a, Listing::Ties("B1".into()));
    let spots = view::spots(&a);
    let at = spots
        .iter()
        .position(|(_, t)| *t == Target::List(Listing::Group("buns".into())))
        .unwrap();
    for _ in 0..=at {
        press(&mut a, KeyCode::Tab);
    }
    press(&mut a, KeyCode::Enter);
    let s = screen(&mut a, 80);
    assert!(
        s.contains("[group buns]") && s.contains("group buns 1"),
        "{s}"
    );
}
