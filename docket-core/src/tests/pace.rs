use super::*;

fn logged<'a>(at: i64, id: &'a str, kind: &'a str, note: Option<&'a str>) -> Logged<'a> {
    Logged { at, id, kind, note }
}

fn closed(at: i64) -> Move {
    Move {
        at,
        delta: -1,
        id: "B1".into(),
        verb: "closed",
    }
}

#[test]
fn test_moves_map_kinds_to_verbs_and_skip_edits() {
    let events = [
        logged(1, "B1", "opened", None),
        logged(2, "B1", "edited", None),
        logged(3, "B1", "claimed", None),
        logged(4, "B1", "asked", None),
        logged(5, "B1", "closed", None),
        logged(6, "B2", "dropped", None),
    ];
    let got: Vec<(i64, &str)> = moves(&events).iter().map(|m| (m.delta, m.verb)).collect();
    assert_eq!(
        got,
        [
            (1, "opened"),
            (0, "claimed"),
            (0, "parked"),
            (-1, "closed"),
            (-1, "dropped")
        ]
    );
}

#[test]
fn test_moves_count_a_legacy_close_once() {
    let events = [
        logged(1, "B1", "legacy", Some("closed")),
        logged(2, "B1", "legacy", Some("DONE")),
        logged(3, "B2", "legacy", Some("BUILT")),
    ];
    let got: Vec<String> = moves(&events).into_iter().map(|m| m.id).collect();
    assert_eq!(got, ["B1", "B2"]);
}

#[test]
fn test_pace_reads_the_last_closes_and_caps_a_long_gap() {
    // Closes every 10 minutes, then an overnight stop of 10 hours before now.
    let moves: Vec<Move> = (0..4).map(|i| closed(i * 600)).collect();
    let now = 3 * 600 + 36_000;
    let p = pace(&moves, 20, now);
    assert_eq!(p.closed, 4);
    // Three gaps of 600, the stop capped at max(6 * 600, 1800) = 3600.
    assert_eq!(p.working, 3 * 600 + 3600);
    assert_eq!(p.per_hour(), Some(3));
}

#[test]
fn test_pace_keeps_to_the_recent_closes() {
    let moves: Vec<Move> = (0..10).map(|i| closed(i * 60)).collect();
    assert_eq!(pace(&moves, 3, 600).closed, 3);
}

#[test]
fn test_pace_with_no_moves_has_no_rate() {
    let p = pace(&[], 20, 1000);
    assert_eq!(p, Pace::default());
    assert_eq!(p.per_hour(), None);
}

#[test]
fn test_minutes_walk_back_from_the_open_count() {
    let mut moves = vec![closed(60), closed(70)];
    moves.push(Move {
        at: 130,
        delta: 1,
        id: "B9".into(),
        verb: "opened",
    });
    let rows = minutes(&moves, 10);
    assert_eq!(rows.len(), 2);
    // Newest first: 9 -> 10 with B9 opened, then 11 -> 9 with two closes.
    assert_eq!((rows[0].before, rows[0].after), (9, 10));
    assert_eq!((rows[1].before, rows[1].after), (11, 9));
    assert_eq!(rows[1].verbs, [("closed", vec!["B1".into(), "B1".into()])]);
}

#[test]
fn test_duration_reads_at_each_scale() {
    assert_eq!(duration(-5), "0s");
    assert_eq!(duration(45), "45s");
    assert_eq!(duration(720), "12m");
    assert_eq!(duration(3600 * 3 + 300), "3h 05m");
    assert_eq!(duration(86400 * 2 + 3600 * 4), "2d 4h");
}

#[test]
fn test_epoch_reads_docket_stamps_only() {
    assert_eq!(epoch("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(epoch("2026-10-01T09:29:41Z"), Some(1_790_846_981));
    assert_eq!(epoch("2000-02-29T00:00:00Z"), Some(951_782_400));
    assert_eq!(epoch("o01"), None);
    assert_eq!(epoch("2026-13-01T00:00:00Z"), None);
}
