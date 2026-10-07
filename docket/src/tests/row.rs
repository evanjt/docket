use serde_json::json;

use super::*;
use crate::jsonout::dumps_line;

fn row() -> Row {
    Row {
        id: "B14".into(),
        word: "under way".into(),
        priority: "high".into(),
        complexity: Some("low".into()),
        title: "Fix the thing".into(),
        state: "open".into(),
        claim_branch: Some("audit/b14-1".into()),
        claim_host: Some("box".into()),
        claim_since: Some("2026-01-01T00:00:00Z".into()),
        claim_runner: Some("codex".into()),
        group: Some("g".into()),
        ..Row::default()
    }
}

#[test]
fn test_fmt_row_head_and_tail() {
    assert_eq!(
        fmt_row(&row(), Some("no event for 3h")),
        "B14    under way high [low] Fix the thing\n       held by audit/b14-1 on box since \
         2026-01-01T00:00:00Z, run by codex  [no event for 3h]\n       group g"
    );
}

#[test]
fn test_fmt_row_waits_asks_and_ends() {
    let mut r = Row {
        id: "Q1".into(),
        word: "parked".into(),
        priority: "normal".into(),
        title: "Pick".into(),
        state: "open".into(),
        turn: Some("user".into()),
        turn_note: Some("which one".into()),
        wait_on: Some("condition".into()),
        wait_ref: Some("T4".into()),
        ..Row::default()
    };
    assert_eq!(
        fmt_row(&r, None),
        "Q1     parked   Pick\n       waits on T4, a task for the owner (since None)\n       asked: which one"
    );
    r.state = "done".into();
    r.wait_on = None;
    r.turn = None;
    assert_eq!(fmt_row(&r, None), "Q1     parked   Pick\n       done: None");
}

#[test]
fn test_item_json_orders_columns_extras_and_derived() {
    let v = json!({"id": "T1", "word": "ready", "score": -1.5, "project": "p", "snippet": "s"});
    let out = dumps_line(&item_json(&v, &["score"], &["snippet"]));
    assert!(out.starts_with("{\"project\": \"p\", \"key\": null, \"num\": null, \"id\": \"T1\""));
    assert!(out.ends_with(
        "\"claim_on\": null, \"score\": -1.5, \"group\": null, \"repo\": null, \"word\": \"ready\", \
         \"superseded_by\": null, \"priority\": null, \"snippet\": \"s\"}"
    ));
}
