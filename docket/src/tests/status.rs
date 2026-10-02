use serde_json::json;

use super::*;

#[test]
fn test_flow_line_reads_the_flow_then_what_is_aside() {
    let total = vec![
        ("done".to_string(), 4),
        ("ready".to_string(), 2),
        ("parked".to_string(), 1),
    ];
    assert_eq!(
        flow_line(&total),
        "ready 2 > building 0 > done 4    parked 1"
    );
}

#[test]
fn test_bar_rounds_half_to_even_and_handles_nothing() {
    assert_eq!(bar(0.0, 0.0), "..........");
    // 2.5 cells round to 2
    assert_eq!(bar(1.0, 4.0), "##........");
    assert_eq!(bar(9.0, 4.0), "##########");
}

#[test]
fn test_problem_lines_word_each_kind() {
    let problems = json!([
        {"kind": "undefined_key", "key": "X", "n": 1},
        {"kind": "open_audit", "id": "A1", "n": 2},
        {"kind": "stale_wait", "id": "T3", "since": "2026-01-01T00:00:00Z", "until": "rain"},
    ]);
    assert_eq!(
        problem_lines(&problems),
        [
            "1 item is filed under X, which the project does not define",
            "A1 is open for audit while 2 items it opened are open",
            "T3 has waited since 2026-01-01T00:00:00Z until: rain",
        ]
    );
}

fn read() -> Read {
    Read {
        total: vec![
            ("ready".to_string(), 3),
            ("building".to_string(), 1),
            ("done".to_string(), 4),
            ("parked".to_string(), 1),
        ],
        summary: json!({
            "skills": {"mode": "pause", "pool": "local=2"},
            "pace": {"closed": 0, "working": 0},
            "claims": [{"id": "T2", "title": "Second member", "branch": "audit/t2-1",
                        "host": "devbox", "since": 1000, "flag": "no event for 3h"}],
            "plans": [{"id": "A1", "title": "Buns stay put", "done": 1, "total": 4, "live": 1}],
            "due": [{"id": "A2", "title": "Orders print at the till"}],
            "problems": [],
        }),
        yours: json!([{"id": "Q1", "title": "Which store holds the buns"}]),
        next: json!([{"id": "T3", "title": "Fix the bun cache", "complexity": "low"}]),
        now: 1000 + 3 * 3600,
    }
}

#[test]
fn test_status_text_shows_claims_owner_items_and_due_audits() {
    let text = render("o/p", &read()).join("\n");
    assert!(text.starts_with("o/p"), "{text}");
    assert!(text.contains("4 of 9 closed"), "{text}");
    assert!(text.contains("CLAIMED NOW  1"), "{text}");
    assert!(
        text.contains(" T2     audit/t2-1 on devbox  3h 00m  [no event for 3h] Second member"),
        "{text}"
    );
    assert!(text.contains("YOURS  1  (docket todo)"), "{text}");
    assert!(
        text.contains(" Q1     Which store holds the buns"),
        "{text}"
    );
    assert!(text.contains("AUDITS DUE  1"), "{text}");
    assert!(text.contains(" A2     Orders print at the till"), "{text}");
    assert!(text.contains("PLANS  1 under way"), "{text}");
    assert!(
        text.contains("   1. T3     Fix the bun cache [low]"),
        "{text}"
    );
    assert!(text.contains("/plan") && text.contains("/work") && text.contains("/audit"));
}

#[test]
fn test_status_text_has_no_loop_pool_or_release() {
    let text = render("o/p", &read()).join("\n");
    for gone in [
        "loop", "Loop", "pool", "slots", "MACHINES", "JOBS", "CHORES", "PACKAGES", "Release",
        "NO LOOP",
    ] {
        assert!(!text.contains(gone), "{gone}: {text}");
    }
}

#[test]
fn test_status_text_says_when_nothing_is_claimed_waiting_or_due() {
    let mut r = read();
    r.summary["claims"] = json!([]);
    r.summary["due"] = json!([]);
    r.summary["plans"] = json!([]);
    r.yours = json!([]);
    let text = render("o/p", &r).join("\n");
    assert!(text.contains("CLAIMED NOW  nothing claimed"), "{text}");
    assert!(text.contains("YOURS  nothing waits on you"), "{text}");
    assert!(text.contains("AUDITS DUE  no plan is due"), "{text}");
    assert!(text.contains("PLANS  none under way"), "{text}");
}
