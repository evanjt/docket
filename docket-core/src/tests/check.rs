use serde_json::json;

use super::{counts, ids, phrase};

#[test]
fn test_counts_group_by_kind_in_first_seen_order_and_phrase_the_plural() {
    let problems = json!([
        { "kind": "cycle", "id": "T1" },
        { "kind": "no_body", "id": "T2" },
        { "kind": "cycle", "id": "T3" },
    ]);
    let got = counts(&problems);
    assert_eq!(got, [("cycle".to_string(), 2), ("no_body".to_string(), 1)]);
    assert_eq!(phrase("cycle", 2), "2 cycles");
    assert_eq!(phrase("held_gate", 57), "57 holds on closed work");
    assert_eq!(phrase("cycle", 1), "1 cycle");
}

#[test]
fn test_ids_name_each_item_of_a_kind_once() {
    let problems = vec![
        json!({ "kind": "cycle", "id": "T1" }),
        json!({ "kind": "cycle", "id": "T1" }),
        json!({ "kind": "no_body", "id": "T2" }),
    ];
    assert_eq!(ids(&problems, "cycle"), ["T1"]);
}
