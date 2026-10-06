use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

use super::*;
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

fn project(app: &mut App<Fixture>, slug: &str) {
    app.open(Target::Project(slug.into()));
}

fn browse(app: &mut App<Fixture>, listing: Listing) {
    project(app, "o/p");
    app.open(Target::List(listing));
}

#[test]
fn test_home_lists_every_project_with_its_owner_queue() {
    for w in WIDTHS {
        let s = screen(&mut app(), w);
        assert!(s.contains("[home]"), "{s}");
        assert!(s.contains("o/p") && s.contains("o/q"), "{s}");
        assert!(!s.contains("MODE") && !s.contains("pause"), "{s}");
        assert!(s.contains("YOURS"), "{s}");
    }
    let s = screen(&mut app(), 160);
    assert!(
        s.contains("ready") && !s.contains("under way") && s.contains("done"),
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
fn test_project_page_shows_claims_owner_items_and_due_audits_on_one_screen() {
    for w in WIDTHS {
        let mut a = app();
        project(&mut a, "o/p");
        let s = tall(&mut a, w, 40);
        assert!(s.contains("CLAIMED NOW  1"), "{s}");
        assert!(
            s.contains("T2") && s.contains("audit/t2-1 on devbox"),
            "{s}"
        );
        assert!(s.contains("YOURS  1") && s.contains("Q1"), "{s}");
        assert!(s.contains("AUDITS DUE  1") && s.contains("A2"), "{s}");
        assert!(
            s.contains("/plan") && s.contains("/work") && s.contains("/audit"),
            "{s}"
        );
        for gone in [
            "PAUSE",
            "JOBS",
            "CHORES",
            "Nothing is worked",
            "loop",
            "pool",
        ] {
            assert!(!s.contains(gone), "{gone}: {s}");
        }
    }
}

#[test]
fn test_project_page_shows_the_lead_each_claims_job_and_the_machines_slots() {
    let mut a = app();
    *a.source.lead.lock().unwrap() = Some(docket_core::lead::Lead {
        project: "o/p".into(),
        host: "rack".into(),
        session: "lead-1".into(),
        since: "2026-10-01T08:00:00Z".into(),
        renewed_at: "2026-10-01T08:30:00Z".into(),
        ..docket_core::lead::Lead::default()
    });
    *a.source.lapsed.lock().unwrap() = true;
    project(&mut a, "o/p");
    let s = tall(&mut a, 80, 80);
    assert!(
        s.contains("LEAD") && s.contains("lapsed") && s.contains("lead-1"),
        "{s}"
    );
    assert!(s.contains("claude t2-b1"), "{s}");
    assert!(s.contains("MACHINES"), "{s}");
    assert!(s.contains("rack 1/2") && s.contains("shed 0/1"), "{s}");
}

#[test]
fn test_project_page_without_a_lead_says_none_holds_it() {
    let mut a = app();
    project(&mut a, "o/p");
    let s = tall(&mut a, 80, 80);
    assert!(s.contains("LEAD") && s.contains("none"), "{s}");
}

#[test]
fn test_project_page_puts_the_sidebar_beside_the_main_pane_when_wide() {
    let mut a = app();
    project(&mut a, "o/p");
    let wide = screen(&mut a, 160);
    assert!(
        wide.lines()
            .any(|l| l.contains("PROGRESS") && l.contains("NEXT")),
        "{wide}"
    );
    let narrow = tall(&mut a, 80, 80);
    assert!(
        !narrow
            .lines()
            .any(|l| l.contains("PROGRESS") && l.contains("NEXT")),
        "{narrow}"
    );
    assert!(narrow.contains("NEXT"), "{narrow}");
}

#[test]
fn test_project_page_shows_progress_next_plans_and_moves() {
    for w in WIDTHS {
        let mut a = app();
        project(&mut a, "o/p");
        let s = tall(&mut a, w, 80);
        assert!(s.contains("2 of 9 closed"), "{s}");
        assert!(s.contains("ready 1  in progress 1"), "{s}");
        assert!(s.contains(" T3 "), "{s}");
        assert!(s.contains("Plans: 2 open, 1 audit due"), "{s}");
        assert!(s.contains("PLANS  2 under way"), "{s}");
        assert!(s.contains("claimed: T2"), "{s}");
    }
}

#[test]
fn test_moves_show_the_last_five_and_m_shows_them_all() {
    let mut a = app();
    project(&mut a, "o/p");
    let s = tall(&mut a, 160, 80);
    assert!(s.contains("MOVES  the last 5 of 7"), "{s}");
    assert!(!s.contains("opened: T3"), "{s}");
    press(&mut a, KeyCode::Char('m'));
    let s = tall(&mut a, 160, 80);
    assert!(s.contains("opened: T3"), "{s}");
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
    assert!(s.contains("T2     in progress"), "{s}");
}

#[test]
fn test_settings_list_every_fact_with_its_value_and_meaning() {
    for w in WIDTHS {
        let mut a = app();
        project(&mut a, "o/p");
        press(&mut a, KeyCode::Char('S'));
        let s = tall(&mut a, w, 120);
        assert!(s.contains("SETTINGS  o/p"), "{s}");
        assert!(!s.contains("Nothing is worked"), "{s}");
        assert!(s.contains("  owner           the owner (default)"), "{s}");
        assert!(s.contains("  gates           (not set)"), "{s}");
        assert!(
            s.contains("the checks a job reruns after merging main"),
            "{s}"
        );
        assert!(!s.contains("  pool "), "a retired fact is not listed: {s}");
        assert_ne!(colour_of(&mut a, w, "  gates   "), Some(Color::Red));
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
        let s = tall(&mut a, w, 80);
        assert!(s.contains("What this screen shows"), "{s}");
        assert!(s.contains("Settings (S)"), "{s}");
        assert!(s.contains("  waiting on owner waiting on you"), "{s}");
    }
    let mut a = app();
    press(&mut a, KeyCode::Char('?'));
    assert_eq!(
        colour_of(&mut a, 80, "waiting on owner waiting"),
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
    assert_eq!(colour_of(&mut a, 160, "in progress 1"), Some(Color::Yellow));
    // claimed leads to in progress, so it takes in progress's yellow.
    assert_eq!(colour_of(&mut a, 160, "claimed:"), Some(Color::Yellow));
}

#[test]
fn test_q_quits() {
    let mut a = app();
    press(&mut a, KeyCode::Char('q'));
    assert!(a.quit);
}

#[test]
fn test_a_group_name_in_the_detail_opens_the_group() {
    let mut a = app();
    browse(&mut a, Listing::Ties("B1".into()));
    let spots = view::spots(&a);
    let at = spots
        .iter()
        .position(|(_, t)| *t == Target::List(Listing::Group("loaves".into())))
        .unwrap();
    for _ in 0..=at {
        press(&mut a, KeyCode::Tab);
    }
    press(&mut a, KeyCode::Enter);
    let s = screen(&mut a, 80);
    assert!(
        s.contains("[group loaves]") && s.contains("group loaves 1"),
        "{s}"
    );
}

#[test]
fn test_project_page_prints_the_forecast_of_the_metrics_body() {
    let mut a = app();
    a.source.releases.lock().unwrap()[0].forecast = docket_core::metrics::Forecast {
        open: 12,
        burn: 1.5,
        converging: true,
        p50: Some("2026-10-20".into()),
        p85: Some("2026-11-02".into()),
        ..Default::default()
    };
    project(&mut a, "o/p");
    let s = tall(&mut a, 160, 80);
    assert!(s.contains("clear by P50 2026-10-20"), "{s}");
}

#[test]
fn test_project_page_says_not_converging_for_a_zero_burn() {
    let mut a = app();
    a.source.releases.lock().unwrap()[0].forecast = docket_core::metrics::Forecast {
        open: 12,
        ..Default::default()
    };
    project(&mut a, "o/p");
    let s = tall(&mut a, 160, 80);
    assert!(s.contains("not converging"), "{s}");
}

#[test]
fn test_project_page_shows_each_release_s_progress_and_the_check_problems() {
    let mut a = app();
    project(&mut a, "o/p");
    let s = tall(&mut a, 160, 80);
    assert!(s.contains("RELEASES"), "{s}");
    assert!(
        s.contains("4.2.0") && s.contains("1 of 3 closed") && s.contains("ready 1  held later 1"),
        "{s}"
    );
    assert!(s.contains("4.3.0") && s.contains("1 of 1 closed"), "{s}");
    assert!(s.contains("CHECKS") && s.contains("1 cycle"), "{s}");
}

#[test]
fn test_a_browser_row_shows_its_release() {
    let mut a = app();
    browse(&mut a, Listing::Word("done".into()));
    let s = screen(&mut a, 160);
    assert!(s.contains("4.2.0"), "{s}");
}

#[test]
fn test_browser_rows_show_their_area_and_an_area_filter_keeps_only_its_rows() {
    let mut a = app();
    browse(&mut a, Listing::Word("blocked".into()));
    assert!(screen(&mut a, 120).contains("lanterns"));

    let ids = |a: &mut App<Fixture>, area: &str| -> Vec<String> {
        let filter = crate::filter::Filter::parse(&format!("area:{area}")).unwrap();
        browse(a, Listing::Next(Box::new(filter)));
        let Page::Browser(b) = &a.page else {
            panic!("not a browser");
        };
        b.entries.iter().map(|e| e.id.clone()).collect()
    };
    assert_eq!(ids(&mut a, "Lanterns"), ["T3"]);
    assert!(ids(&mut a, "kites").is_empty());
}
