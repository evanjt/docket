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
fn test_pace_caps_at_six_of_an_even_median_and_rounds_half_to_even() {
    // Gaps 500 and 601 and the stop to now: median of 500, 601, 40000 is 601, cap 3606.
    let moves = [closed(0), closed(500), closed(1101)];
    assert_eq!(pace(&moves, 20, 41_101).working, 500 + 601 + 3606);
    // Gaps 400, 801, 100 and 3602: the median is 600.5, so the cap is 3603 and the stop counts whole.
    let p = pace(
        &[closed(0), closed(400), closed(1201), closed(1301)],
        20,
        4903,
    );
    assert_eq!(p.working, 400 + 801 + 100 + 3602);
    // 1 close in 7200 seconds is 0.5 an hour, which rounds to 0; 3 in 7200 is 1.5, which rounds to 2.
    let half = Pace {
        closed: 1,
        opened: 0,
        working: 7200,
    };
    assert_eq!(half.per_hour(), None);
    let three = Pace {
        closed: 3,
        opened: 0,
        working: 7200,
    };
    assert_eq!(three.per_hour(), Some(2));
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

#[test]
fn test_net_counts_current_code_against_its_opens_and_the_research_that_opened_nothing() {
    let code = Class::Code { current: true };
    let later = Class::Code { current: false };
    let events = [
        Counted {
            at: 50,
            kind: "closed",
            class: code,
        },
        Counted {
            at: 100,
            kind: "closed",
            class: Class::Research { opened: true },
        },
        Counted {
            at: 101,
            kind: "opened",
            class: code,
        },
        Counted {
            at: 102,
            kind: "opened",
            class: code,
        },
        Counted {
            at: 103,
            kind: "opened",
            class: later,
        },
        Counted {
            at: 110,
            kind: "closed",
            class: Class::Research { opened: false },
        },
        Counted {
            at: 120,
            kind: "closed",
            class: code,
        },
        Counted {
            at: 130,
            kind: "dropped",
            class: code,
        },
        Counted {
            at: 140,
            kind: "reopened",
            class: code,
        },
        Counted {
            at: 150,
            kind: "claimed",
            class: code,
        },
        Counted {
            at: 160,
            kind: "closed",
            class: Class::Other,
        },
    ];
    let n = net(&events, 100);
    assert_eq!(
        n,
        Net {
            closed: 2,
            opened: 3,
            idle: 1
        }
    );
    assert_eq!(n.net(), 1);
    assert_eq!(
        n.line("the last hour"),
        "the last hour: current-release tickets 2 closed, 3 opened, net +1; 1 research close opened nothing"
    );
    assert_eq!(
        Net {
            closed: 4,
            opened: 1,
            idle: 0
        }
        .line("the last hour"),
        "the last hour: current-release tickets 4 closed, 1 opened, net -3"
    );
    assert_eq!(Net::default().line("the last hour"), "");
}
