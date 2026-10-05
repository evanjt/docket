use serde_json::json;

use super::*;

fn release(name: &str, shipped: Option<&str>) -> Release {
    Release {
        name: name.into(),
        shipped_at: shipped.map(String::from),
        ..Release::default()
    }
}

fn rows() -> Vec<ReleaseRow> {
    serde_json::from_value::<Value>(json!({"releases": [{
        "name": "2.0", "open": 5, "ready": 1, "building": 2, "waiting_owner": 1,
        "blocked": 1, "closed": 3, "held_later": 1, "pace": 0.5,
        "forecast": {"open": 5, "burn": 0.5, "converging": true,
            "p50": null, "p85": null, "target": null, "late": null}
    }]}))
    .map(|mut b| serde_json::from_value(b["releases"].take()).unwrap())
    .unwrap()
}

#[test]
fn test_releases_text_prints_each_unshipped_release_s_counts_from_the_route_body() {
    let all = [
        release("1.0", Some("2026-01-01T00:00:00Z")),
        release("2.0", None),
    ];
    let text = render(&all, &rows());
    assert_eq!(text[0], "1.0  shipped 2026-01-01");
    assert_eq!(
        text[1],
        "2.0  current\n    5 open (ready 1, building 2, owner 1, blocked 1), 3 closed, 1 held later, pace +0.5/day"
    );
}

#[test]
fn test_releases_json_carries_the_row_of_an_unshipped_release_and_null_for_a_shipped_one() {
    let all = [
        release("1.0", Some("2026-01-01T00:00:00Z")),
        release("2.0", None),
    ];
    let out = rows_json(&all, &rows());
    assert!(out[0]["progress"].is_null());
    assert_eq!(out[1]["progress"]["held_later"], 1);
}
