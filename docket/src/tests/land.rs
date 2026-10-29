use docket_core::assignment::{Land, Stage};

use super::*;

fn claim(id: &str, word: &str, land: Option<(Stage, &str, Option<&str>)>) -> Row {
    Row {
        id: id.into(),
        word: word.into(),
        state: "open".into(),
        claim_branch: Some(format!("lead/{}", id.to_lowercase())),
        claim_land: land.map(|(stage, at, batch)| Land {
            stage,
            at: at.into(),
            note: None,
            batch: batch.map(str::to_string),
        }),
        ..Row::default()
    }
}

fn ids(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|r| r.id.as_str()).collect()
}

#[test]
fn test_with_none_named_only_a_build_to_land_and_not_yet_queued_is_queued() {
    let rows = [
        claim("T1", "to land", None),
        claim(
            "T2",
            "to land",
            Some((Stage::Queued, "2026-01-01T00:00:00Z", None)),
        ),
        claim("T3", "in progress", None),
        claim(
            "T4",
            "to land",
            Some((Stage::GateFailed, "2026-01-01T00:00:00Z", Some("b"))),
        ),
        claim("T5", "job failed", None),
        claim("T6", "to land", None),
    ];
    assert_eq!(unqueued(&rows), ["T1", "T6"]);
}

#[test]
fn test_a_batch_takes_the_oldest_queued_builds_up_to_the_cap_whose_branch_is_here() {
    let rows = [
        claim(
            "T1",
            "to land",
            Some((Stage::Queued, "2026-01-01T00:00:03Z", None)),
        ),
        claim(
            "T2",
            "to land",
            Some((Stage::Queued, "2026-01-01T00:00:01Z", None)),
        ),
        claim(
            "T3",
            "to land",
            Some((Stage::Gating, "2026-01-01T00:00:00Z", Some("b"))),
        ),
        claim(
            "T4",
            "to land",
            Some((Stage::Queued, "2026-01-01T00:00:02Z", None)),
        ),
        claim(
            "T5",
            "to land",
            Some((Stage::Queued, "2026-01-01T00:00:00Z", None)),
        ),
        claim(
            "T6",
            "to land",
            Some((Stage::Queued, "2026-01-01T00:00:01Z", None)),
        ),
    ];
    let here = |branch: &str| branch != "lead/t5";
    assert_eq!(ids(&batch(&rows, 3, here)), ["T2", "T6", "T4"]);
    assert_eq!(ids(&batch(&rows, 9, here)), ["T2", "T6", "T4", "T1"]);
    assert_eq!(ids(&batch(&rows, 3, |_| false)), Vec::<&str>::new());
}

#[test]
fn test_a_build_sent_back_leaves_the_rest_of_its_failed_batch_to_requeue() {
    let failed = |id, batch| claim(id, "to land", Some((Stage::GateFailed, "t", Some(batch))));
    let rows = [
        failed("T1", "b1"),
        failed("T2", "b1"),
        failed("T3", "b2"),
        claim("T4", "to land", Some((Stage::Queued, "t", None))),
        failed("T5", "b1"),
    ];
    assert_eq!(rest_of_batch(&rows, "T1", "b1"), ["T2", "T5"]);
    assert_eq!(rest_of_batch(&rows, "T3", "b2"), Vec::<&str>::new());
}

#[test]
fn test_a_merge_is_made_under_its_commit_subject() {
    assert_eq!(merge_message("Write the parser"), "Merge Write the parser");
}

#[test]
fn test_each_checkout_has_its_own_gate_run_directory() {
    let a = run_dir(Path::new("/s"), "o/p", Path::new("/home/x/src/p"));
    let b = run_dir(Path::new("/s"), "o/p", Path::new("/home/x/src/q"));
    assert_ne!(a, b);
    assert!(a.starts_with("/s/o-p"), "{}", a.display());
    assert!(
        a.file_name().unwrap().to_string_lossy().starts_with('.'),
        "no job is named like it: {}",
        a.display()
    );
}

#[test]
fn test_the_failed_gate_note_is_the_logs_last_lines() {
    let log = (1..=30)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let got = log_tail(&log, 3);
    assert_eq!(got, "line 28\nline 29\nline 30");
}

#[test]
fn test_gating_builds_here_are_queued_again_only_while_the_checkout_lock_is_free() {
    let rows = [
        claim(
            "T1",
            "to land",
            Some((Stage::Gating, "2026-01-01T00:00:00Z", Some("b"))),
        ),
        claim(
            "T2",
            "to land",
            Some((Stage::Queued, "2026-01-01T00:00:00Z", None)),
        ),
        claim(
            "T3",
            "to land",
            Some((Stage::Gating, "2026-01-01T00:00:00Z", Some("b"))),
        ),
    ];
    let here = |b: &str| b != "lead/t3";
    assert_eq!(stranded(&rows, true, here), ["T1"]);
    assert_eq!(stranded(&rows, false, here), Vec::<&str>::new());
}

#[test]
fn test_a_run_starts_when_the_queue_holds_builds_even_if_none_was_queued_now() {
    let queued = claim(
        "T2",
        "to land",
        Some((Stage::Queued, "2026-01-01T00:00:00Z", None)),
    );
    assert!(run_wanted(&[], &[queued]));
    assert!(run_wanted(&["T1".to_string()], &[]));
    assert!(!run_wanted(&[], &[]));
}

#[test]
fn test_the_builds_of_a_dead_batch_still_gating_are_those_picked() {
    let rows = [
        claim(
            "T1",
            "to land",
            Some((Stage::Gating, "2026-01-01T00:00:00Z", Some("b"))),
        ),
        claim(
            "T2",
            "to land",
            Some((Stage::Gating, "2026-01-01T00:00:00Z", Some("b"))),
        ),
        claim(
            "T3",
            "to land",
            Some((Stage::Queued, "2026-01-01T00:00:00Z", None)),
        ),
    ];
    let picked = ["T2".to_string(), "T3".to_string()];
    assert_eq!(still_gating(&rows, &picked), ["T2"]);
}
