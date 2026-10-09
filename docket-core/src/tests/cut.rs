use std::collections::BTreeMap;

use super::*;

fn snap(n: usize, items: &[(&str, Option<&str>)]) -> Snapshot {
    Snapshot {
        sha: format!("s{n}"),
        date: format!("2026-01-0{n}T00:00:00Z"),
        items: items
            .iter()
            .map(|(i, p)| Landed {
                item: (*i).to_string(),
                plan: p.map(str::to_string),
            })
            .collect(),
    }
}

fn open(pairs: &[(&str, usize)]) -> BTreeMap<String, usize> {
    pairs.iter().map(|(p, n)| ((*p).to_string(), *n)).collect()
}

fn ends(groups: &[Group]) -> Vec<&str> {
    groups.iter().map(|g| g.end.as_str()).collect()
}

#[test]
fn test_interleaved_plans_cut_where_the_later_one_finishes() {
    let snaps = [
        snap(1, &[("T1", Some("A1"))]),
        snap(2, &[("T2", Some("A2"))]),
        snap(3, &[("T3", Some("A1"))]),
        snap(4, &[("T4", Some("A2"))]),
    ];
    let groups = cut(&snaps, &open(&[("A1", 0), ("A2", 0)]), None);
    // A1 finishes at s3 and A2 at s4: two groups, the first ending at s3.
    assert_eq!(ends(&groups), ["s3", "s4"]);
    assert_eq!(groups[0].finished, ["A1"]);
    assert_eq!(groups[0].partial, ["A2"]);
    assert_eq!(groups[1].finished, ["A2"]);
    assert_eq!(groups[1].partial.len(), 0);
    assert_eq!(groups[0].date, "2026-01-03T00:00:00Z");
}

#[test]
fn test_a_plan_finishing_alone_gets_its_own_group() {
    let snaps = [
        snap(1, &[("T1", Some("A1"))]),
        snap(2, &[("T2", Some("A2"))]),
        snap(3, &[("T3", Some("A3"))]),
    ];
    let groups = cut(&snaps, &open(&[("A1", 0), ("A2", 0), ("A3", 0)]), None);
    assert_eq!(ends(&groups), ["s1", "s2", "s3"]);
}

#[test]
fn test_a_plan_open_at_the_tip_rides_in_the_last_group_as_partial() {
    let snaps = [
        snap(1, &[("T1", Some("A1"))]),
        snap(2, &[("T2", Some("A2"))]),
        snap(3, &[("T3", None)]),
    ];
    let groups = cut(&snaps, &open(&[("A1", 0), ("A2", 2)]), None);
    assert_eq!(ends(&groups), ["s1", "s3"]);
    assert_eq!(groups[1].partial, ["A2"]);
    assert_eq!(groups[1].finished.len(), 0);
}

#[test]
fn test_a_cap_merges_adjacent_groups_and_keeps_the_tip() {
    let snaps = [
        snap(1, &[("T1", Some("A1"))]),
        snap(2, &[("T2", Some("A2"))]),
        snap(3, &[("T3", Some("A3"))]),
        snap(4, &[("T4", Some("A4"))]),
        snap(5, &[("T5", Some("A5"))]),
    ];
    let all = open(&[("A1", 0), ("A2", 0), ("A3", 0), ("A4", 0), ("A5", 0)]);
    assert_eq!(cut(&snaps, &all, None).len(), 5);
    let groups = cut(&snaps, &all, Some(2));
    assert_eq!(groups.len(), 2);
    assert_eq!(groups.last().unwrap().end, "s5");
    let mut finished: Vec<&String> = groups.iter().flat_map(|g| &g.finished).collect();
    finished.sort();
    assert_eq!(finished, ["A1", "A2", "A3", "A4", "A5"]);
}

#[test]
fn test_groups_never_reorder_snapshots() {
    let snaps = [
        snap(1, &[("T1", Some("A2"))]),
        snap(2, &[("T2", Some("A1"))]),
        snap(3, &[("T3", Some("A2"))]),
        snap(4, &[]),
    ];
    let groups = cut(&snaps, &open(&[("A1", 0), ("A2", 0)]), Some(3));
    let order: Vec<&str> = ends(&groups);
    assert_eq!(order, ["s2", "s3", "s4"]);
}

#[test]
fn test_no_snapshots_give_no_groups() {
    assert_eq!(cut(&[], &open(&[]), Some(1)).len(), 0);
}
