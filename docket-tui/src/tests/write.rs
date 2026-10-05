use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use serde_json::json;

use super::*;
use crate::doc::{Listing, Route, Target};
use crate::tests_fixture::Fixture;
use crate::view;

const REFUSAL: &str = "Q1 was decided by the owner";

fn app() -> App<Fixture> {
    App::new(Fixture::default())
}

fn lines(app: &mut App<Fixture>, width: u16, height: u16) -> Vec<String> {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| view::draw(app, f)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

fn screen(app: &mut App<Fixture>) -> String {
    lines(app, 160, 40).join("\n")
}

/// Every line but the foot, where a refusal is shown.
fn body(app: &mut App<Fixture>) -> Vec<String> {
    let mut all = lines(app, 160, 40);
    all.pop();
    all
}

fn foot(app: &mut App<Fixture>) -> String {
    lines(app, 160, 40).pop().unwrap()
}

fn press(app: &mut App<Fixture>, code: KeyCode) {
    app.key(KeyEvent::from(code));
}

fn ctrl(app: &mut App<Fixture>, c: char) {
    app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
}

fn typed(app: &mut App<Fixture>, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn queue() -> App<Fixture> {
    let mut a = app();
    a.open(Target::Project("o/p".into()));
    press(&mut a, KeyCode::Char('o'));
    a
}

fn browse(listing: Listing) -> App<Fixture> {
    let mut a = app();
    a.open(Target::Project("o/p".into()));
    a.open(Target::List(listing));
    a
}

fn refuse(a: &App<Fixture>) {
    *a.source.refuse.lock().unwrap() = Some(REFUSAL.into());
}

/// The one write sent, its verb and body.
fn only(a: &App<Fixture>) -> (String, serde_json::Value) {
    let sent = a.source.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    sent[0].clone()
}

#[test]
fn test_answer_in_the_owner_queue_sends_one_answer() {
    let mut a = queue();
    press(&mut a, KeyCode::Char('a'));
    assert!(foot(&mut a).contains("answer Q1: _"), "{}", foot(&mut a));
    typed(&mut a, "Use S3");
    press(&mut a, KeyCode::Enter);
    let (verb, body) = only(&a);
    assert_eq!(verb, "answer");
    assert_eq!(
        body,
        json!({ "project": "o/p", "branch": null, "force": false, "id": "Q1", "decision": "Use S3", "derived": null })
    );
    assert!(foot(&mut a).contains("answered Q1"));
    assert!(a.prompt.is_none());
}

#[test]
fn test_a_refused_answer_changes_nothing_and_shows_the_reason() {
    let mut a = queue();
    let before = body(&mut a);
    refuse(&a);
    press(&mut a, KeyCode::Char('a'));
    typed(&mut a, "Use S3");
    press(&mut a, KeyCode::Enter);
    assert_eq!(only(&a).0, "answer");
    assert_eq!(body(&mut a), before);
    assert_eq!(foot(&mut a).trim(), REFUSAL);
}

#[test]
fn test_reply_in_the_owner_queue() {
    let mut a = queue();
    press(&mut a, KeyCode::Char('r'));
    typed(&mut a, "plugged in");
    press(&mut a, KeyCode::Enter);
    let (verb, body) = only(&a);
    assert_eq!(
        (verb.as_str(), &body["id"], &body["note"]),
        ("reply", &json!("Q1"), &json!("plugged in"))
    );
}

#[test]
fn test_ctrl_e_hands_the_line_to_the_editor_and_its_text_is_sent() {
    let mut a = queue();
    press(&mut a, KeyCode::Char('a'));
    typed(&mut a, "Use");
    ctrl(&mut a, 'e');
    assert_eq!(a.editor.as_deref(), Some("Use"));
    assert!(a.source.sent().is_empty());
    a.editor = None;
    a.edited(Some("Use S3.\n\nThe read path is hot.\n".into()));
    let (_, body) = only(&a);
    assert_eq!(body["decision"], "Use S3.\n\nThe read path is hot.");
}

#[test]
fn test_an_editor_left_empty_sends_nothing() {
    let mut a = queue();
    press(&mut a, KeyCode::Char('r'));
    ctrl(&mut a, 'e');
    a.edited(Some("  \n".into()));
    assert!(a.source.sent().is_empty());
    assert!(a.prompt.is_none());
}

#[test]
fn test_esc_drops_a_typed_line_and_leaves_the_page() {
    let mut a = queue();
    press(&mut a, KeyCode::Char('a'));
    typed(&mut a, "q");
    press(&mut a, KeyCode::Esc);
    assert!(a.prompt.is_none());
    assert_eq!(
        a.page.title(),
        "yours",
        "Esc dropped the line, not the page"
    );
    assert!(a.source.sent().is_empty());
}

#[test]
fn test_marked_rows_are_prioritised_in_one_request() {
    let mut a = browse(Listing::Ties("PK1".into()));
    press(&mut a, KeyCode::Char('j'));
    press(&mut a, KeyCode::Char('j'));
    press(&mut a, KeyCode::Char('x'));
    press(&mut a, KeyCode::Char('j'));
    press(&mut a, KeyCode::Char(' '));
    assert!(screen(&mut a).contains("2 marked"));
    assert!(screen(&mut a).contains("*T1"), "{}", screen(&mut a));
    press(&mut a, KeyCode::Char('!'));
    press(&mut a, KeyCode::Char('h'));
    let (verb, body) = only(&a);
    assert_eq!(verb, "priority");
    assert_eq!(body["ids"], json!(["T1", "T2"]));
    assert_eq!(a.selection(), ["T2"], "the marks go once the move lands");
}

#[test]
fn test_a_refused_priority_keeps_the_marks_and_the_view() {
    let mut a = browse(Listing::Ties("PK1".into()));
    press(&mut a, KeyCode::Char('x'));
    press(&mut a, KeyCode::Char('j'));
    press(&mut a, KeyCode::Char('x'));
    let before = body(&mut a);
    refuse(&a);
    press(&mut a, KeyCode::Char('!'));
    press(&mut a, KeyCode::Char('l'));
    let (verb, body_sent) = only(&a);
    assert_eq!(verb, "priority");
    assert_eq!(body_sent["ids"], json!(["PK1", "A1"]));
    assert_eq!(body(&mut a), before);
    assert_eq!(foot(&mut a).trim(), REFUSAL);
}

#[test]
fn test_priority_takes_one_key_for_the_tier() {
    let mut a = browse(Listing::Word("ready".into()));
    press(&mut a, KeyCode::Char('!'));
    assert!(
        foot(&mut a).contains("c critical  h high"),
        "{}",
        foot(&mut a)
    );
    press(&mut a, KeyCode::Char('z'));
    assert!(
        a.source.sent().is_empty(),
        "a key outside the choices waits"
    );
    press(&mut a, KeyCode::Char('h'));
    let (verb, body) = only(&a);
    assert_eq!(verb, "priority");
    assert_eq!(
        (&body["ids"], &body["tier"]),
        (&json!(["T3"]), &json!("high"))
    );
}

#[test]
fn test_rate_sends_one_request_per_marked_row() {
    let mut a = browse(Listing::Word("ready".into()));
    press(&mut a, KeyCode::Char('c'));
    press(&mut a, KeyCode::Char('l'));
    let (verb, body) = only(&a);
    assert_eq!(
        (verb.as_str(), &body["id"], &body["level"]),
        ("rate", &json!("T3"), &json!("low"))
    );

    let mut a = browse(Listing::Ties("PK1".into()));
    press(&mut a, KeyCode::Char('x'));
    press(&mut a, KeyCode::Char('j'));
    press(&mut a, KeyCode::Char('x'));
    press(&mut a, KeyCode::Char('c'));
    press(&mut a, KeyCode::Char('m'));
    let ids: Vec<_> = a
        .source
        .sent()
        .iter()
        .map(|(_, b)| b["id"].clone())
        .collect();
    assert_eq!(ids, [json!("PK1"), json!("A1")]);
}

#[test]
fn test_link_acts_on_the_selection() {
    let mut a = browse(Listing::Word("ready".into()));
    press(&mut a, KeyCode::Char('L'));
    typed(&mut a, "related CON1");
    press(&mut a, KeyCode::Enter);
    let (verb, body) = only(&a);
    assert_eq!(verb, "link");
    assert_eq!(
        (&body["a"], &body["kind"], &body["b"]),
        (&json!(["T3"]), &json!("related"), &json!("CON1"))
    );
}

#[test]
fn test_a_link_line_without_a_kind_and_an_id_sends_nothing() {
    let mut a = browse(Listing::Word("ready".into()));
    press(&mut a, KeyCode::Char('L'));
    typed(&mut a, "CON1");
    press(&mut a, KeyCode::Enter);
    assert!(a.source.sent().is_empty());
    assert!(foot(&mut a).contains("link takes a kind and an id"));
}

#[test]
fn test_answering_a_derived_decision_again_overturns_it() {
    let mut a = browse(Listing::Route(Route::Questions));
    a.source.items.lock().unwrap()[4].decision = Some("Derived from CID1: one store".into());
    a.changed();
    press(&mut a, KeyCode::Char('a'));
    assert!(
        foot(&mut a).contains("overturn Q1 (Derived from CID1: one store): _"),
        "{}",
        foot(&mut a)
    );
    typed(&mut a, "Two stores");
    press(&mut a, KeyCode::Enter);
    let (verb, body) = only(&a);
    assert_eq!(verb, "answer");
    assert_eq!(
        (&body["decision"], &body["derived"]),
        (&json!("Two stores"), &json!(null))
    );
}

fn settings_on(a: &mut App<Fixture>, key: &str) {
    a.open(Target::Project("o/p".into()));
    press(a, KeyCode::Char('S'));
    for _ in 0..view::spots(a).len() {
        if a.cursor().selected() == Some(&Target::Fact(key.into())) {
            return;
        }
        press(a, KeyCode::Tab);
    }
    assert!(
        a.cursor().selected() == Some(&Target::Fact(key.into())),
        "the settings page lists no fact {key}"
    );
}

fn colour_of(a: &mut App<Fixture>, text: &str) -> Option<Color> {
    let mut term = Terminal::new(TestBackend::new(160, 120)).unwrap();
    term.draw(|f| view::draw(a, f)).unwrap();
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

#[test]
#[should_panic(expected = "lists no fact nosuch")]
fn test_settings_on_panics_naming_a_fact_the_page_does_not_list() {
    let mut a = app();
    settings_on(&mut a, "nosuch");
}

#[test]
fn test_settings_enter_edits_a_fact_in_place() {
    let mut a = app();
    settings_on(&mut a, "gates");
    press(&mut a, KeyCode::Enter);
    typed(&mut a, "make test");
    assert!(
        lines(&mut a, 160, 120)
            .iter()
            .any(|l| l.starts_with("  gates           make test_"))
    );
    press(&mut a, KeyCode::Enter);
    let (verb, body) = only(&a);
    assert_eq!(verb, "fact");
    assert_eq!(
        (&body["key"], &body["value"]),
        (&json!("gates"), &json!("make test"))
    );
    let all = lines(&mut a, 160, 120);
    assert!(
        all.iter()
            .any(|l| l.trim_end() == "  gates           make test"),
        "{}",
        all.join("\n")
    );
    assert!(foot(&mut a).contains("set gates"));
}

#[test]
fn test_settings_a_refused_value_is_named_under_the_fact_and_nothing_changes() {
    let mut a = app();
    settings_on(&mut a, "mode");
    press(&mut a, KeyCode::Enter);
    typed(&mut a, "go");
    *a.source.refuse.lock().unwrap() = Some("mode is one of run, drain, pause, not 'go'".into());
    press(&mut a, KeyCode::Enter);
    assert_eq!(only(&a).0, "fact");
    let s = lines(&mut a, 160, 120).join("\n");
    assert!(s.contains("  mode            run (default)"), "{s}");
    assert!(
        s.contains("refused: mode is one of run, drain, pause, not 'go'"),
        "{s}"
    );
    assert_eq!(colour_of(&mut a, "refused: mode"), Some(Color::Red));
}

#[test]
fn test_settings_an_empty_value_unsets_the_fact() {
    let mut a = app();
    a.source
        .facts
        .lock()
        .unwrap()
        .push(("owner".into(), "Ana".into()));
    settings_on(&mut a, "owner");
    press(&mut a, KeyCode::Enter);
    assert!(
        lines(&mut a, 160, 120)
            .iter()
            .any(|l| l.starts_with("  owner           Ana_"))
    );
    ctrl(&mut a, 'u');
    press(&mut a, KeyCode::Enter);
    assert_eq!(only(&a).1["value"], "");
    assert!(screen(&mut a).contains("the owner (default)"));
}

#[test]
fn test_palette_runs_a_verb_on_the_selected_row() {
    let mut a = browse(Listing::Word("ready".into()));
    press(&mut a, KeyCode::Char(':'));
    typed(&mut a, "close abc1234");
    press(&mut a, KeyCode::Enter);
    let (verb, body) = only(&a);
    assert_eq!(verb, "close");
    assert_eq!(
        (&body["id"], &body["resolution"]),
        (&json!("T3"), &json!("abc1234"))
    );
}

#[test]
fn test_palette_opens_a_read_and_refuses_a_bad_line_without_a_request() {
    let mut a = browse(Listing::Word("ready".into()));
    press(&mut a, KeyCode::Char(':'));
    typed(&mut a, "questions");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.page.title(), "questions");
    press(&mut a, KeyCode::Char(':'));
    typed(&mut a, "rate");
    press(&mut a, KeyCode::Enter);
    assert!(a.source.sent().is_empty());
    assert!(
        foot(&mut a).contains("rate needs level"),
        "{}",
        foot(&mut a)
    );
}

#[test]
fn test_plans_space_still_folds_and_x_marks() {
    let mut a = app();
    a.open(Target::Project("o/p".into()));
    press(&mut a, KeyCode::Char('t'));
    press(&mut a, KeyCode::Char(' '));
    assert!(a.page.marks().unwrap().is_empty());
    assert!(screen(&mut a).contains("- A1"));
    press(&mut a, KeyCode::Char('x'));
    assert_eq!(a.selection(), ["A1"]);
    assert!(screen(&mut a).contains("*- A1"), "{}", screen(&mut a));
}

#[test]
fn test_a_write_key_with_nothing_selected_sends_nothing() {
    let mut a = app();
    for c in ['a', 'r', 'R', 'p', 'd', '!', 'c', 'L', 'F', 'x', ':'] {
        press(&mut a, KeyCode::Char(c));
    }
    assert!(a.source.sent().is_empty());
    assert!(a.prompt.is_none());
    assert_eq!(a.page.title(), "home");
}

#[test]
fn test_typing_a_line_takes_the_global_keys_as_text() {
    let mut a = queue();
    press(&mut a, KeyCode::Char('a'));
    typed(&mut a, "go to ?q");
    assert!(!a.quit);
    assert_eq!(a.page.title(), "yours");
    assert_eq!(a.prompt.as_ref().unwrap().text, "go to ?q");
}

#[test]
fn test_f_opens_the_queue_under_the_filter_typed_and_titles_it() {
    let mut a = app();
    a.open(Target::Project("o/p".into()));
    press(&mut a, KeyCode::Char('F'));
    typed(&mut a, "key:B release:9.9");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.page.title(), "next release:9.9 key:B");
    let Page::Browser(b) = &a.page else {
        panic!("not a browser");
    };
    assert!(
        b.entries.iter().all(|e| e.id.starts_with('B')),
        "{:?}",
        b.entries.len()
    );
    press(&mut a, KeyCode::Char('F'));
    assert_eq!(a.prompt.as_ref().unwrap().text, "release:9.9 key:B");
}

#[test]
fn test_f_refuses_an_unknown_field_and_stays_on_the_page() {
    let mut a = app();
    a.open(Target::Project("o/p".into()));
    press(&mut a, KeyCode::Char('F'));
    typed(&mut a, "colour:red");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.page.title(), "o/p");
    assert!(a.flash.as_ref().unwrap().contains("the fields are release"));
}
