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
fn test_concepts_of_reads_ties_either_way_and_up_through_parents() {
    // 10 and 11 are concepts. 1 relates to 10; 2 is a child of 1 and was spawned by 3, which is tied
    // from 11; 4 stands alone.
    let ties = [related(1, 10), parent(2, 1), origin(2, 3), related(11, 3)];
    let index = Neighbours::new(&ties);
    let concepts = set(&[10, 11]);
    assert_eq!(index.concepts_of(&concepts, 1), set(&[10]));
    assert_eq!(index.concepts_of(&concepts, 2), set(&[10]));
    assert_eq!(index.concepts_of(&concepts, 3), set(&[11]));
    assert_eq!(index.concepts_of(&concepts, 4), set(&[]));
}

#[test]
fn test_concepts_of_a_concept_leaves_itself_out_and_stops_at_a_cycle() {
    let ties = [related(10, 11), parent(1, 2), parent(2, 1)];
    let index = Neighbours::new(&ties);
    let concepts = set(&[10, 11]);
    assert_eq!(index.concepts_of(&concepts, 10), set(&[11]));
    assert_eq!(index.concepts_of(&concepts, 1), set(&[]));
}
