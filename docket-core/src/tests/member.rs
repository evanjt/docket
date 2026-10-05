use super::*;

fn parent(rid: i64, to: i64) -> Tie {
    Tie {
        rid,
        edge: Edge::Parent,
        to,
    }
}

fn related(rid: i64, to: i64) -> Tie {
    Tie {
        rid,
        edge: Edge::Related,
        to,
    }
}

fn origin(rid: i64, to: i64) -> Tie {
    Tie {
        rid,
        edge: Edge::Origin,
        to,
    }
}

fn set(rids: &[i64]) -> HashSet<i64> {
    rids.iter().copied().collect()
}

#[test]
fn test_descendants_follow_parents_at_any_depth_and_origin_adds_nothing() {
    // 1 is a plan over 2, which is a plan over 3; 4 was spawned by 1 and 5 by 3, with no parent.
    let ties = [parent(2, 1), parent(3, 2), origin(4, 1), origin(5, 3)];
    assert_eq!(descendants(&ties, 1), set(&[2, 3]));
    assert_eq!(descendants(&ties, 2), set(&[3]));
    assert_eq!(descendants(&ties, 4), set(&[]));
}

#[test]
fn test_descendants_skip_a_cycle_and_a_related_tie() {
    let ties = [parent(2, 1), parent(3, 2), parent(1, 3), related(4, 1)];
    assert_eq!(descendants(&ties, 1), set(&[2, 3]));
    assert_eq!(descendants(&ties, 4), set(&[]));
}

#[test]
fn test_ancestors_run_nearest_first_and_stop_at_a_cycle() {
    let ties = [
        parent(3, 2),
        parent(2, 1),
        origin(1, 9),
        parent(5, 6),
        parent(6, 5),
    ];
    assert_eq!(ancestors(&ties, 3), [2, 1]);
    assert_eq!(ancestors(&ties, 1), Vec::<i64>::new());
    assert_eq!(ancestors(&ties, 5), [6]);
}

#[test]
fn test_members_of_reads_plain_ties_both_ways_and_the_children_below() {
    // 10 is a concept; 1 relates to it, 2 is a child of 1, 3 was spawned by the concept, 11 is another
    // concept, 5 a child of 3.
    let ties = [
        related(1, 10),
        parent(2, 1),
        origin(3, 10),
        related(11, 10),
        parent(5, 3),
    ];
    let standing = set(&[10, 11]);
    assert_eq!(members_of(&ties, &standing, 10), set(&[1, 2, 3, 5]));
    assert_eq!(members_of(&ties, &standing, 11), set(&[]));
}

#[test]
fn test_members_of_survives_a_parent_cycle_and_an_empty_graph() {
    let ties = [parent(1, 2), parent(2, 1), related(1, 9)];
    let standing = set(&[9]);
    assert_eq!(members_of(&ties, &standing, 9), set(&[1, 2]));
    assert_eq!(members_of(&[], &standing, 9), set(&[]));
}

#[test]
fn test_an_item_under_a_labelled_plan_carries_the_label() {
    // 1 is a plan over 2, which is a plan over 3; 4 sits under no plan and 5 is only related to 1.
    let ties = [parent(2, 1), parent(3, 2), related(5, 1)];
    let own = HashMap::from([
        (1, vec!["area:sync".to_string()]),
        (2, vec!["slow".to_string(), "area:sync".to_string()]),
        (4, vec!["lone".to_string()]),
    ]);
    assert_eq!(labels_carried(&ties, &own, 3), ["slow", "area:sync"]);
    assert_eq!(labels_carried(&ties, &own, 2), ["slow", "area:sync"]);
    assert_eq!(labels_carried(&ties, &own, 1), ["area:sync"]);
    assert_eq!(labels_carried(&ties, &own, 4), ["lone"]);
    assert!(labels_carried(&ties, &own, 5).is_empty());
}

fn totals(rows: &[(i64, Option<i64>, bool)]) -> Vec<(i64, u64, u64)> {
    let mut out: Vec<(i64, u64, u64)> = member_totals(rows)
        .into_iter()
        .map(|(rid, m)| (rid, m.open, m.closed))
        .collect();
    out.sort_unstable();
    out
}

#[test]
fn test_member_totals_count_every_depth_under_each_plan() {
    // 1 holds 2 and 3; 2 holds 4, which is closed; 5 is a second plan over 6; 7 stands alone.
    let rows = [
        (1, None, true),
        (2, Some(1), true),
        (3, Some(1), false),
        (4, Some(2), false),
        (5, None, true),
        (6, Some(5), true),
        (7, None, true),
    ];
    assert_eq!(totals(&rows), [(1, 1, 2), (2, 0, 1), (5, 1, 0)]);
}

#[test]
fn test_member_totals_survive_a_parent_cycle_and_no_rows() {
    let rows = [(1, Some(2), true), (2, Some(1), false), (3, Some(1), true)];
    assert_eq!(totals(&rows), [(1, 1, 1), (2, 2, 0)]);
    assert!(member_totals(&[]).is_empty());
}
