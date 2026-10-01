use serde_json::json;

use super::*;

const ITEM_FULL: &str = include_str!("fixtures/dump/item_full.md");
const ITEM_BARE: &str = include_str!("fixtures/dump/item_bare.md");
const EVENTS: &str = include_str!("fixtures/dump/events.jsonl");
const PROJECT: &str = include_str!("fixtures/dump/project.json");

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn full_item() -> ItemDump {
    ItemDump {
        project: "org/proj".into(),
        id: "T7".into(),
        title: "Caf\u{e9} \"quoted\" \\ back \u{2014} and \u{1f600}".into(),
        state: "open".into(),
        turn: Some("agent".into()),
        turn_note: Some("line one\nline two\ttab".into()),
        claim_branch: Some("audit/t7-1".into()),
        claim_host: Some("devbox".into()),
        claim_since: Some("2026-01-02T03:04:05Z".into()),
        claim_runner: Some("codex".into()),
        claim_job: Some("job1".into()),
        claim_on: Some("buildbox".into()),
        wait_on: Some("item".into()),
        wait_ref: Some("Q2".into()),
        wait_since: Some("2026-01-02T00:00:00Z".into()),
        scope: Some("later".into()),
        complexity: Some("medium".into()),
        group: Some("g\u{fc}rtel".into()),
        rank: Some(3),
        tags: strings(&["high", "single"]),
        related: strings(&["T10", "CON1", "T9"]),
        opened: strings(&["PK2", "A1"]),
        opened_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-03T00:00:00Z".into(),
        body: "**Evidence.** `src/a.rs:3` \u{e9}t\u{e9}\n\n- a\n\n\n".into(),
        ..ItemDump::default()
    }
}

fn bare_item() -> ItemDump {
    ItemDump {
        project: "org/proj".into(),
        id: "B1".into(),
        title: "Bare".into(),
        state: "done".into(),
        decision: Some("yes".into()),
        decided_at: Some("2026-02-02T00:00:00Z".into()),
        resolution: Some("abc1234".into()),
        superseded_by: Some("B2".into()),
        theme: Some("ui".into()),
        opened_at: "2026-02-01T00:00:00Z".into(),
        updated_at: "2026-02-02T00:00:00Z".into(),
        body: "\n\n".into(),
        ..ItemDump::default()
    }
}

fn event(uid: &str, at: &str, kind: &str, item: Option<&str>, note: Option<&str>) -> EventDump {
    EventDump {
        project: "org/proj".into(),
        uid: uid.into(),
        at: at.into(),
        host: "laptop".into(),
        kind: kind.into(),
        item: item.map(str::to_string),
        note: note.map(str::to_string),
        ..EventDump::default()
    }
}

fn fixture_events() -> Vec<EventDump> {
    let mut closed = event(
        "2026-01-02T00:00:00Z-devbox-aaaaaa",
        "2026-01-02T00:00:00Z",
        "closed",
        Some("T7"),
        Some("fixed \u{e9} \"q\""),
    );
    closed.host = "devbox".into();
    closed.branch = Some("main".into());
    closed.data = Some(r#"{"gates": "passed: ok", "landed": "abc", "nested": {"z": 1, "a": [true, null]}, "u": "\u00e9"}"#.into());
    let mut edited = event(
        "2026-01-01T00:00:00Z-laptop-aaaaaa",
        "2026-01-01T00:00:00Z",
        "edited",
        Some("B1"),
        Some("priority high"),
    );
    edited.data = Some(String::new());
    let queue = event(
        "2026-01-01T00:00:00Z-laptop-bbbbbb",
        "2026-01-01T00:00:00Z",
        "queue",
        None,
        None,
    );
    vec![closed, queue, edited]
}

fn fixture_project() -> ProjectDump {
    ProjectDump {
        slug: "org/proj".into(),
        keys: json!([{"key": "T", "kind": "work", "meaning": "t\u{e2}ches", "turn": "agent"}]),
        remotes: json!(["git@example.com:org/proj.git"]),
        themes: json!([]),
        cite_roots: json!([]),
        repos: json!([]),
        integration_ref: Some("main".into()),
        worktree_hint: Some("git worktree add".into()),
        skills: json!({"merge": "rebase", "a": "first"}),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-04T00:00:00Z".into(),
        ..ProjectDump::default()
    }
}

/// One event: when, its kind, its item and its note.
type Spec<'a> = (&'a str, &'a str, Option<&'a str>, Option<&'a str>);

fn writes(spec: &[Spec]) -> Vec<EventDump> {
    spec.iter()
        .enumerate()
        .map(|(n, (at, kind, item, note))| event(&format!("u{n}"), at, kind, *item, *note))
        .collect()
}

#[test]
fn test_render_item_matches_python_with_every_field_set() {
    assert_eq!(render_item(&full_item()), ITEM_FULL);
}

#[test]
fn test_render_item_matches_python_with_optional_fields_unset_and_empty_body() {
    assert_eq!(render_item(&bare_item()), ITEM_BARE);
}

#[test]
fn test_event_line_matches_python_sorted_with_data_only_when_set() {
    let mut events = fixture_events();
    events.sort_by(|a, b| (&a.at, &a.uid).cmp(&(&b.at, &b.uid)));
    let lines: String = events.iter().map(|e| event_line(e) + "\n").collect();
    assert_eq!(lines, EVENTS);
}

#[test]
fn test_project_text_matches_python() {
    assert_eq!(project_text(&fixture_project()), PROJECT);
}

#[test]
fn test_merge_events_adds_new_lines_in_order_and_keeps_each_uid_once() {
    let events = fixture_events();
    let first: Vec<&EventDump> = events.iter().take(1).collect();
    let before = merge_events("", &first);
    let all: Vec<&EventDump> = events.iter().collect();
    assert_eq!(merge_events(&before, &all), EVENTS);
    assert_eq!(merge_events(EVENTS, &all), EVENTS);
    assert_eq!(merge_events("", &[]), "");
}

#[test]
fn test_files_full_page_replaces_logs_and_writes_empty_ones() {
    let page = DumpPage {
        cursor: 3,
        full: true,
        projects: vec![fixture_project()],
        items: vec![full_item(), bare_item()],
        events: vec![],
    };
    let out = files(&page, |_| Some("stale\n".to_string()));
    let paths: Vec<&str> = out.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "org/proj/project.json",
            "org/proj/items/T7.md",
            "org/proj/items/B1.md",
            "org/proj/events.jsonl"
        ]
    );
    assert_eq!(out[3].1, "");
}

#[test]
fn test_files_incremental_page_adds_to_the_log_on_disk() {
    let events = fixture_events();
    let page = DumpPage {
        cursor: 3,
        full: false,
        projects: vec![],
        items: vec![],
        events: events[1..].to_vec(),
    };
    let before = event_line(&events[0]) + "\n";
    let out = files(&page, |p| {
        (p == "org/proj/events.jsonl").then(|| before.clone())
    });
    assert_eq!(
        out,
        vec![("org/proj/events.jsonl".to_string(), EVENTS.to_string())]
    );
}

#[test]
fn test_parse_item_round_trips_python_files() {
    for text in [ITEM_FULL, ITEM_BARE] {
        let (fields, body) = parse_item(text).unwrap();
        let item = item_from("org/proj", fields, body).unwrap();
        assert_eq!(render_item(&item), text);
    }
}

#[test]
fn test_parse_item_refuses_files_that_are_not_items() {
    assert!(parse_item("id: \"T1\"\n").is_err());
    assert!(parse_item("---\nid: \"T1\"\n").is_err());
    assert!(parse_item("---\nowner: \"x\"\n---\n").is_err());
    assert!(parse_item("---\nid: T1\n---\n").is_err());
    let (fields, _) = parse_item("---\ntitle: \"x\"\n---\n").unwrap();
    assert!(item_from("o/p", fields, String::new()).is_err());
}

#[test]
fn test_commit_subject_names_three_then_counts() {
    let m = |v: &[&str]| strings(v);
    assert_eq!(commit_subject(&[]), "docket write");
    assert_eq!(commit_subject(&m(&["Close T1", "Close T1"])), "Close T1");
    assert_eq!(
        commit_subject(&m(&["A", "B", "C", "D", "E", "A"])),
        "A; B; C; and 2 more"
    );
}

#[test]
fn test_messages_name_each_write_by_its_first_event() {
    let t = |s: u8| format!("2026-01-01T00:00:{s:02}Z");
    let (t0, t1, t2, t3) = (t(0), t(1), t(2), t(3));
    let events = writes(&[
        (&t0, "closed", Some("T1"), Some("abc")),
        (&t0, "resumed", Some("T2"), Some("T1 closed")),
        (&t1, "claimed", Some("T3"), None),
        (&t2, "opened", Some("Q1"), Some("x")),
        (&t3, "queue", None, None),
    ]);
    assert_eq!(
        messages(&events),
        strings(&["Close T1", "Start T3", "Open Q1"])
    );
}

#[test]
fn test_messages_tell_edits_apart_by_note() {
    let cases: [(&[Spec], &str); 8] = [
        (&[("a", "edited", Some("T1"), Some("title=x"))], "Edit T1"),
        (
            &[("a", "edited", Some("T1"), Some("complexity low"))],
            "Rate T1",
        ),
        (
            &[("a", "edited", Some("T1"), Some("priority high"))],
            "Prioritise T1",
        ),
        (
            &[
                ("a", "edited", Some("T1"), Some("priority low")),
                ("a", "edited", Some("T2"), Some("priority low")),
            ],
            "Prioritise 2 items",
        ),
        (
            &[("a", "edited", Some("T1"), Some("link related PK1"))],
            "Link T1",
        ),
        (
            &[
                ("a", "edited", Some("T1"), Some("link opened A3")),
                ("a", "edited", Some("T2"), Some("link opened A3")),
                ("a", "waited", Some("A3"), Some("everything")),
            ],
            "Link 2 items to A3",
        ),
        (
            &[("a", "edited", Some("T1"), Some("to the later: why"))],
            "Move T1 to the later",
        ),
        (
            &[
                ("a", "edited", Some("T1"), Some("moved from PK2 to PK1")),
                ("a", "dropped", Some("PK2"), Some("folded into PK1")),
                ("a", "edited", Some("PK1"), Some("folded PK2, PK3")),
            ],
            "Fold PK2, PK3 into PK1",
        ),
    ];
    for (spec, expected) in cases {
        assert_eq!(messages(&writes(spec)), strings(&[expected]), "{spec:?}");
    }
}

#[test]
fn test_messages_answer_or_decide_by_derived_data() {
    let mut events = writes(&[("a", "decided", Some("Q1"), Some("yes"))]);
    assert_eq!(messages(&events), strings(&["Answer Q1"]));
    events[0].data = Some(r#"{"derived": "CID1"}"#.into());
    assert_eq!(messages(&events), strings(&["Decide on Q1"]));
}

#[test]
fn test_key_messages_name_keys_added_or_changed() {
    let mut after = fixture_project();
    assert!(key_messages(Some(PROJECT), &after).is_empty());
    assert!(key_messages(None, &after).is_empty());
    after.keys = json!([
        {"key": "T", "kind": "work", "meaning": "tasks", "turn": "agent"},
        {"key": "B", "kind": "work", "meaning": "bugs", "turn": "agent"}
    ]);
    assert_eq!(
        key_messages(Some(PROJECT), &after),
        strings(&["Key T", "Key B"])
    );
}

#[test]
fn test_messages_split_a_burst_in_one_second_into_its_writes() {
    let led = |kind: &str, item: &str, note: Option<&str>| {
        let mut e = event(&format!("{kind}{item}"), "s", kind, Some(item), note);
        e.branch = Some("main".into());
        e
    };
    let side = |kind: &str, item: &str, note: &str| event("x", "s", kind, Some(item), Some(note));
    let events = vec![
        led("opened", "T1", Some("a")),
        led("opened", "T2", Some("b")),
        led("closed", "Q1", Some("opened T1")),
        side("resumed", "T1", "Q1 closed"),
        side("edited", "T1", "priority high"),
        side("edited", "T2", "priority high"),
        side("edited", "T1", "link opened A1"),
        side(
            "waited",
            "A1",
            "until: everything it opened is closed (1 open)",
        ),
        led("edited", "B3", Some("moved from PK2 to PK1")),
        led("dropped", "PK2", Some("folded into PK1")),
        led("edited", "PK1", Some("folded PK2")),
        led("resumed", "T2", None),
        led("claimed", "T2", None),
    ];
    let expected = [
        "Open T1",
        "Open T2",
        "Close Q1",
        "Prioritise 2 items",
        "Link T1",
        "Fold PK2 into PK1",
        "Resume T2",
        "Start T2",
    ];
    assert_eq!(messages(&events), strings(&expected));
}
