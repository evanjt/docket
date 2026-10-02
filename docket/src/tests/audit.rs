use serde_json::json;

use super::*;

#[test]
fn test_shas_finds_hex_words_of_seven_or_more() {
    assert_eq!(
        shas("landed abc1234, see 12345 and deadbeefcafe"),
        ["abc1234", "deadbeefcafe"]
    );
}

#[test]
fn test_unreachable_sorts_done_work_by_where_its_sha_is() {
    let done = [
        json!({"id": "B1", "state": "done", "kind": "work", "resolution": "abc1234"}),
        json!({"id": "B2", "state": "done", "kind": "work", "resolution": "def5678"}),
        json!({"id": "B3", "state": "done", "kind": "work", "resolution": "no sha"}),
        json!({"id": "Q1", "state": "done", "kind": "decision", "resolution": "opened B1"}),
    ];
    let head: HashSet<String> = ["abc1234ffff".to_string()].into();
    let known: HashSet<String> = ["abc1234ffff".to_string(), "def5678000".to_string()].into();
    let (off, nowhere) = unreachable(&done, &head, &known);
    assert_eq!(off, ["B2"]);
    assert_eq!(nowhere, ["B3"]);
}

#[test]
fn test_breakdown_lines_for_a_package_and_a_concept() {
    let pkg =
        json!({"progress": {"done": 1, "total": 3, "live": 1}, "touches": ["a.rs", "b/c.rs"]});
    assert_eq!(
        breakdown_lines(&pkg),
        [
            "members: 1 of 3 done, 1 live",
            "touches, 2 files: a.rs, b/c.rs"
        ]
    );
    let con = json!({"by plan": [["A1", 4, 1]], "shared": 2, "of": "concept"});
    assert_eq!(
        breakdown_lines(&con),
        [
            "by plan: A1 4 (1 open)",
            "2 of these belong to other concepts too"
        ]
    );
}

#[test]
fn test_sections_are_found_by_word_and_open_counts_the_flow_words() {
    let rows: Vec<Value> = [
        "ready", "blocked", "blocked", "parked", "standing", "done", "dropped",
    ]
    .iter()
    .enumerate()
    .map(|(i, w)| serde_json::json!({"id": format!("T{i}"), "word": w}))
    .collect();
    let sections = sections_of(&rows);
    assert_eq!(section(&sections, "done").len(), 1);
    assert_eq!(section(&sections, "done")[0]["id"], "T5");
    assert_eq!(section(&sections, "blocked").len(), 2);
    assert_eq!(open_count(&sections), 4);
    assert!(section(&sections, "inbox").is_empty());
}
