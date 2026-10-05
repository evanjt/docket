use serde_json::json;

use super::*;

fn event(kind: &str, at: &str, branch: &str, note: Option<&str>) -> Event {
    Event {
        kind: kind.into(),
        at: at.into(),
        host: "bench".into(),
        branch: Some(branch.into()),
        note: note.map(str::to_string),
        data: None,
    }
}

fn ends(rows: &[Assignment]) -> Vec<(Kind, Option<&str>, Option<Outcome>)> {
    rows.iter()
        .map(|a| (a.kind, a.ended_at.as_deref(), a.outcome))
        .collect()
}

#[test]
fn test_a_released_then_closed_item_rebuilds_a_failed_and_a_landed_row() {
    let rows = rebuild(&[
        event("claimed", "2026-01-02T00:00:00Z", "build/t1-1", None),
        event("released", "2026-01-02T01:00:00Z", "build/t1-1", None),
        event("claimed", "2026-01-02T02:00:00Z", "build/t1-2", None),
        event(
            "closed",
            "2026-01-03T00:00:00Z",
            "build/t1-2",
            Some("abc1234"),
        ),
    ]);
    assert_eq!(
        ends(&rows),
        [
            (
                Kind::Claim,
                Some("2026-01-02T01:00:00Z"),
                Some(Outcome::Failed)
            ),
            (
                Kind::Claim,
                Some("2026-01-03T00:00:00Z"),
                Some(Outcome::Landed)
            ),
        ]
    );
    assert_eq!(rows[0].branch.as_deref(), Some("build/t1-1"));
    assert_eq!(rows[1].started_at, "2026-01-02T02:00:00Z");
    assert!(rows.iter().all(|a| a.assignee == AGENT));
}

#[test]
fn test_an_outcome_written_on_the_release_wins_over_its_note() {
    let mut released = event("released", "b", "build/t1-1", Some("gates failed"));
    released.data = Some(json!({ "outcome": "conflict" }));
    let rows = rebuild(&[event("claimed", "a", "build/t1-1", None), released]);
    assert_eq!(rows[0].outcome, Some(Outcome::Conflict));
    assert_eq!(rows[0].note.as_deref(), Some("gates failed"));
}

#[test]
fn test_a_release_without_an_outcome_is_read_from_its_note() {
    let cases = [
        (
            Some("merge conflict in src/hive.rs"),
            None,
            Outcome::Conflict,
        ),
        (
            Some("finished"),
            Some(json!({ "rebase": "build/t1-1" })),
            Outcome::Conflict,
        ),
        (Some("gates failed: clippy"), None, Outcome::Gate),
        (Some("blocked: waits on the owner"), None, Outcome::Blocked),
        (Some("job timed out"), None, Outcome::Failed),
        (None, None, Outcome::Failed),
    ];
    for (note, data, want) in cases {
        let mut released = event("released", "b", "build/t1-1", note);
        released.data = data;
        let rows = rebuild(&[event("claimed", "a", "build/t1-1", None), released]);
        assert_eq!(rows[0].outcome, Some(want), "{note:?}");
    }
}

#[test]
fn test_a_holder_that_waits_ends_its_attempt_blocked() {
    let rows = rebuild(&[
        event("claimed", "a", "build/t1-1", None),
        event("waited", "b", "build/t1-1", Some("on Q1")),
        event("waited", "c", "build/t1-1", Some("on Q2")),
    ]);
    assert_eq!(
        ends(&rows),
        [(Kind::Claim, Some("b"), Some(Outcome::Blocked))]
    );
}

#[test]
fn test_an_ask_hands_the_item_to_the_owner_until_the_reply() {
    let rows = rebuild(&[
        event("claimed", "a", "build/t1-1", None),
        event("asked", "b", "build/t1-1", Some("run it on a device")),
        event("replied", "c", "main", Some("it runs")),
        event("claimed", "d", "build/t1-2", None),
    ]);
    assert_eq!(
        ends(&rows),
        [
            (Kind::Claim, Some("b"), Some(Outcome::Blocked)),
            (Kind::Ask, Some("c"), None),
            (Kind::Claim, None, None),
        ]
    );
    assert_eq!(rows[1].assignee, OWNER);
    assert_eq!(rows[1].note.as_deref(), Some("run it on a device"));
    assert_eq!(rows[1].started_at, "b");
}

#[test]
fn test_an_item_opened_on_the_owners_turn_is_asked_from_the_start() {
    let mut opened = event("opened", "a", "main", Some("Which glaze"));
    opened.data = Some(json!({ "turn": "user" }));
    let rows = rebuild(&[
        opened,
        event("opened", "b", "main", Some("Glaze the bowl")),
        event("decided", "c", "main", Some("matte")),
    ]);
    assert_eq!(ends(&rows), [(Kind::Ask, Some("c"), None)]);
    assert_eq!(rows[0].note, None);
}

#[test]
fn test_an_answer_ends_the_ask_and_a_derived_decision_does_not() {
    let mut decide = event("decided", "c", "main", Some("best practice"));
    decide.data = Some(json!({ "derived": "the usual rule" }));
    let rows = rebuild(&[
        event("asked", "a", "main", Some("which way")),
        event("edited", "b", "main", Some("title")),
        decide,
    ]);
    assert_eq!(ends(&rows), [(Kind::Ask, None, None)]);
    let rows = rebuild(&[
        event("asked", "a", "main", Some("which way")),
        event("decided", "b", "main", Some("north")),
    ]);
    assert_eq!(ends(&rows), [(Kind::Ask, Some("b"), None)]);
}

#[test]
fn test_a_claim_carries_the_job_it_names() {
    let mut claimed = event("claimed", "a", "lead/t1-9", None);
    claimed.data = Some(json!({
        "runner": "claude", "model": "m-large", "effort": "high", "role": "build",
        "job": "lead-t1-9", "machine": "workbench",
    }));
    let rows = rebuild(&[claimed]);
    let a = &rows[0];
    assert_eq!(
        (
            a.runner.as_deref(),
            a.model.as_deref(),
            a.effort.as_deref(),
            a.role.as_deref(),
            a.job.as_deref(),
            a.machine.as_deref(),
            a.host.as_str(),
        ),
        (
            Some("claude"),
            Some("m-large"),
            Some("high"),
            Some("build"),
            Some("lead-t1-9"),
            Some("workbench"),
            "bench"
        )
    );
    assert_eq!(
        (a.tokens_in, a.tokens_out, a.cost_reported),
        (None, None, None)
    );
}

#[test]
fn test_settle_ends_what_the_item_no_longer_holds_and_opens_what_it_does() {
    let mut rows = rebuild(&[event("claimed", "a", "build/t1-1", None)]);
    settle(
        &mut rows,
        &Now {
            updated_at: "z".into(),
            ..Now::default()
        },
    );
    assert_eq!(ends(&rows), [(Kind::Claim, Some("z"), None)]);

    let mut rows = Vec::new();
    settle(
        &mut rows,
        &Now {
            claim: Some(Claim {
                branch: "build/t2-1".into(),
                host: "bench".into(),
                since: "s".into(),
                runner: Some("codex".into()),
                job: Some("j".into()),
                on: Some("workbench".into()),
            }),
            asked: None,
            updated_at: "z".into(),
        },
    );
    assert_eq!(ends(&rows), [(Kind::Claim, None, None)]);
    assert_eq!(rows[0].machine.as_deref(), Some("workbench"));

    let mut rows = Vec::new();
    settle(
        &mut rows,
        &Now {
            claim: None,
            asked: Some(("q".into(), Some("pick one".into()))),
            updated_at: "z".into(),
        },
    );
    assert_eq!(ends(&rows), [(Kind::Ask, None, None)]);
    assert_eq!(rows[0].started_at, "q");
}

#[test]
fn test_each_live_step_matches_the_rebuild() {
    let events = [
        event("claimed", "a", "build/t1-1", None),
        event("released", "b", "build/t1-1", Some("merge conflict")),
        event("asked", "c", "main", Some("which way")),
        event("replied", "d", "main", Some("north")),
        event("claimed", "e", "build/t1-2", None),
        event("closed", "f", "build/t1-2", Some("abc1234")),
    ];
    let mut live: Vec<Assignment> = Vec::new();
    for e in &events {
        let open = live
            .iter()
            .rev()
            .find(|a| a.ended_at.is_none())
            .map(|a| a.kind);
        let s = step(e, open);
        if let Some(end) = s.end {
            let a = live
                .iter_mut()
                .rev()
                .find(|a| a.ended_at.is_none())
                .unwrap();
            a.ended_at = Some(e.at.clone());
            a.outcome = end.outcome;
            if end.note.is_some() {
                a.note = end.note;
            }
        }
        live.extend(s.open);
    }
    assert_eq!(live, rebuild(&events));
}

#[test]
fn test_an_ask_keeps_the_need_its_event_names() {
    let mut asked = event("asked", "b", "build/t1-1", Some("sign the build"));
    asked.data = Some(json!({"need": "access"}));
    let rows = rebuild(&[event("claimed", "a", "build/t1-1", None), asked]);
    assert_eq!(rows[1].need.as_deref(), Some("access"));
    assert_eq!(rows[0].need, None);
}

#[test]
fn test_the_failure_limit_assigns_the_item_at_the_count_not_before() {
    assert!(!past_failure_limit(1, 2));
    assert!(past_failure_limit(2, 2));
}

#[test]
fn test_a_halt_needs_the_last_three_ended_attempts_all_failed() {
    use Outcome::{Failed, Landed};
    assert!(halt(&[Failed, Failed]).is_none());
    assert!(halt(&[Failed, Failed, Landed]).is_none());
    assert!(halt(&[Landed, Failed, Failed]).is_none());
    assert!(halt(&[Failed, Failed, Failed, Landed]).is_some());
}

#[test]
fn test_rebuild_keeps_a_job_reports_start_end_and_exit_on_the_open_claim() {
    let mut report = event("job_reported", "2026-01-02T01:00:00Z", "build/t1-1", None);
    report.data = Some(json!({
        "start": "2026-01-02T00:10:00Z", "end": "2026-01-02T00:50:00Z", "exit": 2
    }));
    let rows = rebuild(&[
        event("claimed", "2026-01-02T00:00:00Z", "build/t1-1", None),
        report,
    ]);
    let a = &rows[0];
    assert_eq!(
        (
            a.job_started_at.as_deref(),
            a.job_ended_at.as_deref(),
            a.job_exit
        ),
        (
            Some("2026-01-02T00:10:00Z"),
            Some("2026-01-02T00:50:00Z"),
            Some(2)
        )
    );
}
