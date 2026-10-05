use std::collections::BTreeMap;

use super::*;

fn holds(pairs: &[(i64, i64)]) -> BTreeMap<i64, Vec<i64>> {
    let mut out: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for (from, to) in pairs {
        out.entry(*from).or_default().push(*to);
    }
    out
}

#[test]
fn test_a_cycle_through_two_containers_and_their_members_is_found() {
    // Two packages each wait for their members; each holds a member that waits on the other.
    let edges = holds(&[(10, 11), (11, 20), (20, 21), (21, 10)]);
    assert_eq!(
        cycles(&edges).into_iter().collect::<Vec<_>>(),
        [10, 11, 20, 21]
    );
}

#[test]
fn test_a_chain_that_ends_is_no_cycle_and_a_tail_into_one_is_left_out() {
    assert!(cycles(&holds(&[(1, 2), (2, 3), (4, 2)])).is_empty());
    let edges = holds(&[(9, 1), (1, 2), (2, 1)]);
    assert_eq!(cycles(&edges).into_iter().collect::<Vec<_>>(), [1, 2]);
    assert_eq!(
        cycles(&holds(&[(5, 5)])).into_iter().collect::<Vec<_>>(),
        [5]
    );
}

#[test]
fn test_an_item_held_by_one_in_a_later_release_is_found() {
    // A container in the current release holds a later member; an item waits on a later item.
    let edges = holds(&[(10, 11), (10, 12), (20, 21), (30, 31)]);
    let rank = BTreeMap::from([
        (10, 0),
        (11, 0),
        (12, 1),
        (20, 0),
        (21, 2),
        (30, 1),
        (31, 0),
    ]);
    assert_eq!(held_later(&edges, &rank), [(10, 12), (20, 21)]);
}

#[test]
fn test_a_hold_runs_into_a_later_release_only_when_the_holder_ships_after_the_held() {
    let releases = ["0.4.1".to_string(), "0.4.2".to_string()];
    assert!(runs_later(&releases, Some("0.4.1"), Some("0.4.2")));
    assert!(!runs_later(&releases, Some("0.4.2"), Some("0.4.2")));
    assert!(!runs_later(&releases, Some("0.4.2"), Some("0.4.1")));
    assert!(runs_later(&releases, Some("upkeep"), Some("0.4.2")));
    assert!(!runs_later(&[], Some("0.4.1"), Some("0.4.2")));
}

const DAY: i64 = 86_400;

fn at(kind: &'static str, days_ago: i64) -> (&'static str, i64) {
    (kind, 100 * DAY - days_ago * DAY)
}

#[test]
fn test_an_edit_does_not_count_as_activity_but_an_unclaim_does() {
    let now = 100 * DAY;
    let edited = [at("claimed", 10), at("edited", 1)];
    assert_eq!((now - idle_since(&edited).unwrap()) / DAY, 10);
    let released = [at("claimed", 10), at("released", 1)];
    assert_eq!((now - idle_since(&released).unwrap()) / DAY, 1);
}

#[test]
fn test_only_events_that_move_an_item_forward_count() {
    for kind in ["edited", "renumbered", "claim_lost", "queue", "audited"] {
        assert_eq!(idle_since(&[at("opened", 5), at(kind, 1)]), Some(95 * DAY));
    }
    for kind in [
        "opened", "claimed", "released", "waited", "resumed", "asked", "replied", "decided",
        "closed", "dropped", "reopened",
    ] {
        assert_eq!(idle_since(&[at("opened", 5), at(kind, 1)]), Some(99 * DAY));
    }
    assert_eq!(idle_since(&[at("edited", 1)]), None);
}
