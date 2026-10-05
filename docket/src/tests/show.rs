use serde_json::json;

use super::*;

#[test]
fn test_the_newest_placement_among_an_items_events_is_its_placed_line() {
    let events = vec![
        json!({"kind": "decided", "data": {"derived": "b", "area": "kites"}}),
        json!({"kind": "noted", "data": {}}),
        json!({"kind": "decided", "data": {"derived": "b", "area": "lanterns"}}),
        json!({"kind": "decided", "data": {"derived": "plain"}}),
    ];
    assert_eq!(
        placed_line(&events),
        Some("placed in lanterns (derived)".to_string())
    );
    assert_eq!(placed_line(&events[1..2]), None);
}
