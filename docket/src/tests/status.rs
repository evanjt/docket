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
        {"kind": "stale_wait", "id": "T3", "since": "2026-01-01T00:00:00Z", "until": "rain"},
    ]);
    assert_eq!(
        problem_lines(&problems),
        [
            "1 item is filed under X, which the project does not define",
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
            "claims": [{"id": "T2", "title": "Second member", "branch": "audit/t2-1",
                        "host": "devbox", "since": 1000, "flag": "no event for 3h"}],
            "plans": [{"id": "A1", "title": "Loaves stay put", "done": 1, "total": 4, "live": 1}],
            "due": [{"id": "A2", "title": "Orders print at the till"}],
            "problems": [],
        }),
        metrics: Value::Null,
        yours: json!([{"id": "Q1", "title": "Which shelf holds the loaves"}]),
        next: json!([{"id": "T3", "title": "Fix the loaf count", "complexity": "low"}]),
        now: 1000 + 3 * 3600,
        squash: None,
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
        text.contains(" Q1     Which shelf holds the loaves"),
        "{text}"
    );
    assert!(text.contains("AUDITS DUE  1"), "{text}");
    assert!(text.contains(" A2     Orders print at the till"), "{text}");
    assert!(text.contains("PLANS  1 under way"), "{text}");
    assert!(
        text.contains("   1. T3     Fix the loaf count [low]"),
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

#[test]
fn test_status_text_prints_the_forecast_of_the_metrics_body() {
    let mut r = read();
    r.metrics = json!({
        "forecast": {
            "open": 12, "burn": 1.5, "converging": true,
            "p50": "2026-10-20", "p85": "2026-11-02", "target": null, "late": null
        }
    });
    let lines = render("o/p", &r);
    assert_eq!(lines[1], "12 open, clear by P50 2026-10-20, P85 2026-11-02");
}

#[test]
fn test_status_text_says_not_converging_for_a_zero_burn() {
    let mut r = read();
    r.metrics = json!({
        "forecast": {
            "open": 12, "burn": 0.0, "converging": false,
            "p50": null, "p85": null, "target": null, "late": null
        }
    });
    assert_eq!(
        render("o/p", &r)[1],
        "12 open, not converging: closes do not outrun opens"
    );
}

#[test]
fn test_status_text_prints_no_forecast_line_without_one() {
    assert!(render("o/p", &read())[1].starts_with("ready "));
}

#[test]
fn test_a_hold_by_a_later_release_names_both_releases() {
    let problems = json!([
        {"kind": "held_later", "id": "A1", "by": "T4", "release": "1.0", "later": "1.1"},
    ]);
    assert_eq!(
        problem_lines(&problems),
        ["A1 (1.0) is held by T4, in the later release 1.1"]
    );
}

#[test]
fn test_many_problems_print_as_one_line_of_counts() {
    let mut problems: Vec<_> = (0..95)
        .map(|i| json!({"kind": "held_later", "id": format!("T{i}"), "by": "T1", "release": "1.0", "later": "1.1"}))
        .collect();
    problems.extend((0..5).map(|i| json!({"kind": "cycle", "id": format!("B{i}")})));
    let mut r = read();
    r.summary["problems"] = json!(problems);
    let text = render("o/p", &r).join("\n");
    assert!(
        text.contains("\nCheck:\n  95 release inversions, 5 cycles: docket check\n"),
        "{text}"
    );
    assert!(!text.contains("is held by"), "{text}");
}

#[test]
fn test_status_json_carries_every_section_of_the_text() {
    let json = json_status(&read());
    for key in ["yours", "next", "plans", "problems", "metrics"] {
        assert!(json.get(key).is_some(), "{key}: {json}");
    }
    assert_eq!(json["yours"][0]["id"], "Q1");
    assert_eq!(json["next"][0]["id"], "T3");
    assert_eq!(json["plans"][0]["id"], "A1");
}

#[test]
fn test_a_dependency_dropped_without_a_successor_names_the_repair() {
    let problems = json!([{"kind": "abandoned", "id": "T6", "on": "T7"}]);
    assert_eq!(
        problem_lines(&problems),
        [
            "T6 depends on T7, dropped with no successor: docket dep rm T6 T7, or wait on what replaced it"
        ]
    );
}

#[test]
fn test_counts_read_the_word_map_the_status_route_answers_with() {
    let by_word = json!({"done": 4, "ready": 2});
    assert_eq!(
        counts(&by_word),
        vec![("done".to_string(), 4), ("ready".to_string(), 2)]
    );
}
