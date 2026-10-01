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
        "ready 2 > building 0 > checking 0 > done 4    parked 1"
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
        {"kind": "two_packages", "id": "B1", "packages": "PK1, PK2"},
        {"kind": "stale_wait", "id": "T3", "since": "2026-01-01T00:00:00Z", "until": "rain"},
    ]);
    assert_eq!(
        problem_lines(&problems),
        [
            "1 item is filed under X, which the project does not define",
            "B1 sits in two open packages, PK1, PK2. Unlink it from one.",
            "T3 has waited since 2026-01-01T00:00:00Z until: rain",
        ]
    );
}

#[test]
fn test_facts_default_and_pool_slots() {
    let f = Facts::of(&json!({"pool": "local=4 devbox=16", "stale_claim": "30"}));
    assert_eq!(f.mode, "pause");
    assert_eq!(f.stale, 30);
    assert_eq!(
        f.slots(),
        [("local".to_string(), 4), ("devbox".to_string(), 16)]
    );
}
